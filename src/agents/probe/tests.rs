use super::*;
#[cfg(unix)]
use crate::test_util::{TempDir, write_executable};

const LIMITS: Limits = Limits {
    name: "fake",
    timeout: Duration::from_secs(10),
    grace: Duration::from_secs(2),
    close_input: false,
    contain: false,
};

#[cfg(unix)]
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

#[cfg(unix)]
#[test]
fn a_probe_that_gets_no_answer_fails() {
    let (_dir, command) = fake("exec sleep 30");
    let limits = Limits { timeout: Duration::from_millis(300), ..LIMITS };
    let error = run(limits, command, "hi\n", echoes).expect_err("no answer");
    assert_eq!(error.to_string(), "fake did not answer in time");
}

#[cfg(unix)]
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

#[cfg(unix)]
#[test]
fn a_follow_up_is_written_after_an_answer() {
    let (_dir, command) = fake("read -r a\necho first\nread -r b\necho \"answer $b\"\nexec sleep 30");
    let talk = |line: &str| match line {
        "first" => Step::Write("again\n".into()),
        other => echoes(other),
    };
    assert_eq!(run(LIMITS, command, "hi\n", talk).expect("an answer"), "answer again");
}

#[cfg(unix)]
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

#[cfg(unix)]
#[test]
fn a_probe_that_times_out_is_killed_without_waiting_for_the_grace() {
    let (_dir, command) = fake("exec sleep 30");
    let limits = Limits { timeout: Duration::from_millis(200), grace: Duration::from_secs(8), ..LIMITS };
    let started = Instant::now();
    run(limits, command, "hi\n", echoes).expect_err("no answer");
    assert!(started.elapsed() < Duration::from_secs(4), "{:?}", started.elapsed());
}

#[cfg(unix)]
#[test]
fn a_program_reads_end_of_input_when_stdin_is_closed() {
    let (_dir, command) = fake("cat > /dev/null\necho answer eof");
    let limits = Limits { close_input: true, ..LIMITS };
    assert_eq!(run(limits, command, "prompt\n", echoes).expect("answer"), "answer eof");
}

#[cfg(unix)]
mod a_contained_run {
    use super::*;
    use crate::os::process::{Pid, Signal, kill_process};
    use crate::test_util::wait_until;

    fn contained() -> Limits {
        Limits { contain: true, grace: Duration::from_millis(100), ..LIMITS }
    }

    fn pid_of(line: &str) -> Option<i32> {
        line.strip_prefix("child ").and_then(|pid| pid.parse().ok())
    }

    fn stamped(line: &str) -> Step<(i32, Option<u64>)> {
        match pid_of(line) {
            Some(pid) => Step::Answer(Ok((pid, process::start_stamp(pid)))),
            None => Step::Read,
        }
    }

    #[test]
    fn holds_what_it_starts_and_nothing_else() {
        let (_dir, command) = fake(r"/bin/sh -c '/bin/sleep 30 & echo child $!; wait'");
        let running = Running::default();
        let seen = running.clone();
        let own = i32::try_from(std::process::id()).expect("a pid");
        let talk = move |line: &str| match pid_of(line) {
            Some(pid) => Step::Answer(Ok((seen.holds(pid), seen.holds(own)))),
            None => Step::Read,
        };
        let held = run_in(contained(), command, "", &running, talk).expect("an answer");
        assert_eq!(held, (true, false));
    }

    #[test]
    fn is_not_held_by_a_plain_run() {
        let (_dir, command) = fake(r"/bin/sh -c '/bin/sleep 30 & echo child $!; wait'");
        let running = Running::default();
        let seen = running.clone();
        let talk = move |line: &str| pid_of(line).map_or(Step::Read, |pid| Step::Answer(Ok(seen.holds(pid))));
        let held = run_in(Limits { grace: Duration::from_millis(100), ..LIMITS }, command, "", &running, talk);
        assert!(!held.expect("an answer"));
    }

    #[test]
    fn stops_what_it_left_behind() {
        let (_dir, command) = fake("/bin/sleep 30 &\necho child $!\nread x");
        let (pid, stamp) = run(contained(), command, "", stamped).expect("an answer");
        wait_until("what the run left behind is gone", || !process::running(pid, stamp));
    }

    #[test]
    fn a_plain_run_leaves_it_running() {
        let (_dir, command) = fake("/bin/sleep 30 &\necho child $!\nread x");
        let limits = Limits { grace: Duration::from_millis(100), ..LIMITS };
        let (pid, stamp) = run(limits, command, "", stamped).expect("an answer");
        assert!(process::running(pid, stamp));
        if let Some(pid) = Pid::from_raw(pid) {
            let _ = kill_process(pid, Signal::KILL);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn adopts_an_orphan_of_what_it_started() {
        let (_dir, command) = fake(r"/bin/sh -c '/bin/sleep 30 & echo child $!'; read x");
        let running = Running::default();
        let seen = running.clone();
        let talk = move |line: &str| {
            let Some(pid) = pid_of(line) else { return Step::Read };
            let root = seen.0.lock().pid.map(|(root, _)| root);
            wait_until("the orphan is adopted by the run's root", || process::parent(pid) == root);
            Step::Answer(Ok(seen.holds(pid)))
        };
        assert!(run_in(contained(), command, "", &running, talk).expect("an answer"));
    }

    #[test]
    fn stops_what_a_root_that_closed_its_output_started() {
        let (dir, command) = fake("/bin/sleep 30 >/dev/null 2>&1 &\necho $! > \"$0.pid\"\nexec >&-\nsleep 3");
        let error = run(contained(), command, "", echoes).expect_err("no answer");
        assert_eq!(error.to_string(), "fake exited without answering");
        let pid: i32 =
            std::fs::read_to_string(dir.path().join("fake.pid")).expect("a pid").trim().parse().expect("a pid");
        wait_until("what the root started is gone", || !process::running(pid, None));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn stops_what_a_child_left_behind_during_the_grace() {
        let script = "/bin/sh -c 'sleep 0.5; /bin/sleep 30 >/dev/null 2>&1 & echo $! > \"$0.pid\"' \"$0\" &\necho answer\nsleep 3";
        let (dir, command) = fake(script);
        let limits = Limits { grace: Duration::from_millis(1500), ..contained() };
        assert_eq!(run(limits, command, "", echoes).expect("an answer"), "answer");
        let pid: i32 =
            std::fs::read_to_string(dir.path().join("fake.pid")).expect("a pid").trim().parse().expect("a pid");
        wait_until("the grandchild is gone", || !process::running(pid, None));
    }
}
