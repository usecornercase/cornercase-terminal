use std::os::unix::net::UnixStream;
use std::thread;

use super::*;

fn old_server(answer: ServerMessage) -> Result<Value> {
    let (ours, mut theirs) = UnixStream::pair().expect("a socket pair");
    let server = thread::spawn(move || {
        let asked = protocol::recv::<ClientMessage>(&mut theirs).expect("a request");
        protocol::send(&mut theirs, &answer).expect("an answer");
        asked
    });

    let answered =
        ask_on(&mut Link::socket(ours).expect("a link"), "contract", control::Command::Contract(control::Contract {}));

    let Some(ClientMessage::Request(text)) = server.join().expect("the fake server") else { panic!("not a request") };
    assert!(text.contains("\"contract\""), "{text}");
    answered
}

#[test]
fn a_server_without_contract_says_it_has_no_such_command() {
    let refusal = serde_json::to_string(&Response::Error(control::unknown_command("contract"))).expect("json");

    let error = old_server(ServerMessage::Response(refusal)).expect_err("refused");

    assert!(matches!(&error, Error::Control(_)), "{error:?}");
    assert!(error.to_string().contains("has no `contract` command"), "{error}");
}

#[test]
fn a_server_older_than_requests_is_too_old() {
    let error = old_server(ServerMessage::Rejected("another build".into())).expect_err("rejected");

    assert!(matches!(error, Error::OldServer("contract")), "{error:?}");
}

mod features {
    use super::*;

    #[test]
    fn a_failure_to_reach_the_server_is_not_taken_for_a_missing_feature() {
        let checked = require_on(Err(Error::NoServer), "accounts", "has no account profiles", None);

        assert!(matches!(checked, Err(Error::NoServer)), "{checked:?}");
    }

    fn offering(features: &[&str]) -> (Result<()>, Vec<String>) {
        asking(features, "accounts", "has no account profiles")
    }

    fn asking(features: &[&str], feature: &str, lacks: &str) -> (Result<()>, Vec<String>) {
        let (ours, mut theirs) = UnixStream::pair().expect("a socket pair");
        let mut descriptor = control::contract::describe("boot", &[]);
        descriptor.features.retain(|feature| features.contains(&feature.as_str()));
        let server = thread::spawn(move || {
            let mut asked = Vec::new();
            while let Ok(Some(ClientMessage::Request(text))) = protocol::recv::<ClientMessage>(&mut theirs) {
                asked.push(text);
                let answer = serde_json::to_string(&Response::Ok(json(&descriptor))).expect("json");
                protocol::send(&mut theirs, &ServerMessage::Response(answer)).expect("an answer");
            }
            asked
        });

        let checked = require_on(Link::socket(ours), feature, lacks, None);

        (checked, server.join().expect("the fake server"))
    }

    #[test]
    fn a_missing_feature_there_says_how_to_restart_it_from_here() {
        let remote =
            Remote { destination: "devbox".into(), command: Some("/opt/cc".into()), option: "--remote-command" };

        let message = missing("has no views", Some(&remote)).to_string();

        assert!(
            message.contains("`devbox`")
                && message.contains("`cornercase --remote devbox --remote-command /opt/cc restart`")
                && message.contains("`cornercase --remote devbox --remote-command /opt/cc update`"),
            "{message}"
        );
    }

    #[test]
    fn a_missing_feature_here_says_to_update_cornercase() {
        let message = missing("has no views", None).to_string();

        assert!(message.contains("update cornercase and restart the server"), "{message}");
    }

    fn json(descriptor: &control::contract::Descriptor) -> Value {
        serde_json::to_value(descriptor).expect("a descriptor")
    }

    #[test]
    fn a_server_without_accounts_is_refused_after_asking_only_for_its_contract() {
        let (checked, asked) = offering(&["views"]);

        let error = checked.expect_err("refused");
        assert!(error.to_string().contains("has no account profiles"), "{error}");
        assert_eq!(asked.len(), 1, "{asked:?}");
        assert!(asked[0].contains("\"contract\""), "{}", asked[0]);
    }

    #[test]
    fn a_server_with_accounts_lets_the_request_go() {
        let (checked, asked) = offering(&["accounts"]);

        checked.expect("accepted");
        assert_eq!(asked.len(), 1);
    }

    #[test]
    fn a_server_without_login_checks_is_refused_before_the_list_is_asked_for() {
        let (checked, asked) = asking(&["accounts"], "account-login", "cannot check the accounts' logins");

        let error = checked.expect_err("refused");
        assert!(error.to_string().contains("cannot check the accounts' logins"), "{error}");
        assert_eq!(asked.len(), 1, "{asked:?}");
    }

    #[test]
    fn a_server_without_account_switching_is_refused_before_the_change_is_sent() {
        let (checked, asked) = asking(&["accounts"], "account-switch", "cannot rename or switch accounts");

        let error = checked.expect_err("refused");
        assert!(error.to_string().contains("cannot rename or switch accounts"), "{error}");
        assert_eq!(asked.len(), 1, "{asked:?}");
    }

    #[test]
    fn a_server_that_cannot_name_the_workspace_is_refused_before_the_start_is_sent() {
        let (checked, asked) = asking(&["issues"], "issue-names", "cannot name the workspace an issue starts in");

        let error = checked.expect_err("refused");
        assert!(error.to_string().contains("cannot name the workspace an issue starts in"), "{error}");
        assert_eq!(asked.len(), 1, "{asked:?}");
    }

    #[test]
    fn a_server_that_cannot_reveal_is_refused_before_the_focus_is_sent() {
        let (checked, asked) = asking(&["views"], "focus-reveal", "cannot reveal a search match in its pane");

        let error = checked.expect_err("refused");
        assert!(error.to_string().contains("cannot reveal a search match in its pane"), "{error}");
        assert_eq!(asked.len(), 1, "{asked:?}");
        assert!(asked[0].contains("\"contract\""), "{}", asked[0]);
    }
}

mod the_view {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::stays_out_of_the_contract(control::Command::Contract(control::Contract {}), None)]
    #[case::goes_with_a_command_that_takes_one(control::Command::Status(control::Status {}), Some(7))]
    #[case::goes_with_one_that_takes_none_for_the_server_to_refuse(control::Command::Stats(control::Stats {}), Some(7))]
    fn of_the_invocation_goes_with_every_request_but_the_contract(
        #[case] command: control::Command,
        #[case] sent: Option<u64>,
    ) {
        assert_eq!(view_for(&command, Some(7)), sent);
    }
}

mod a_follow {
    use std::fs::OpenOptions;
    use std::os::fd::{AsFd, OwnedFd};
    use std::os::unix::fs::OpenOptionsExt;
    use std::sync::mpsc;
    use std::time::Duration;

    use rstest::rstest;
    use serde_json::json;

    use super::*;
    use crate::test_util::TempDir;

    const WAIT: Duration = Duration::from_secs(10);

    type Output = (OwnedFd, Option<OwnedFd>);

    struct Following {
        server: UnixStream,
        heard: mpsc::Receiver<Value>,
        ended: mpsc::Receiver<std::result::Result<(), String>>,
    }

    impl Following {
        fn start(out: OwnedFd) -> Self {
            let (ours, server) = UnixStream::pair().expect("a socket pair");
            let (told, heard) = mpsc::channel();
            let (done, ended) = mpsc::channel();
            thread::spawn(move || {
                let followed = follow_on(ours, watched(out.as_fd()), |value| Ok(told.send(value).is_err()));
                let _ = done.send(followed.map_err(|e| e.to_string()));
            });
            Self { server, heard, ended }
        }

        fn say(&mut self, n: u64) {
            let text = serde_json::to_string(&Response::Ok(json!({ "n": n }))).expect("json");
            protocol::send(&mut self.server, &ServerMessage::Response(text)).expect("a message");
            assert_eq!(self.heard.recv_timeout(WAIT).expect("the message"), json!({ "n": n }));
        }
    }

    fn pipe() -> Output {
        let (reader, out) = io::pipe().expect("a pipe");
        (out.into(), Some(reader.into()))
    }

    fn socket_pair() -> Output {
        let (out, reader) = UnixStream::pair().expect("a socket pair");
        (out.into(), Some(reader.into()))
    }

    fn socket_with_bytes_to_read() -> Output {
        let (out, mut reader) = UnixStream::pair().expect("a socket pair");
        reader.write_all(b"typed").expect("bytes for the command to read");
        (out.into(), Some(reader.into()))
    }

    fn named_fifo() -> Output {
        let dir = TempDir::new();
        let path = dir.path().join("fifo");
        assert!(std::process::Command::new("mkfifo").arg(&path).status().expect("mkfifo").success());
        let nonblocking = rustix::fs::OFlags::NONBLOCK.bits().cast_signed();
        let reader = OpenOptions::new().read(true).custom_flags(nonblocking).open(&path).expect("its reader");
        let out = OpenOptions::new().write(true).open(&path).expect("its writer");
        (out.into(), Some(reader.into()))
    }

    fn file() -> Output {
        let dir = TempDir::new();
        (std::fs::File::create(dir.path().join("out")).expect("a file").into(), None)
    }

    fn dev_null() -> Output {
        (OpenOptions::new().write(true).open("/dev/null").expect("/dev/null").into(), None)
    }

    #[rstest]
    #[case::pipe(pipe())]
    #[case::socket_pair(socket_pair())]
    fn ends_soon_after_its_reader_leaves_and_the_server_reads_the_end(#[case] output: Output) {
        let (out, reader) = output;
        let mut following = Following::start(out);
        following.say(1);

        drop(reader);

        assert_eq!(following.ended.recv_timeout(WAIT).expect("it ended"), Ok(()));
        assert!(matches!(protocol::recv::<ClientMessage>(&mut following.server), Ok(None)));
    }

    #[rstest]
    #[case::pipe(pipe())]
    #[case::socket_pair(socket_pair())]
    #[case::named_fifo(named_fifo())]
    #[case::socket_it_could_read(socket_with_bytes_to_read())]
    #[case::file(file())]
    #[case::dev_null(dev_null())]
    fn goes_on_while_its_output_stays(#[case] output: Output) {
        let (out, _reader) = output;
        let mut following = Following::start(out);

        following.say(1);
        following.say(2);
        protocol::send(&mut following.server, &ServerMessage::Shutdown).expect("the end");

        assert_eq!(following.ended.recv_timeout(WAIT).expect("it ended"), Ok(()));
    }

    const SHORT: Duration = Duration::from_millis(50);

    #[test]
    fn a_log_follow_ends_when_its_reader_hangs_up() {
        let (out, reader) = pipe();
        let mut out = watched(out.as_fd());

        drop(reader);

        assert!(reader_gone_within(&mut out, WAIT));
    }

    #[rstest]
    #[case::pipe(pipe())]
    #[case::named_fifo(named_fifo())]
    fn a_log_follow_waits_the_whole_time_while_its_reader_stays(#[case] output: Output) {
        let (out, _reader) = output;
        let mut out = watched(out.as_fd());
        let started = std::time::Instant::now();

        assert!(!reader_gone_within(&mut out, SHORT));

        assert!(started.elapsed() >= SHORT && out.is_some());
    }

    #[test]
    fn a_log_follow_stops_watching_a_reader_that_has_bytes_to_read() {
        let (out, _reader) = socket_with_bytes_to_read();
        let mut out = watched(out.as_fd());

        assert!(!reader_gone_within(&mut out, WAIT));

        assert!(out.is_none());
    }

    #[test]
    fn a_log_follow_stops_watching_when_it_cannot_poll() {
        let (out, _reader) = pipe();
        let mut out = watched(out.as_fd());

        assert!(!reader_gone_within(&mut out, Duration::MAX));

        assert!(out.is_none());
    }

    #[test]
    fn a_log_follow_with_nothing_to_watch_just_waits() {
        let started = std::time::Instant::now();

        assert!(!reader_gone_within(&mut None, SHORT));

        assert!(started.elapsed() >= SHORT);
    }

    #[rstest]
    #[case::pipe(pipe(), true)]
    #[case::socket_pair(socket_pair(), true)]
    #[case::named_fifo(named_fifo(), true)]
    #[case::file(file(), false)]
    #[case::dev_null(dev_null(), false)]
    fn watches_standard_output_only_as_a_pipe_or_a_socket(#[case] output: Output, #[case] watches: bool) {
        assert_eq!(watched(output.0.as_fd()).is_some(), watches);
    }
}

mod one_link {
    use super::*;

    fn serving(answers: usize) -> (Link, thread::JoinHandle<usize>) {
        let (ours, mut theirs) = UnixStream::pair().expect("a socket pair");
        let server = thread::spawn(move || {
            let mut seen = 0;
            while seen < answers && protocol::recv::<ClientMessage>(&mut theirs).expect("a request").is_some() {
                let answer = serde_json::to_string(&Response::Ok(serde_json::json!({"n": seen}))).expect("json");
                protocol::send(&mut theirs, &ServerMessage::Response(answer)).expect("an answer");
                seen += 1;
            }
            seen
        });
        (Link::socket(ours).expect("a link"), server)
    }

    #[test]
    fn asks_one_request_after_another_over_it() {
        let (mut link, server) = serving(2);

        let first = link.ask("input", control::Command::Contract(control::Contract {})).expect("first");
        let second = link.ask("input", control::Command::Contract(control::Contract {})).expect("second");

        assert_eq!((first["n"].clone(), second["n"].clone()), (0.into(), 1.into()));
        assert_eq!(server.join().expect("the fake server"), 2);
    }

    #[test]
    fn says_the_server_stopped_when_it_left_between_two_requests() {
        let (mut link, server) = serving(1);
        link.ask("input", control::Command::Contract(control::Contract {})).expect("first");
        server.join().expect("the fake server");

        let error = link.ask("input", control::Command::Contract(control::Contract {})).expect_err("gone");

        assert!(matches!(error, Error::ServerGone), "{error:?}");
    }
}
