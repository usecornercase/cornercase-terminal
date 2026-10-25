use std::io;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use super::*;
use crate::process;
use crate::test_util::{self, TempDir};

fn hung(pid_file: &Path) -> io::Result<Child> {
    let script = format!("sleep 30 & echo $! > '{}'; wait", pid_file.display());
    Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
}

#[test]
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
fn a_full_budget_timeout_is_not_painted_again() {
    let mut session = Session::default();
    let mut spawns = 0;
    let mut spawn = || {
        spawns += 1;
        Command::new("sleep")
            .arg("30")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
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

fn child_pid(path: &Path) -> i32 {
    test_util::wait_until("the child pid is written", || path.is_file());
    std::fs::read_to_string(path).expect("pid").trim().parse().expect("a pid")
}
