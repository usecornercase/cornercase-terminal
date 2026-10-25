use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

use super::{Outcome, Target, kill, stop};

const LISTEN: &str = "import signal, socket, sys, time
if sys.argv[1] == 'ignore':
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
s = socket.socket()
s.bind(('127.0.0.1', 0))
s.listen()
print(s.getsockname()[1], flush=True)
time.sleep(60)
";

struct Listening {
    child: Child,
    port: u16,
}

impl Listening {
    fn new(term: &str) -> Self {
        let mut child = Command::new("python3")
            .args(["-c", LISTEN, term])
            .stdout(Stdio::piped())
            .stdin(Stdio::null())
            .spawn()
            .expect("run python3");
        let mut line = String::new();
        BufReader::new(child.stdout.take().expect("stdout")).read_line(&mut line).expect("read the port");
        let port = line.trim().parse().expect("a port");
        Self { child, port }
    }

    fn target(&self) -> Target {
        let pid = i32::try_from(self.child.id()).expect("a pid");
        Target { pid, port: self.port, started: crate::process::start_stamp(pid), program: Some("python3".into()) }
    }

    fn running(&mut self) -> bool {
        self.child.try_wait().expect("check the child").is_none()
    }
}

impl Drop for Listening {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn a_program_that_ends_on_sigterm_is_stopped() {
    let listening = Listening::new("end");
    let target = listening.target();
    assert_eq!(stop(vec![target.clone()]), [(target, Ok(Outcome::Stopped))]);
}

#[test]
fn a_program_that_ignores_sigterm_is_killed() {
    let listening = Listening::new("ignore");
    let target = listening.target();
    assert_eq!(stop(vec![target.clone()]), [(target, Ok(Outcome::Killed))]);
}

#[test]
fn a_pid_that_no_longer_holds_the_port_is_left_alone() {
    let mut listening = Listening::new("end");
    let target = Target { port: listening.port.wrapping_add(1), ..listening.target() };
    let stopped = stop(vec![target]);
    assert!(matches!(&stopped[..], [(_, Err(reason))] if reason.contains("no longer listens")), "{stopped:?}");
    assert!(listening.running());
}

#[test]
fn a_pid_that_started_again_is_left_alone() {
    let mut listening = Listening::new("end");
    let target = listening.target();
    let target = Target { started: target.started.map(|stamp| stamp + 1), ..target };
    let stopped = stop(vec![target]);
    assert!(matches!(&stopped[..], [(_, Err(reason))] if reason.contains("no longer the program")), "{stopped:?}");
    assert!(listening.running());
}

#[test]
fn several_programs_are_stopped_together() {
    let (first, second) = (Listening::new("end"), Listening::new("ignore"));
    let outcomes: Vec<_> = stop(vec![first.target(), second.target()]).into_iter().map(|(t, o)| (t.port, o)).collect();
    assert!(outcomes.contains(&(first.port, Ok(Outcome::Stopped))), "{outcomes:?}");
    assert!(outcomes.contains(&(second.port, Ok(Outcome::Killed))), "{outcomes:?}");
}

#[test]
fn a_pid_whose_start_changed_before_sigkill_is_not_killed() {
    let mut listening = Listening::new("ignore");
    let target = listening.target();
    let target = Target { started: target.started.map(|stamp| stamp + 1), ..target };
    let mut done = Vec::new();
    assert_eq!(kill(vec![target.clone()], &mut done), []);
    assert_eq!(done, [(target, Ok(Outcome::Stopped))]);
    assert!(listening.running());
}

#[test]
fn a_program_that_released_its_port_before_sigkill_is_not_killed() {
    let mut listening = Listening::new("ignore");
    let target = Target { port: listening.port.wrapping_add(1), ..listening.target() };
    let mut done = Vec::new();
    assert_eq!(kill(vec![target.clone()], &mut done), []);
    assert_eq!(done, [(target, Ok(Outcome::Stopped))]);
    assert!(listening.running());
}
