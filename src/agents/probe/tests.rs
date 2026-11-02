use super::*;
use crate::test_util::{TempDir, write_executable};

const LIMITS: Limits = Limits { name: "fake", timeout: Duration::from_secs(10), grace: Duration::from_secs(2) };

fn fake(script: &str) -> (TempDir, Command) {
    let dir = TempDir::new();
    let path = dir.path().join("fake");
    write_executable(&path, &format!("#!/bin/sh\n{script}\n"));
    let command = Command::new(&path);
    (dir, command)
}

fn echoes(line: &str) -> Step<String> {
    if line.starts_with("answer") { Step::Answer(Ok(line.to_string())) } else { Step::Read }
}

#[test]
fn a_probe_that_gets_no_answer_fails() {
    let (_dir, command) = fake("exec sleep 30");
    let limits = Limits { timeout: Duration::from_millis(300), ..LIMITS };
    let error = run(limits, command, "hi\n", echoes).expect_err("no answer");
    assert_eq!(error.to_string(), "fake did not answer in time");
}

#[test]
fn a_program_that_exits_fails_at_once() {
    let (_dir, command) = fake("exit 1");
    let error = run(LIMITS, command, "hi\n", echoes).expect_err("no answer");
    assert_eq!(error.to_string(), "fake exited without answering");
}

#[test]
fn a_missing_program_fails() {
    let error = run(LIMITS, Command::new("/nonexistent/fake"), "hi\n", echoes).expect_err("no answer");
    assert!(error.to_string().starts_with("could not run /nonexistent/fake"), "{error}");
}

#[test]
fn a_follow_up_is_written_after_an_answer() {
    let (_dir, command) = fake("read -r a\necho first\nread -r b\necho \"answer $b\"\nexec sleep 30");
    let talk = |line: &str| match line {
        "first" => Step::Write("again\n".into()),
        other => echoes(other),
    };
    assert_eq!(run(LIMITS, command, "hi\n", talk).expect("an answer"), "answer again");
}

#[test]
fn closing_its_input_lets_the_probe_exit_on_its_own() {
    let (dir, command) = fake("read -r a\necho answer\ncat > /dev/null\ntouch \"$0.done\"");
    assert_eq!(run(LIMITS, command, "hi\n", echoes).expect("an answer"), "answer");
    assert!(dir.path().join("fake.done").exists(), "the program was killed instead of reaching its end");
}

#[test]
fn each_answer_counts_once_and_a_running_probe_is_not_restarted() {
    let mut probe = Probe::<u8>::default();
    assert!(probe.start());
    assert!(!probe.start());
    probe.answered(Ok(1), Instant::now());
    assert_eq!((probe.answers(), probe.last().map(|(n, _)| *n)), (1, Some(1)));
    assert!(probe.start());
    probe.answered(Err(Error::Probe("no".into())), Instant::now());
    assert_eq!((probe.answers(), probe.error()), (2, Some("no")));
}
