use std::fs::File;
use std::io::{BufRead, BufReader};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use rstest::rstest;
use windows_sys::Win32::Foundation::{GetHandleInformation, WAIT_TIMEOUT};
use windows_sys::Win32::System::Console::GetConsoleProcessList;
use windows_sys::Win32::System::Threading::{PROCESS_SYNCHRONIZE, WaitForSingleObject};

use super::*;
use crate::os::process::Grouped;
use crate::process::{descendants, name};
use crate::test_util::{TempDir, wait_until, wait_until_within};

const PING: &str = r"%SystemRoot%\System32\ping.exe -n 30 127.0.0.1";

const SHORT_PING: &str = r"%SystemRoot%\System32\ping.exe -n 2 127.0.0.1";

const STARTS_WITHIN: Duration = Duration::from_secs(30);

fn ping() -> Child {
    Command::new("ping").args(["-n", "30", "127.0.0.1"]).stdout(Stdio::null()).spawn().expect("spawn ping")
}

fn pid_of(child: &Child) -> i32 {
    i32::try_from(child.id()).expect("a pid")
}

fn pings(pid: i32) -> Vec<i32> {
    let named = |pid: &i32| name(*pid).is_some_and(|name| name.eq_ignore_ascii_case("ping"));
    descendants(pid).into_iter().filter(named).collect()
}

fn grouped(line: &str) -> (Grouped, Vec<i32>) {
    let child =
        Grouped::spawn(Command::new("cmd.exe").args(["/d", "/c", line]).stdin(Stdio::null()).stdout(Stdio::null()))
            .expect("spawn cmd");
    let pid = pid_of(&child);
    wait_until_within("cmd starts its pings", STARTS_WITHIN, || pings(pid).len() == line.matches("ping").count());
    let tree = std::iter::once(pid).chain(descendants(pid)).collect();
    (child, tree)
}

fn all_gone(tree: &[i32]) -> bool {
    tree.iter().all(|pid| !alive(*pid))
}

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

mod stopping_a_process {
    use super::*;

    #[test]
    fn the_hard_step_ends_it() {
        let mut child = ping();
        let pid = Pid::from_raw(pid_of(&child)).expect("a pid");

        kill_process(pid, Signal::KILL).expect("terminate");

        assert!(!child.wait().expect("reap").success());
        assert!(!alive(pid_of(&child)));
    }

    #[test]
    fn a_program_without_a_window_cannot_be_asked_and_the_request_changes_nothing() {
        let mut child = ping();
        let pid = pid_of(&child);

        assert!(!askable(pid));
        kill_process(Pid::from_raw(pid).expect("a pid"), Signal::TERM).expect("nothing to close");
        assert!(child.try_wait().expect("check").is_none());
        let _ = (child.kill(), child.wait());
    }

    #[test]
    fn a_pid_without_a_process_cannot_be_ended() {
        let pid = Pid::from_raw(crate::test_util::exited_pid()).expect("a pid");
        assert!(kill_process(pid, Signal::KILL).is_err());
    }
}

mod a_job {
    use super::*;

    #[test]
    fn holds_the_child_and_what_it_starts() {
        let (child, tree) = grouped(&format!("start /b {PING} & {PING}"));
        let job = child.job.handle.as_ref().expect("a job");

        let mut members = listed(job);
        members.sort_unstable();
        let mut expected = tree.iter().map(|pid| u32::try_from(*pid).expect("a pid")).collect::<Vec<_>>();
        expected.sort_unstable();

        assert!(expected.iter().all(|pid| members.contains(pid)), "{members:?} {expected:?}");
        child.signal(Signal::KILL);
    }

    #[test]
    fn ends_the_whole_tree_on_the_hard_step() {
        let (mut child, tree) = grouped(&format!("start /b {PING} & {PING}"));

        assert!(child.signal(Signal::KILL));

        wait_until("the tree is gone", || all_gone(&tree));
        assert!(!child.wait().expect("reap").success());
    }

    #[test]
    fn asks_nothing_of_programs_without_a_window() {
        let (mut child, tree) = grouped(PING);

        assert!(!child.signal(Signal::TERM));
        assert!(child.try_wait().expect("check").is_none());
        child.signal(Signal::KILL);
        wait_until("the tree is gone", || all_gone(&tree));
    }

    #[test]
    fn closed_by_the_system_ends_what_is_left() {
        let (mut child, tree) = grouped(&format!("start /b {PING} & {PING}"));

        drop(child.job.handle.take());

        wait_until("the tree is gone", || all_gone(&tree));
        child.wait().expect("reap");
    }

    #[test]
    fn dropped_after_its_child_ends_leaves_what_the_child_started() {
        let (mut child, tree) = grouped(&format!("start /b {PING} & {SHORT_PING}"));
        assert!(child.wait().expect("cmd ends").success());
        let access = PROCESS_SYNCHRONIZE | PROCESS_TERMINATE;
        let left: Vec<OwnedHandle> =
            tree.into_iter().filter(|pid| alive(*pid)).filter_map(|pid| open(pid_u32(pid), access).ok()).collect();

        drop(child);

        assert!(!left.is_empty() && left.iter().all(still_running_a_second_later));
        for process in &left {
            let _ = unsafe { TerminateProcess(process.as_raw_handle(), KILLED) };
        }
    }

    fn pid_u32(pid: i32) -> u32 {
        u32::try_from(pid).expect("a pid")
    }

    fn still_running_a_second_later(process: &OwnedHandle) -> bool {
        unsafe { WaitForSingleObject(process.as_raw_handle(), 1000) == WAIT_TIMEOUT }
    }
}
