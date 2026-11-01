use std::os::unix::net::UnixStream;
use std::thread::JoinHandle;
use std::time::Instant;

use rstest::rstest;
use serde_json::json;

use super::*;
use crate::control::{Response, Stats};
use crate::machines::link::fake::{Hanging, Once, REFUSED, ended, line, request, respond, window_messages};
use crate::protocol::{self, ClientMessage, ServerMessage};

const WAIT: Duration = Duration::from_secs(10);
const SAID: &str = "Connection to vps closed by remote host.";

fn stats() -> Command {
    Command::Stats(Stats::default())
}

fn asking(timeout: Duration) -> (JoinHandle<Result<String, String>>, UnixStream) {
    asking_with(None, timeout)
}

fn asking_with(boot: Option<&str>, timeout: Duration) -> (JoinHandle<Result<String, String>>, UnixStream) {
    let (line, theirs) = line("0.12.232", Some(SAID));
    let connect = Once(Mutex::new(Some(line)));
    let boot = boot.map(str::to_string);
    (thread::spawn(move || ask(&connect, "vps", stats(), boot, timeout)), theirs)
}

fn answered(asking: JoinHandle<Result<String, String>>) -> Result<String, String> {
    asking.join().expect("the question ends")
}

fn cut(theirs: &mut UnixStream) -> bool {
    theirs.set_read_timeout(Some(WAIT)).expect("a wait for the cut");
    loop {
        match protocol::recv::<ClientMessage>(theirs) {
            Ok(Some(_)) => {}
            Ok(None) => return true,
            Err(_) => return false,
        }
    }
}

fn reply() -> (Response, String) {
    let response = Response::Ok(json!({"since": "2026-10-01T00:00:00.000Z"}));
    let text = serde_json::to_string(&response).expect("an answer");
    (response, text)
}

mod a_question {
    use super::*;

    #[test]
    fn writes_only_its_request_and_gets_its_answer() {
        let (asking, mut theirs) = asking(WAIT);

        let asked = request(&mut theirs);
        let (response, text) = reply();
        respond(&mut theirs, &response);

        assert_eq!(answered(asking), Ok(text));
        assert_eq!(asked["command"], "stats");
        assert!(asked["caller"].is_null() && asked["server"].is_null(), "{asked}");
        assert!(["view", "contract", "boot"].iter().all(|key| asked.get(key).is_none()), "{asked}");
        assert!(ended(&mut theirs));
    }

    #[test]
    fn carries_the_boot_it_is_given() {
        let (asking, mut theirs) = asking_with(Some("boot-7"), WAIT);

        let asked = request(&mut theirs);
        respond(&mut theirs, &reply().0);

        assert!(answered(asking).is_ok());
        assert_eq!(asked["boot"], "boot-7", "{asked}");
        assert_eq!(asked["command"], "stats");
    }

    #[test]
    fn ignores_what_a_window_gets_before_its_answer() {
        let (asking, mut theirs) = asking(WAIT);

        request(&mut theirs);
        for message in window_messages() {
            protocol::send(&mut theirs, &message).expect("send to the question");
        }
        protocol::send(&mut theirs, &99u8).expect("send a message from a newer cornercase");
        let (response, text) = reply();
        respond(&mut theirs, &response);

        assert_eq!(answered(asking), Ok(text));
    }

    #[test]
    fn a_machine_that_cannot_be_reached_says_why() {
        let connect = Once(Mutex::new(None));

        assert_eq!(ask(&connect, "vps", stats(), None, WAIT), Err(REFUSED.into()));
    }

    #[rstest]
    #[case::closed(None, SAID)]
    #[case::stopped(Some(ServerMessage::Shutdown), "the server there stopped")]
    #[case::let_go(Some(ServerMessage::Detached), "the server there ended the link")]
    fn an_end_without_an_answer_says_why(#[case] ending: Option<ServerMessage>, #[case] why: &str) {
        let (asking, mut theirs) = asking(WAIT);

        request(&mut theirs);
        match ending {
            Some(message) => protocol::send(&mut theirs, &message).expect("end the question"),
            None => drop(theirs),
        }

        assert_eq!(answered(asking), Err(why.into()));
    }
}

mod the_deadline {
    use super::*;

    #[test]
    fn cuts_a_machine_that_does_not_answer() {
        let (asking, mut theirs) = asking(Duration::from_millis(50));

        assert!(cut(&mut theirs));
        assert_eq!(answered(asking), Err(LATE.into()));
    }

    #[test]
    fn ends_an_ssh_that_has_not_answered() {
        let started = Instant::now();

        let asked = ask(&Hanging, "vps", stats(), None, Duration::from_millis(50));

        assert_eq!(asked, Err(LATE.into()));
        assert!(started.elapsed() < WAIT, "{:?}", started.elapsed());
    }
}
