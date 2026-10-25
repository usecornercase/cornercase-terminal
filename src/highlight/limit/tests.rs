use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::*;

fn sleep_child() -> io::Result<Child> {
    Command::new("sleep")
        .arg("30")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
}

#[test]
fn a_painter_that_never_answers_is_killed_and_the_same_block_is_not_tried_again() {
    let mut session = Session::default();
    let mut spawns = 0;
    let mut spawn = || {
        spawns += 1;
        sleep_child()
    };
    let started = Instant::now();
    for _ in 0..3 {
        assert!(session.ask("fn stuck() {}", "rust", Duration::from_millis(200), &mut spawn).is_none());
    }
    let pid = session.last_pid.expect("the painter was started");
    assert_eq!(spawns, 1);
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(!process::alive(pid));
}
