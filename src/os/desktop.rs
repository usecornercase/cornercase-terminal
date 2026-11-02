pub(crate) use imp::{Awake, detach, play, shell_open};

#[cfg(test)]
#[cfg(windows)]
pub(crate) use imp::system_required;

#[cfg(windows)]
mod imp {
    use std::ffi::OsStr;
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::Command;
    use std::ptr::{null, null_mut};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    use parking_lot::Mutex;
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME, SND_NODEFAULT};
    use windows_sys::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
    };
    use windows_sys::Win32::System::Power::{ES_CONTINUOUS, ES_SYSTEM_REQUIRED, SetThreadExecutionState};
    use windows_sys::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW};
    use windows_sys::Win32::UI::Shell::{SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    static PLAYED: Mutex<u64> = Mutex::new(0);

    pub(crate) fn detach(command: &mut Command) -> &mut Command {
        command.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP)
    }

    fn wide(text: &OsStr) -> io::Result<Vec<u16>> {
        let mut wide: Vec<u16> = text.encode_wide().collect();
        if wide.contains(&0) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "it holds a NUL character"));
        }
        wide.push(0);
        Ok(wide)
    }

    fn pointer(text: Option<&[u16]>) -> *const u16 {
        text.map_or(null(), <[u16]>::as_ptr)
    }

    pub(crate) fn shell_open(verb: Option<&str>, file: &OsStr, parameters: Option<&OsStr>) -> io::Result<()> {
        let verb = verb.map(|verb| wide(OsStr::new(verb))).transpose()?;
        let file = wide(file)?;
        let parameters = parameters.map(wide).transpose()?;
        let mut info = SHELLEXECUTEINFOW {
            cbSize: u32::try_from(size_of::<SHELLEXECUTEINFOW>()).map_err(io::Error::other)?,
            fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
            lpVerb: pointer(verb.as_deref()),
            lpFile: file.as_ptr(),
            lpParameters: pointer(parameters.as_deref()),
            nShow: SW_SHOWNORMAL,
            ..SHELLEXECUTEINFOW::default()
        };
        let com =
            unsafe { CoInitializeEx(null(), (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).cast_unsigned()) };
        let opened = unsafe { ShellExecuteExW(&raw mut info) } != 0;
        let result = if opened { Ok(()) } else { Err(io::Error::last_os_error()) };
        if com >= 0 {
            unsafe { CoUninitialize() };
        }
        result
    }

    pub(crate) fn play(file: &Path, limit: Duration) -> io::Result<()> {
        let this = start(&wide(file.as_os_str())?)?;
        thread::sleep(limit);
        let played = PLAYED.lock();
        if *played == this {
            unsafe { PlaySoundW(null(), null_mut(), 0) };
        }
        Ok(())
    }

    fn start(file: &[u16]) -> io::Result<u64> {
        let mut played = PLAYED.lock();
        let started = unsafe { PlaySoundW(file.as_ptr(), null_mut(), SND_FILENAME | SND_ASYNC | SND_NODEFAULT) };
        if started == 0 {
            return Err(io::Error::other("PlaySound could not play it"));
        }
        *played += 1;
        Ok(*played)
    }

    pub(crate) struct Awake {
        stop: Sender<()>,
        thread: JoinHandle<()>,
    }

    impl Awake {
        pub(crate) fn hold() -> io::Result<Self> {
            let (stop, stopped) = mpsc::channel();
            let (told, held) = mpsc::channel();
            let thread = thread::Builder::new().name("keep awake".into()).spawn(move || hold_until(&stopped, &told))?;
            held.recv().map_err(io::Error::other)??;
            Ok(Self { stop, thread })
        }

        pub(crate) fn release(self) {
            drop(self.stop);
            let _ = self.thread.join();
        }
    }

    fn hold_until(stopped: &Receiver<()>, told: &Sender<io::Result<()>>) {
        let previous = unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) };
        if previous == 0 {
            let _ = told.send(Err(io::Error::last_os_error()));
            return;
        }
        let _ = told.send(Ok(()));
        let _ = stopped.recv();
        unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
    }

    #[cfg(test)]
    pub(crate) fn system_required() -> bool {
        use windows_sys::Win32::System::Power::{CallNtPowerInformation, SystemExecutionState};
        let mut state = 0u32;
        let size = u32::try_from(size_of::<u32>()).unwrap_or_default();
        let status = unsafe { CallNtPowerInformation(SystemExecutionState, null(), 0, (&raw mut state).cast(), size) };
        status == 0 && state & ES_SYSTEM_REQUIRED != 0
    }
}

#[cfg(not(windows))]
mod imp {
    use std::convert::Infallible;
    use std::ffi::OsStr;
    use std::io;
    use std::path::Path;
    use std::process::Command;
    use std::time::Duration;

    use crate::os::process::CommandExt;

    pub(crate) struct Awake(Infallible);

    impl Awake {
        pub(crate) fn hold() -> io::Result<Self> {
            Err(unsupported())
        }

        pub(crate) fn release(self) {
            match self.0 {}
        }
    }

    pub(crate) fn detach(command: &mut Command) -> &mut Command {
        command.process_group(0)
    }

    pub(crate) fn shell_open(_verb: Option<&str>, _file: &OsStr, _parameters: Option<&OsStr>) -> io::Result<()> {
        Err(unsupported())
    }

    pub(crate) fn play(_file: &Path, _limit: Duration) -> io::Result<()> {
        Err(unsupported())
    }

    fn unsupported() -> io::Error {
        io::ErrorKind::Unsupported.into()
    }
}
