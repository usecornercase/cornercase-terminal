use std::fs::File;
use std::io::{BufRead, BufReader};
use std::os::windows::io::AsRawHandle;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rstest::rstest;
use windows_sys::Win32::Foundation::GetHandleInformation;
use windows_sys::Win32::System::Console::GetConsoleProcessList;

use super::*;
use crate::test_util::TempDir;

mod detached {
    use super::*;

    fn console_processes() -> Vec<u32> {
        let mut list = [0u32; 256];
        let length = u32::try_from(list.len()).expect("length");
        let count = unsafe { GetConsoleProcessList(list.as_mut_ptr(), length) };
        list.into_iter().take(usize::try_from(count).expect("count")).collect()
    }

    fn flags(handle: HANDLE) -> io::Result<u32> {
        let mut flags = 0;
        checked(unsafe { GetHandleInformation(handle, &raw mut flags) }).map(|()| flags)
    }

    fn inherited(handle: HANDLE) -> bool {
        flags(handle).expect("flags") & HANDLE_FLAG_INHERIT != 0
    }

    struct Restored(Vec<(HANDLE, u32)>);

    impl Restored {
        fn standard_handles() -> Self {
            let handles =
                [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE].map(|which| unsafe { GetStdHandle(which) });
            Self(handles.into_iter().filter_map(|handle| Some((handle, flags(handle).ok()?))).collect())
        }
    }

    impl Drop for Restored {
        fn drop(&mut self) {
            for (handle, flags) in &self.0 {
                let _ = checked(unsafe { SetHandleInformation(*handle, HANDLE_FLAG_INHERIT, *flags) });
            }
        }
    }

    #[test]
    fn a_server_gets_a_hidden_console_of_its_own() {
        let _restored = Restored::standard_handles();
        let stdout = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
        checked(unsafe { SetHandleInformation(stdout, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) }).expect("inherit");
        let mut command = Command::new("cmd");
        command
            .args(["/d", "/q", "/c", "(echo hidden> CON) && echo console& echo end& set /p line="])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        let mut server = spawn_detached(&mut command).expect("spawned");
        let said: Vec<String> = BufReader::new(server.stdout.take().expect("stdout"))
            .lines()
            .map_while(Result::ok)
            .take_while(|line| line != "end")
            .collect();
        let attached = console_processes();
        drop(server.stdin.take());
        server.wait().expect("ended");

        assert_eq!(said, ["console"], "no console at all");
        assert!(attached.contains(&process::id()), "this test has no console to compare with: {attached:?}");
        assert!(!attached.contains(&server.id()), "it shares this console: {attached:?}");
        assert!(!inherited(stdout), "this process's standard output would reach the server");
    }

    #[test]
    fn a_handle_is_kept_from_the_server() {
        let tmp = TempDir::new();
        let file = File::create(tmp.path().join("log")).expect("file");
        let handle = file.as_raw_handle();
        checked(unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) }).expect("inherit");

        not_inherited(handle).expect("kept");

        assert!(!inherited(handle));
    }
}

mod launch {
    use super::*;

    #[test]
    fn the_folder_it_left_is_the_one_it_started_in() {
        let tmp = TempDir::new();

        assert_eq!(launch_in(&Mutex::new(Some(tmp.path().to_path_buf()))), Some(tmp.path().to_path_buf()));
    }

    #[test]
    fn until_it_leaves_it_is_the_current_folder() {
        assert_eq!(launch_in(&Mutex::new(None)), std::env::current_dir().ok());
    }
}

mod stop_events {
    use super::*;

    fn counted() -> (Mutex<Option<Stop>>, Arc<AtomicUsize>) {
        let stops = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&stops);
        let stop: Stop = Box::new(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        });
        (Mutex::new(Some(stop)), stops)
    }

    #[rstest]
    #[case::ctrl_c(CTRL_C_EVENT, Answer::Handled, Answer::Pass)]
    #[case::ctrl_break(CTRL_BREAK_EVENT, Answer::Handled, Answer::Pass)]
    #[case::a_closed_window(CTRL_CLOSE_EVENT, Answer::Wait, Answer::Wait)]
    #[case::a_logoff(CTRL_LOGOFF_EVENT, Answer::Wait, Answer::Wait)]
    #[case::a_shutdown(CTRL_SHUTDOWN_EVENT, Answer::Wait, Answer::Wait)]
    fn an_event_stops_the_server_once(#[case] event: u32, #[case] first: Answer, #[case] again: Answer) {
        let (slot, stops) = counted();

        assert_eq!(answer(event, false, &slot), first);
        assert_eq!(answer(event, false, &slot), again);
        assert_eq!(stops.load(Ordering::Relaxed), 1);
    }

    #[rstest]
    #[case::a_closed_window(CTRL_CLOSE_EVENT)]
    #[case::a_logoff(CTRL_LOGOFF_EVENT)]
    #[case::a_shutdown(CTRL_SHUTDOWN_EVENT)]
    fn an_ending_after_a_ctrl_c_waits_for_the_stop_under_way(#[case] event: u32) {
        let (slot, stops) = counted();

        assert_eq!(answer(CTRL_C_EVENT, false, &slot), Answer::Handled);
        assert_eq!(answer(event, false, &slot), Answer::Wait);
        assert_eq!(stops.load(Ordering::Relaxed), 1);
    }

    #[rstest]
    #[case::ctrl_c(CTRL_C_EVENT)]
    #[case::ctrl_break(CTRL_BREAK_EVENT)]
    fn a_console_without_a_window_takes_ctrl_c_without_stopping(#[case] event: u32) {
        let (slot, stops) = counted();

        assert_eq!(answer(event, true, &slot), Answer::Handled);
        assert_eq!(stops.load(Ordering::Relaxed), 0);
        assert!(slot.lock().is_some());
    }

    #[test]
    fn a_console_without_a_window_still_stops_at_a_shutdown() {
        let (slot, stops) = counted();

        assert_eq!(answer(CTRL_SHUTDOWN_EVENT, true, &slot), Answer::Wait);
        assert_eq!(stops.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn another_event_passes_and_keeps_the_stop() {
        let (slot, stops) = counted();

        assert_eq!(answer(4, false, &slot), Answer::Pass);
        assert_eq!(stops.load(Ordering::Relaxed), 0);
        assert!(slot.lock().is_some());
    }
}

mod in_place {
    use super::*;

    #[test]
    fn the_exit_code_of_the_new_binary_is_passed_through() {
        let code = run_in_place(Command::new("cmd").args(["/d", "/c", "exit 3"])).expect("ran");

        assert_eq!(code, 3);
    }

    #[test]
    fn a_binary_that_cannot_start_is_an_error() {
        let tmp = TempDir::new();

        let error = run_in_place(&mut Command::new(tmp.path().join("missing.exe"))).expect_err("not started");

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }
}
