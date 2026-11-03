use std::sync::mpsc::Receiver;
use std::time::Instant;

use rstest::rstest;
use windows_sys::Win32::Foundation::TRUE;
use windows_sys::Win32::System::Console::CTRL_C_EVENT;
use windows_sys::Win32::System::Threading::GetProcessShutdownParameters;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsWindow, IsWindowVisible, PostThreadMessageW, SendMessageW, WM_QUIT,
    WM_USER,
};
use windows_sys::core::BOOL;

use super::*;
use crate::os::process::windows::{Answer, answer};

const SHORT: Duration = Duration::from_millis(200);

fn slot() -> (&'static Mutex<Option<Stop>>, Receiver<()>) {
    let (stopped, stops) = mpsc::channel();
    let stop: Stop = Box::new(move || {
        let _ = stopped.send(());
    });
    (Box::leak(Box::new(Mutex::new(Some(stop)))), stops)
}

mod replies {
    use super::*;

    #[rstest]
    #[case::the_question(WM_QUERYENDSESSION, 0, Reply::Allow, 0)]
    #[case::the_end(WM_ENDSESSION, 1, Reply::Wait, 1)]
    #[case::an_end_called_off(WM_ENDSESSION, 0, Reply::Handled, 0)]
    #[case::a_close(WM_CLOSE, 0, Reply::Handled, 1)]
    #[case::another_message(WM_USER, 1, Reply::Default, 0)]
    fn each_message_gets_its_reply(
        #[case] message: u32,
        #[case] wparam: WPARAM,
        #[case] expected: Reply,
        #[case] stops: usize,
    ) {
        let (stop, stopped) = slot();

        assert_eq!(reply(message, wparam, stop), expected);
        assert_eq!(stopped.try_iter().count(), stops);
    }

    #[test]
    fn the_stop_runs_once() {
        let (stop, stopped) = slot();

        assert_eq!(reply(WM_CLOSE, 0, stop), Reply::Handled);
        assert_eq!(reply(WM_ENDSESSION, 1, stop), Reply::Wait);
        assert_eq!(stopped.try_iter().count(), 1);
    }

    #[test]
    fn an_end_after_a_ctrl_c_waits_for_the_stop_under_way() {
        let (stop, stopped) = slot();

        assert_eq!(answer(CTRL_C_EVENT, false, stop), Answer::Handled);
        assert_eq!(reply(WM_ENDSESSION, 1, stop), Reply::Wait);
        assert_eq!(stopped.try_iter().count(), 1);
    }
}

mod window {
    use super::*;

    struct Opened {
        window: HWND,
        pump: u32,
        thread: Option<JoinHandle<()>>,
        stopped: Receiver<()>,
    }

    impl Opened {
        fn new() -> Self {
            Self::holding(SHORT)
        }

        fn holding(wait: Duration) -> Self {
            let (stop, stopped) = slot();
            let (window, thread) = open(Ending { stop, wait }).expect("opened");
            let window = ptr::with_exposed_provenance_mut(window);
            let pump = unsafe { GetWindowThreadProcessId(window, ptr::null_mut()) };
            assert_ne!(pump, 0, "no thread for the window");
            Self { window, pump, thread: Some(thread), stopped }
        }

        fn send(&self, message: u32, wparam: WPARAM) -> LRESULT {
            unsafe { SendMessageW(self.window, message, wparam, 0) }
        }

        fn stops(&self) -> usize {
            self.stopped.try_iter().count()
        }
    }

    impl Drop for Opened {
        fn drop(&mut self) {
            let quit = unsafe { PostThreadMessageW(self.pump, WM_QUIT, 0, 0) } != 0;
            if let Some(thread) = self.thread.take().filter(|_| quit) {
                let _ = thread.join();
            }
        }
    }

    unsafe extern "system" fn listed(window: HWND, into: LPARAM) -> BOOL {
        let top_level = unsafe { &mut *ptr::with_exposed_provenance_mut::<Vec<HWND>>(into.cast_unsigned()) };
        top_level.push(window);
        TRUE
    }

    fn top_level_windows() -> Vec<HWND> {
        let mut top_level: Vec<HWND> = Vec::new();
        unsafe { EnumWindows(Some(listed), (&raw mut top_level).expose_provenance().cast_signed()) };
        top_level
    }

    #[test]
    fn the_question_is_allowed_without_calling_the_stop() {
        let opened = Opened::new();

        assert_eq!(opened.send(WM_QUERYENDSESSION, 0), 1);
        assert_eq!(opened.stops(), 0);
    }

    #[test]
    fn the_end_calls_the_stop_and_answers_only_after_waiting() {
        let opened = Opened::new();
        let sent = Instant::now();

        assert_eq!(opened.send(WM_ENDSESSION, 1), 0);
        assert!(sent.elapsed() >= SHORT, "it answered after {:?}", sent.elapsed());
        assert_eq!(opened.stops(), 1);
    }

    #[test]
    fn an_end_called_off_neither_calls_the_stop_nor_waits() {
        let opened = Opened::holding(Duration::from_secs(30));
        let sent = Instant::now();

        assert_eq!(opened.send(WM_ENDSESSION, 0), 0);
        assert!(sent.elapsed() < Duration::from_secs(10), "it answered after {:?}", sent.elapsed());
        assert_eq!(opened.stops(), 0);
    }

    #[test]
    fn a_close_calls_the_stop_and_keeps_the_window() {
        let opened = Opened::new();

        assert_eq!(opened.send(WM_CLOSE, 0), 0);
        assert_eq!(opened.stops(), 1);
        assert_ne!(unsafe { IsWindow(opened.window) }, 0, "the window was destroyed");
    }

    #[test]
    fn it_is_a_hidden_top_level_window_which_windows_asks_at_the_end_of_a_session() {
        let opened = Opened::new();

        assert!(top_level_windows().contains(&opened.window), "not a top-level window");
        assert_eq!(unsafe { IsWindowVisible(opened.window) }, 0, "it shows");
    }

    #[test]
    fn two_windows_open_with_one_class() {
        let first = Opened::new();
        let second = Opened::new();

        assert_ne!(first.window, second.window);
    }
}

mod order {
    use super::*;

    #[test]
    fn the_server_is_asked_before_the_programs_it_started() {
        ask_first();

        let (mut level, mut flags) = (0, 0);
        assert_ne!(unsafe { GetProcessShutdownParameters(&raw mut level, &raw mut flags) }, 0);
        assert_eq!(level, FIRST);
        assert!(level > 0x280, "the level every process starts at");
    }
}
