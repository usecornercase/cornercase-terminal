use std::os::unix::net::UnixStream;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use rstest::rstest;
use serde_json::json;

use super::*;
use crate::control::{Changes, Response};
use crate::machines::link::fake::{Hanging, Once, REFUSED, ended, line, request, respond, window_messages};
use crate::protocol::{self, ServerMessage};
use crate::remote::Killer;

const WAIT: Duration = Duration::from_secs(10);
const SAID: &str = "Connection to vps closed by remote host.";
const BOOT: &str = "boot-7";

struct Bug;

impl Connect for Bug {
    fn connect(&self, _: &Killer) -> crate::error::Result<Line> {
        panic!("a connector bug")
    }
}

struct Rig {
    follow: Follow,
    heard: Receiver<Followed>,
}

impl Rig {
    fn start(id: u64, once: bool) -> (Self, UnixStream) {
        let (line, theirs) = line("0.12.232", Some(SAID));
        (Self::with(id, Box::new(Once(Mutex::new(Some(line)))), once), theirs)
    }

    fn with(id: u64, connector: Box<dyn Connect>, once: bool) -> Self {
        let (tx, heard) = mpsc::channel();
        let wanted = Wanted { command: changes(), boot: BOOT.into(), once };
        Self { follow: Follow::start(id, connector, "vps", wanted, &tx), heard }
    }

    fn event(&self) -> Event {
        let followed = self.heard.recv_timeout(WAIT).expect("an event from the follow");
        assert_eq!(followed.id, self.follow.id());
        followed.event
    }

    fn quiet(&self) -> bool {
        self.heard.recv_timeout(Duration::from_millis(100)).is_err()
    }
}

fn changes() -> Command {
    Command::Changes(Changes::default())
}

fn answer(theirs: &mut UnixStream, n: u64) -> String {
    let response = Response::Ok(json!({ "n": n }));
    respond(theirs, &response);
    serde_json::to_string(&response).expect("an answer")
}

mod a_stream {
    use super::*;

    #[test]
    fn sends_its_request_with_the_boot_once() {
        let (rig, mut theirs) = Rig::start(3, false);

        let asked = request(&mut theirs);

        assert_eq!(asked["command"], "changes");
        assert_eq!(asked["boot"], BOOT, "{asked}");
        drop(rig);
        assert!(ended(&mut theirs));
    }

    #[test]
    fn hands_on_every_answer_in_order_then_its_end() {
        let (rig, mut theirs) = Rig::start(3, false);
        request(&mut theirs);

        let said: Vec<String> = (1..=3).map(|n| answer(&mut theirs, n)).collect();
        protocol::send(&mut theirs, &ServerMessage::Shutdown).expect("stop the server there");

        let heard: Vec<Event> = (0..4).map(|_| rig.event()).collect();
        let mut expected: Vec<Event> = said.into_iter().map(Event::Line).collect();
        expected.push(Event::Ended("the server there stopped".into()));
        assert_eq!(heard, expected);
        assert!(ended(&mut theirs));
    }

    #[test]
    fn skips_what_a_window_gets_between_its_answers() {
        let (rig, mut theirs) = Rig::start(3, false);
        request(&mut theirs);

        let first = answer(&mut theirs, 1);
        for message in window_messages() {
            protocol::send(&mut theirs, &message).expect("send to the stream");
        }
        protocol::send(&mut theirs, &99u8).expect("send a message from a newer cornercase");
        let second = answer(&mut theirs, 2);

        assert_eq!(rig.event(), Event::Line(first));
        assert_eq!(rig.event(), Event::Line(second));
    }

    #[rstest]
    #[case::closed(None, SAID)]
    #[case::let_go(Some(ServerMessage::Detached), "the server there ended the link")]
    #[case::restarting(Some(ServerMessage::Restart("/usr/bin/cornercase".into())), "the server there is restarting")]
    fn an_end_says_why(#[case] ending: Option<ServerMessage>, #[case] why: &str) {
        let (rig, mut theirs) = Rig::start(3, false);
        request(&mut theirs);

        match ending {
            Some(message) => protocol::send(&mut theirs, &message).expect("end the stream"),
            None => drop(theirs),
        }

        assert_eq!(rig.event(), Event::Ended(why.into()));
        assert!(rig.quiet());
    }
}

mod once {
    use super::*;

    #[test]
    fn hands_on_one_answer_and_closes_the_connection() {
        let (rig, mut theirs) = Rig::start(5, true);
        request(&mut theirs);

        let said = answer(&mut theirs, 1);

        assert_eq!(rig.event(), Event::Line(said));
        assert!(ended(&mut theirs));
        assert!(rig.quiet());
    }
}

mod failures {
    use super::*;

    #[test]
    fn a_machine_that_cannot_be_reached_ends_it_with_why() {
        let rig = Rig::with(3, Box::new(Once(Mutex::new(None))), false);

        assert_eq!(rig.event(), Event::Ended(REFUSED.into()));
    }

    #[test]
    fn a_bug_ends_it_without_taking_the_caller_down() {
        let rig = Rig::with(3, Box::new(Bug), false);

        assert_eq!(rig.event(), Event::Ended(Error::Bug.to_string()));
    }
}

mod dropping {
    use super::*;

    #[test]
    fn closes_the_connection_and_tells_nothing_more() {
        let (rig, mut theirs) = Rig::start(3, false);
        request(&mut theirs);
        let said = answer(&mut theirs, 1);
        assert_eq!(rig.event(), Event::Line(said));

        let Rig { follow, heard } = rig;
        drop(follow);

        assert!(ended(&mut theirs));
        assert!(heard.recv_timeout(Duration::from_millis(100)).is_err());
    }

    #[test]
    fn ends_an_ssh_that_has_not_answered() {
        let started = Instant::now();
        let (tx, heard) = mpsc::channel::<Followed>();
        let wanted = Wanted { command: changes(), boot: BOOT.into(), once: false };
        let follow = Follow::start(3, Box::new(Hanging), "vps", wanted, &tx);
        drop(tx);

        drop(follow);

        assert!(heard.recv_timeout(WAIT).is_err());
        assert!(started.elapsed() < WAIT, "{:?}", started.elapsed());
    }
}
