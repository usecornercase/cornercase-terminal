use super::*;
use crate::test_util::{TempDir, wait_until, write_executable};

const HOLDS: &str = "#!/bin/sh\necho \"$@\" >> \"$0.args\"\necho $$ > \"$0.pid\"\nexec cat\n";
const REFUSES: &str = "#!/bin/sh\necho \"$@\" >> \"$0.args\"\necho 'Failed to inhibit: Access denied' >&2\nexit 1\n";

struct Fakes {
    dir: TempDir,
}

impl Fakes {
    fn new() -> Self {
        Self { dir: TempDir::new() }
    }

    fn tool(&self, name: &str, script: &str) -> Tool {
        let path = self.dir.path().join(name);
        write_executable(&path, script);
        Tool::new(name, path, &["--why=working", "cat"])
    }

    fn calls(&self, name: &str) -> usize {
        std::fs::read_to_string(self.dir.path().join(format!("{name}.args"))).map_or(0, |text| text.lines().count())
    }

    fn pid(&self, name: &str) -> i32 {
        let path = self.dir.path().join(format!("{name}.pid"));
        wait_until("the fake writes its pid", || std::fs::read_to_string(&path).is_ok_and(|t| t.ends_with('\n')));
        std::fs::read_to_string(path).expect("the pid").trim().parse().expect("a pid")
    }
}

fn gone(pid: i32) -> bool {
    !crate::process::alive(pid)
}

fn settled(awake: &mut Awake, working: usize, now: Instant) {
    wait_until("the refusal is seen", || {
        awake.update(true, working, now);
        !matches!(awake.state(), State::Held(ref tool) if tool == "refuses")
    });
}

#[test]
fn holds_while_an_agent_works_with_the_tools_arguments() {
    let fakes = Fakes::new();
    let mut awake = Awake::new(vec![fakes.tool("holds", HOLDS)]);

    awake.update(true, 2, Instant::now());
    let pid = fakes.pid("holds");
    let args = std::fs::read_to_string(fakes.dir.path().join("holds.args")).expect("the arguments");

    assert_eq!((awake.state(), args.as_str()), (State::Held("holds".into()), "--why=working cat\n"));
    assert!(!gone(pid));
}

#[test]
fn nothing_is_held_while_no_agent_works_or_the_setting_is_off() {
    let fakes = Fakes::new();
    let mut awake = Awake::new(vec![fakes.tool("holds", HOLDS)]);

    awake.update(true, 0, Instant::now());
    let idle = awake.state();
    awake.update(false, 3, Instant::now());

    assert_eq!((idle, awake.state(), fakes.calls("holds")), (State::Released, State::Off, 0));
}

#[test]
fn lets_go_once_no_agent_has_worked_for_a_while() {
    let fakes = Fakes::new();
    let mut awake = Awake::new(vec![fakes.tool("holds", HOLDS)]);
    let start = Instant::now();
    awake.update(true, 1, start);
    let pid = fakes.pid("holds");

    awake.update(true, 0, start + Duration::from_secs(1));
    awake.update(true, 0, start + KEEP_FOR);
    let kept = awake.state();
    awake.update(true, 0, start + Duration::from_secs(1) + KEEP_FOR);

    assert_eq!((kept, awake.state()), (State::Held("holds".into()), State::Released));
    wait_until("the holder ends", || gone(pid));
}

#[test]
fn work_again_within_the_grace_keeps_the_same_holder() {
    let fakes = Fakes::new();
    let mut awake = Awake::new(vec![fakes.tool("holds", HOLDS)]);
    let start = Instant::now();
    awake.update(true, 1, start);

    awake.update(true, 0, start + Duration::from_secs(1));
    awake.update(true, 1, start + Duration::from_secs(20));
    awake.update(true, 0, start + Duration::from_secs(40));
    awake.update(true, 0, start + Duration::from_secs(60));
    fakes.pid("holds");

    assert_eq!((awake.state(), fakes.calls("holds")), (State::Held("holds".into()), 1));
}

#[test]
fn turning_it_off_lets_go_at_once() {
    let fakes = Fakes::new();
    let mut awake = Awake::new(vec![fakes.tool("holds", HOLDS)]);
    awake.update(true, 1, Instant::now());
    let pid = fakes.pid("holds");

    awake.update(false, 1, Instant::now());

    assert_eq!(awake.state(), State::Off);
    wait_until("the holder ends", || gone(pid));
}

#[test]
fn a_refused_lock_falls_back_to_the_next_tool() {
    let fakes = Fakes::new();
    let mut awake = Awake::new(vec![fakes.tool("refuses", REFUSES), fakes.tool("holds", HOLDS)]);

    settled(&mut awake, 1, Instant::now());

    assert_eq!((awake.state(), fakes.calls("refuses")), (State::Held("holds".into()), 1));
}

#[test]
fn says_why_when_every_tool_refuses_and_stops_trying() {
    let fakes = Fakes::new();
    let mut awake = Awake::new(vec![fakes.tool("refuses", REFUSES)]);

    settled(&mut awake, 1, Instant::now());
    awake.update(true, 1, Instant::now());

    let reason = "refuses: Failed to inhibit: Access denied".to_string();
    assert_eq!((awake.state(), fakes.calls("refuses")), (State::Unavailable(reason), 1));
}

#[test]
fn a_missing_tool_is_named() {
    let mut awake = Awake::new(vec![Tool::new("systemd-inhibit", "/nonexistent/systemd-inhibit", &[])]);

    awake.update(true, 1, Instant::now());

    assert_eq!(awake.state(), State::Unavailable("systemd-inhibit is not installed".into()));
}

#[test]
fn the_holder_ends_with_the_end_of_its_input_as_when_the_server_dies() {
    let fakes = Fakes::new();
    let mut awake = Awake::new(vec![fakes.tool("holds", HOLDS)]);
    awake.update(true, 1, Instant::now());
    let pid = fakes.pid("holds");

    drop(awake.holder.as_mut().and_then(|h| h.stdin.take()));

    wait_until("the holder ends", || {
        awake.update(true, 1, Instant::now());
        !matches!(awake.state(), State::Held(_))
    });
    assert!(gone(pid));
}
