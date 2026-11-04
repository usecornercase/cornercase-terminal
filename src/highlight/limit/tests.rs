#[cfg(unix)]
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use super::*;
#[cfg(unix)]
use crate::process;
#[cfg(unix)]
use crate::test_util::{self, TempDir};

#[cfg(unix)]
fn hung(pid_file: &Path) -> io::Result<Grouped> {
    let script = format!("sleep 30 & echo $! > '{}'; wait", pid_file.display());
    Grouped::spawn(
        Command::new("/bin/sh")
            .arg("-c")
            .arg(script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()),
    )
}

#[test]
#[cfg(unix)]
fn a_short_paint_is_killed_with_its_child_and_can_be_tried_again() {
    let tmp = TempDir::new();
    let pid_file = tmp.path().join("pid");
    let mut session = Session::default();
    let mut spawns = 0;
    let mut spawn = || {
        spawns += 1;
        hung(&pid_file)
    };
    assert!(session.ask("fn stuck() {}", "rust", Duration::from_millis(200), &mut spawn).is_none());
    let child = child_pid(&pid_file);
    test_util::wait_until("the painter's child is gone", || !process::alive(child));
    assert!(session.ask("fn stuck() {}", "rust", Duration::from_millis(200), &mut spawn).is_none());
    assert_eq!(spawns, 2);
}

#[test]
#[cfg(unix)]
fn a_full_budget_timeout_is_not_painted_again() {
    let mut session = Session::default();
    let mut spawns = 0;
    let mut spawn = || {
        spawns += 1;
        Grouped::spawn(
            Command::new("sleep").arg("30").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()),
        )
    };
    for _ in 0..2 {
        assert!(session.ask("fn stuck() {}", "rust", PAINT_BUDGET, &mut spawn).is_none());
    }
    assert_eq!(spawns, 1);
}

#[test]
fn an_exhausted_budget_does_not_start_a_painter() {
    let mut session = Session::default();
    let mut spawn = || panic!("started");
    assert!(session.ask("fn stuck() {}", "rust", Duration::ZERO, &mut spawn).is_none());
}

#[test]
fn a_replaced_binary_drops_the_deleted_suffix() {
    let path = Path::new("/usr/local/bin/cornercase (deleted)");
    assert_eq!(usable_exe(path), PathBuf::from("/usr/local/bin/cornercase"));
}

#[cfg(unix)]
fn child_pid(path: &Path) -> i32 {
    test_util::wait_until("the child pid is written", || path.is_file());
    std::fs::read_to_string(path).expect("pid").trim().parse().expect("a pid")
}

#[cfg(windows)]
mod on_windows {
    use std::time::Instant;

    use super::*;

    const SILENT: &str = r"%SystemRoot%\System32\ping.exe -n 30 127.0.0.1 >nul";

    fn worker(line: &str) -> (Grouped, ChildStdout) {
        let mut command = Command::new("cmd.exe");
        command.stderr(Stdio::null()).stdout(Stdio::piped()).args(["/d", "/c", line]);
        let mut child = Grouped::spawn(&mut command).expect("spawn cmd");
        let stdout = child.stdout.take().expect("its output");
        (child, stdout)
    }

    #[test]
    fn an_answer_is_waited_for() {
        let (mut child, stdout) = worker("echo painted");

        assert!(wait_in(&stdout, Instant::now() + Duration::from_secs(30)).is_ok());
        kill_group(&mut child);
    }

    #[test]
    fn a_worker_that_ends_without_answering_is_broken() {
        let (mut child, stdout) = worker("exit 0");

        assert!(matches!(wait_in(&stdout, Instant::now() + Duration::from_secs(30)), Err(Fail::Broken)));
        kill_group(&mut child);
    }

    #[test]
    fn a_silent_worker_times_out_at_its_deadline() {
        let (mut child, stdout) = worker(SILENT);
        let started = Instant::now();

        assert!(matches!(wait_in(&stdout, started + Duration::from_millis(300)), Err(Fail::Timeout)));
        assert!(started.elapsed() >= Duration::from_millis(300));
        kill_group(&mut child);
    }
}
