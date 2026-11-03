pub(crate) use imp::{AsFd, BorrowedFd, OwnedFd, enter_raw, keep_modes, redirect_stderr, restore_modes, stdin, stdout};
#[cfg(windows)]
pub(crate) use imp::{echo_off, off_ctrl, on_ctrl, read_input, set_input_mode, wait_input};

#[cfg(unix)]
mod imp {
    use std::fs::File;
    use std::io;

    pub(crate) use rustix::stdio::{stdin, stdout};
    pub(crate) use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

    pub(crate) fn redirect_stderr(file: &File) -> io::Result<()> {
        rustix::stdio::dup2_stderr(file)?;
        Ok(())
    }

    pub(crate) fn keep_modes() {}

    #[expect(clippy::unnecessary_wraps, reason = "it keeps the Windows signature, where the console can refuse")]
    pub(crate) fn enter_raw() -> io::Result<()> {
        Ok(())
    }

    pub(crate) fn restore_modes() {}
}

#[cfg(windows)]
mod imp {
    use std::collections::VecDeque;
    use std::convert::Infallible;
    use std::fs::File;
    use std::io;
    use std::iter;
    use std::marker::PhantomData;
    use std::time::{Duration, Instant};

    use parking_lot::Mutex;
    use windows_sys::Win32::Foundation::{FALSE, HANDLE, TRUE, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Console::{
        DISABLE_NEWLINE_AUTO_RETURN, ENABLE_ECHO_INPUT, ENABLE_EXTENDED_FLAGS, ENABLE_LINE_INPUT,
        ENABLE_PROCESSED_INPUT, ENABLE_QUICK_EDIT_MODE, ENABLE_VIRTUAL_TERMINAL_INPUT,
        ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetNumberOfConsoleInputEvents, GetStdHandle, INPUT_RECORD,
        KEY_EVENT, LEFT_CTRL_PRESSED, RIGHT_CTRL_PRESSED, ReadConsoleInputW, STD_HANDLE, STD_INPUT_HANDLE,
        STD_OUTPUT_HANDLE, SetConsoleCtrlHandler, SetConsoleMode,
    };
    use windows_sys::Win32::System::Threading::{INFINITE, WaitForSingleObject};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_MENU;
    use windows_sys::core::BOOL;

    #[derive(Debug, Clone, Copy)]
    pub(crate) struct BorrowedFd<'fd>(PhantomData<&'fd ()>);

    impl BorrowedFd<'_> {
        #[expect(clippy::trivially_copy_pass_by_ref, clippy::unused_self, reason = "it keeps std's signature")]
        pub(crate) fn try_clone_to_owned(&self) -> io::Result<OwnedFd> {
            Err(io::ErrorKind::Unsupported.into())
        }
    }

    #[derive(Debug)]
    pub(crate) struct OwnedFd(Infallible);

    pub(crate) trait AsFd {
        fn as_fd(&self) -> BorrowedFd<'_>;
    }

    impl<T: ?Sized> AsFd for T {
        fn as_fd(&self) -> BorrowedFd<'_> {
            BorrowedFd(PhantomData)
        }
    }

    pub(crate) fn stdin() -> BorrowedFd<'static> {
        BorrowedFd(PhantomData)
    }

    pub(crate) fn stdout() -> BorrowedFd<'static> {
        BorrowedFd(PhantomData)
    }

    pub(crate) fn redirect_stderr(_file: &File) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }

    const READ_AT_ONCE: usize = 128;
    const NUL_KEY: u16 = 0x40;
    const HIGH_SURROGATES: std::ops::Range<u16> = 0xD800..0xDC00;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(super) struct Modes {
        pub(super) input: u32,
        pub(super) output: u32,
    }

    pub(super) fn raw(kept: Modes) -> Modes {
        let off = ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT | ENABLE_QUICK_EDIT_MODE;
        Modes {
            input: (kept.input | ENABLE_VIRTUAL_TERMINAL_INPUT | ENABLE_EXTENDED_FLAGS) & !off,
            output: kept.output | ENABLE_VIRTUAL_TERMINAL_PROCESSING | DISABLE_NEWLINE_AUTO_RETURN,
        }
    }

    pub(super) trait Handles {
        fn mode(&self, which: STD_HANDLE) -> io::Result<u32>;
        fn set_mode(&self, which: STD_HANDLE, mode: u32) -> io::Result<()>;
    }

    pub(super) trait Records {
        fn wait(&self, millis: u32) -> io::Result<bool>;
        fn read(&self) -> io::Result<Vec<INPUT_RECORD>>;
    }

    struct Real;

    fn handle(which: STD_HANDLE) -> HANDLE {
        unsafe { GetStdHandle(which) }
    }

    fn checked(done: BOOL) -> io::Result<()> {
        if done == FALSE { Err(io::Error::last_os_error()) } else { Ok(()) }
    }

    impl Handles for Real {
        fn mode(&self, which: STD_HANDLE) -> io::Result<u32> {
            let mut mode = 0;
            checked(unsafe { GetConsoleMode(handle(which), &raw mut mode) })?;
            Ok(mode)
        }

        fn set_mode(&self, which: STD_HANDLE, mode: u32) -> io::Result<()> {
            checked(unsafe { SetConsoleMode(handle(which), mode) })
        }
    }

    impl Records for Real {
        fn wait(&self, millis: u32) -> io::Result<bool> {
            match unsafe { WaitForSingleObject(handle(STD_INPUT_HANDLE), millis) } {
                WAIT_OBJECT_0 => Ok(true),
                WAIT_TIMEOUT => Ok(false),
                _ => Err(io::Error::last_os_error()),
            }
        }

        fn read(&self) -> io::Result<Vec<INPUT_RECORD>> {
            let mut queued = 0;
            checked(unsafe { GetNumberOfConsoleInputEvents(handle(STD_INPUT_HANDLE), &raw mut queued) })?;
            let length = queued.min(u32::try_from(READ_AT_ONCE).unwrap_or(1));
            if length == 0 {
                return Ok(Vec::new());
            }
            let mut records = [INPUT_RECORD::default(); READ_AT_ONCE];
            let mut read = 0;
            checked(unsafe {
                ReadConsoleInputW(handle(STD_INPUT_HANDLE), records.as_mut_ptr(), length, &raw mut read)
            })?;
            Ok(records.into_iter().take(usize::try_from(read).unwrap_or(0)).collect())
        }
    }

    static KEPT: Mutex<Option<Modes>> = Mutex::new(None);

    pub(super) fn read_modes(console: &impl Handles) -> io::Result<Modes> {
        Ok(Modes { input: console.mode(STD_INPUT_HANDLE)?, output: console.mode(STD_OUTPUT_HANDLE)? })
    }

    pub(super) fn write_modes(console: &impl Handles, modes: Modes) -> io::Result<()> {
        let input = console.set_mode(STD_INPUT_HANDLE, modes.input);
        let output = console.set_mode(STD_OUTPUT_HANDLE, modes.output);
        input.and(output)
    }

    pub(crate) fn keep_modes() {
        *KEPT.lock() = read_modes(&Real).ok();
    }

    pub(super) fn raw_from(console: &impl Handles, kept: Option<Modes>) -> io::Result<()> {
        let kept = kept
            .ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, "standard input or output is not a console"))?;
        write_modes(console, raw(kept))
    }

    pub(crate) fn enter_raw() -> io::Result<()> {
        let kept = *KEPT.lock();
        raw_from(&Real, kept)
    }

    pub(crate) fn restore_modes() {
        if let Some(kept) = *KEPT.lock() {
            let _ = write_modes(&Real, kept);
        }
    }

    pub(super) fn hide_echo(console: &impl Handles) -> Option<u32> {
        let before = console.mode(STD_INPUT_HANDLE).ok()?;
        console.set_mode(STD_INPUT_HANDLE, before & !ENABLE_ECHO_INPUT).ok()?;
        Some(before)
    }

    pub(crate) fn echo_off() -> Option<u32> {
        hide_echo(&Real)
    }

    pub(crate) fn set_input_mode(mode: u32) {
        let _ = Real.set_mode(STD_INPUT_HANDLE, mode);
    }

    #[derive(Debug)]
    pub(super) struct Pending {
        bytes: VecDeque<u8>,
        high: Option<u16>,
    }

    impl Pending {
        pub(super) const fn new() -> Self {
            Self { bytes: VecDeque::new(), high: None }
        }

        fn push(&mut self, units: impl IntoIterator<Item = u16>) {
            let mut units: Vec<u16> = self.high.take().into_iter().chain(units).collect();
            if units.last().is_some_and(|unit| HIGH_SURROGATES.contains(unit)) {
                self.high = units.pop();
            }
            self.bytes.extend(String::from_utf16_lossy(&units).into_bytes());
        }

        fn take(&mut self, buf: &mut [u8]) -> usize {
            let count = buf.len().min(self.bytes.len());
            for (to, from) in buf.iter_mut().zip(self.bytes.drain(..count)) {
                *to = from;
            }
            count
        }
    }

    static INPUT: Mutex<Pending> = Mutex::new(Pending::new());

    fn typed(record: &INPUT_RECORD) -> Option<(u16, u16)> {
        if u32::from(record.EventType) != KEY_EVENT {
            return None;
        }
        let key = unsafe { record.Event.KeyEvent };
        let unit = unsafe { key.uChar.UnicodeChar };
        let down = key.bKeyDown != FALSE;
        let character = (down || key.wVirtualKeyCode == VK_MENU) && unit != 0;
        let ctrl = key.dwControlKeyState & (LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED) != 0;
        let nul = down && ctrl && key.wVirtualKeyCode == NUL_KEY && unit == 0;
        (character || nul).then_some((unit, key.wRepeatCount.max(1)))
    }

    pub(super) fn characters(records: &[INPUT_RECORD]) -> Vec<u16> {
        records.iter().filter_map(typed).flat_map(|(unit, count)| iter::repeat_n(unit, usize::from(count))).collect()
    }

    pub(super) fn millis(left: Duration) -> u32 {
        u32::try_from(left.as_micros().div_ceil(1000)).map_or(INFINITE - 1, |millis| millis.min(INFINITE - 1))
    }

    pub(super) fn wait_with(console: &impl Records, pending: &Mutex<Pending>, wait: Duration) -> io::Result<bool> {
        let deadline = Instant::now().checked_add(wait);
        while pending.lock().bytes.is_empty() {
            let left = deadline.map_or(INFINITE, |deadline| millis(deadline.saturating_duration_since(Instant::now())));
            if !console.wait(left)? {
                return Ok(false);
            }
            let units = characters(&console.read()?);
            pending.lock().push(units);
        }
        Ok(true)
    }

    pub(super) fn read_with(console: &impl Records, pending: &Mutex<Pending>, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            wait_with(console, pending, Duration::MAX)?;
            let taken = pending.lock().take(buf);
            if taken > 0 || buf.is_empty() {
                return Ok(taken);
            }
        }
    }

    pub(crate) fn wait_input(wait: Duration) -> io::Result<bool> {
        wait_with(&Real, &INPUT, wait)
    }

    pub(crate) fn read_input(buf: &mut [u8]) -> io::Result<usize> {
        read_with(&Real, &INPUT, buf)
    }

    type OnCtrl = Box<dyn Fn(u32) -> bool + Send>;

    static CTRL: Mutex<Option<OnCtrl>> = Mutex::new(None);

    pub(super) extern "system" fn ctrl_event(event: u32) -> BOOL {
        if CTRL.lock().as_ref().is_some_and(|handled| handled(event)) { TRUE } else { FALSE }
    }

    pub(crate) fn on_ctrl(handled: impl Fn(u32) -> bool + Send + 'static) -> io::Result<()> {
        *CTRL.lock() = Some(Box::new(handled));
        checked(unsafe { SetConsoleCtrlHandler(Some(ctrl_event), TRUE) }).inspect_err(|_| *CTRL.lock() = None)
    }

    pub(crate) fn off_ctrl() {
        let _ = checked(unsafe { SetConsoleCtrlHandler(Some(ctrl_event), FALSE) });
        *CTRL.lock() = None;
    }
}

#[cfg(test)]
mod tests;
