use std::thread;

use super::*;
use crate::control::Response;
use crate::os::socket::UnixStream;

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

mod how_a_follow_ends {
    use rstest::rstest;
    use serde_json::json;

    use super::*;

    #[rstest]
    #[case::detached(ServerMessage::Detached, StreamEnd::Detached)]
    #[case::shutdown(ServerMessage::Shutdown, StreamEnd::Stopped)]
    #[case::restart(ServerMessage::Restart(std::path::PathBuf::from("cornercase")), StreamEnd::Restarted)]
    fn says_which_message_ended_it(#[case] message: ServerMessage, #[case] expected: StreamEnd) {
        let (ours, mut theirs) = UnixStream::pair().expect("a socket pair");
        let line = serde_json::to_string(&Response::Ok(json!({ "n": 1 }))).expect("json");
        protocol::send(&mut theirs, &ServerMessage::Response(line)).expect("a line");
        protocol::send(&mut theirs, &message).expect("the end");
        let mut heard = Vec::new();

        let ended = follow_on(ours, None, |value| {
            heard.push(value);
            Ok(false)
        });

        assert_eq!(ended.expect("it ended"), expected);
        assert_eq!(heard, [json!({ "n": 1 })]);
    }
}

mod features {
    use super::*;

    #[test]
    fn a_failure_to_reach_the_server_is_not_taken_for_a_missing_feature() {
        let checked = require_on(Err(Error::NoServer), "accounts", "has no account profiles", None).map(drop);

        assert!(matches!(checked, Err(Error::NoServer)), "{checked:?}");
    }

    fn offering(features: &[&str]) -> (Result<()>, Vec<String>) {
        asking(features, "accounts", "has no account profiles")
    }

    fn serving(features: &[&str]) -> (UnixStream, thread::JoinHandle<Vec<String>>) {
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
        (ours, server)
    }

    fn asking(features: &[&str], feature: &str, lacks: &str) -> (Result<()>, Vec<String>) {
        let (ours, server) = serving(features);
        let checked = require_on(Link::socket(ours), feature, lacks, None).map(drop);
        (checked, server.join().expect("the fake server"))
    }

    #[test]
    fn a_passed_check_hands_its_link_to_the_request() {
        let (ours, server) = serving(&["views"]);

        let mut link = require_on(Link::socket(ours), "views", "has no views", None).expect("accepted");
        link.ask("status", control::Command::Status(control::Status {})).expect("an answer");
        drop(link);

        let asked = server.join().expect("the fake server");
        assert_eq!(asked.len(), 2, "{asked:?}");
        assert!(asked[0].contains("\"contract\"") && asked[1].contains("\"status\""), "{asked:?}");
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
    fn a_server_that_cannot_start_on_a_machine_is_refused_before_the_start_is_sent() {
        let lacks = "cannot start agents, workspaces or flows on another machine";
        let (checked, asked) = asking(&["machine-usage"], "machine-start", lacks);

        let error = checked.expect_err("refused");
        assert!(error.to_string().contains(lacks), "{error}");
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

mod the_envelope {
    use rstest::rstest;

    use super::*;

    fn full() -> Envelope {
        Envelope { view: Some(7), boot: Some("1-2".into()) }
    }

    #[rstest]
    #[case::stays_out_of_the_contract(control::Command::Contract(control::Contract {}), Envelope::default())]
    #[case::goes_with_a_command_that_takes_one(control::Command::Status(control::Status {}), full())]
    #[case::goes_with_one_that_takes_none_for_the_server_to_refuse(
        control::Command::Stats(control::Stats::default()),
        full()
    )]
    fn of_the_invocation_goes_with_every_request_but_the_contract(
        #[case] command: control::Command,
        #[case] sent: Envelope,
    ) {
        assert_eq!(full().for_command(&command), sent);
    }
}

mod a_request_without_a_boot {
    use super::*;

    fn keys_sent(envelope: Envelope) -> Vec<String> {
        let mut framed = Vec::new();
        send_request(&mut framed, control::Command::Status(control::Status {}), envelope).expect("written");
        let Some(ClientMessage::Request(text)) =
            protocol::recv::<ClientMessage>(&mut framed.as_slice()).expect("a frame")
        else {
            panic!("not a request")
        };
        let sent: Value = serde_json::from_str(&text).expect("json");
        sent.as_object().expect("an object").keys().cloned().collect()
    }

    #[test]
    fn has_only_the_keys_a_command_always_sent() {
        assert_eq!(keys_sent(Envelope::default()), ["args", "caller", "command", "server"]);
    }

    #[test]
    fn differs_from_one_with_a_boot_only_by_that_key() {
        let boot = Envelope { boot: Some("1-2".into()), ..Envelope::default() };

        assert_eq!(keys_sent(boot), ["args", "boot", "caller", "command", "server"]);
    }
}

mod an_answer {
    use rstest::rstest;
    use serde_json::json;

    use super::*;
    use crate::control::contract::{Code, Refusal, render};
    use crate::test_util::fixture;

    const MISSING: &str = "there is no pane 9; `cornercase status` lists them";

    #[rstest]
    #[case::legacy(fixture!("control/responses/legacy-ok.json"))]
    #[case::structured(fixture!("control/responses/structured-ok.json"))]
    fn that_went_well_reads_the_same_in_both_modes(#[case] text: &str) {
        assert_eq!(answered(text).expect("an answer"), json!({"pane": 4, "tab": 3}));
    }

    #[rstest]
    #[case::legacy(fixture!("control/responses/legacy-error.json"))]
    #[case::structured(fixture!("control/responses/structured-error.json"))]
    fn that_refuses_gives_its_message_in_both_modes(#[case] text: &str) {
        let error = answered(text).expect_err("refused");

        assert!(matches!(&error, Error::Control(message) if message == MISSING), "{error:?}");
    }

    #[test]
    fn that_says_the_boot_is_stale_is_told_apart_and_keeps_its_message() {
        let text = render(Err(Refusal::new(Code::StaleBoot, "it restarted")), true).expect("an answer");

        let error = answered(&text).expect_err("refused");

        assert!(matches!(error, Error::StaleBoot(_)), "{error:?}");
        assert_eq!(error.to_string(), "it restarted");
    }

    #[test]
    fn with_a_code_this_release_does_not_know_still_gives_its_message() {
        let error = answered(r#"{"error":{"code":"from-later","message":"why"}}"#).expect_err("refused");

        assert!(matches!(&error, Error::Control(message) if message == "why"), "{error:?}");
    }

    #[test]
    fn that_cannot_be_read_says_so() {
        let error = answered(r#"{"error":{"code":"failed"}}"#).expect_err("unreadable");

        assert!(error.to_string().starts_with("cannot read the server's answer"), "{error}");
    }
}

#[cfg(unix)]
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
        ended: mpsc::Receiver<std::result::Result<StreamEnd, String>>,
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

        assert_eq!(following.ended.recv_timeout(WAIT).expect("it ended"), Ok(StreamEnd::Left));
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

        assert_eq!(following.ended.recv_timeout(WAIT).expect("it ended"), Ok(StreamEnd::Stopped));
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

#[cfg(unix)]
mod an_input {
    use std::io::{self, Write};
    use std::net::Shutdown;
    use std::os::fd::{AsFd, OwnedFd};
    use std::time::{Duration, Instant};

    use rstest::rstest;

    use super::*;

    const WAIT: Duration = Duration::from_secs(10);
    const SHORT: Duration = Duration::from_millis(50);

    fn closed_pipe() -> OwnedFd {
        let (reader, writer) = io::pipe().expect("a pipe");
        drop(writer);
        reader.into()
    }

    #[rstest]
    #[case::pipe(true)]
    #[case::socket_pair_closed(false)]
    fn ends_when_the_other_end_closes(#[case] pipe: bool) {
        let (input, keep): (OwnedFd, Box<dyn std::any::Any>) = if pipe {
            let (reader, writer) = io::pipe().expect("a pipe");
            (reader.into(), Box::new(writer))
        } else {
            let (ours, peer) = UnixStream::pair().expect("a socket pair");
            (ours.into(), Box::new(peer))
        };
        drop(keep);

        assert!(input_ended(None, &mut Some(input.as_fd()), Some(WAIT)));
    }

    #[test]
    fn ends_when_a_socket_peer_only_shuts_its_writing_half() {
        let (ours, peer) = UnixStream::pair().expect("a socket pair");
        peer.shutdown(Shutdown::Write).expect("shut");

        assert!(input_ended(None, &mut Some(ours.as_fd()), Some(WAIT)));
    }

    #[test]
    fn an_end_already_there_is_seen_without_waiting() {
        let input = closed_pipe();

        assert!(input_ended(None, &mut Some(input.as_fd()), Some(Duration::ZERO)));
    }

    #[test]
    fn an_open_input_does_not_end_at_once() {
        let (reader, _writer) = io::pipe().expect("a pipe");
        let started = Instant::now();

        assert!(!input_ended(None, &mut Some(reader.as_fd()), Some(Duration::ZERO)));

        assert!(started.elapsed() < WAIT);
    }

    #[test]
    fn bytes_are_not_an_end() {
        let (ours, mut peer) = UnixStream::pair().expect("a socket pair");
        peer.write_all(b"x").expect("bytes");
        let mut input = Some(ours.as_fd());

        assert!(!input_ended(None, &mut input, Some(SHORT)));

        assert!(input.is_some());
    }

    #[test]
    fn a_server_message_comes_first() {
        let (server, mut theirs) = UnixStream::pair().expect("a socket pair");
        theirs.write_all(b"x").expect("bytes");
        let (reader, _writer) = io::pipe().expect("a pipe");
        let mut input = Some(reader.as_fd());

        assert!(!input_ended(Some(server.as_fd()), &mut input, None));

        assert!(input.is_some());
    }

    #[test]
    fn nothing_to_watch_never_ends() {
        assert!(!input_ended(None, &mut None, Some(SHORT)));
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

mod a_shared_link {
    #[cfg(unix)]
    use std::time::Duration;

    use super::*;

    #[cfg(unix)]
    const WAIT: Duration = Duration::from_secs(10);

    type Seen = thread::JoinHandle<Vec<String>>;

    fn status() -> control::Command {
        control::Command::Status(control::Status {})
    }

    fn sharing(features: &[&str], watch: Option<console::OwnedFd>) -> Seen {
        let (ours, mut theirs) = UnixStream::pair().expect("a socket pair");
        let mut descriptor = control::contract::describe("boot", &[]);
        descriptor.features.retain(|feature| features.contains(&feature.as_str()));
        let server = thread::spawn(move || {
            let mut seen = Vec::new();
            while let Ok(Some(ClientMessage::Request(text))) = protocol::recv::<ClientMessage>(&mut theirs) {
                let value = reply(&text, &descriptor, seen.len());
                seen.push(text);
                let answer = serde_json::to_string(&Response::Ok(value)).expect("json");
                protocol::send(&mut theirs, &ServerMessage::Response(answer)).expect("an answer");
            }
            seen
        });
        share(Link::socket(ours).expect("a link"), watch);
        server
    }

    fn reply(text: &str, descriptor: &control::contract::Descriptor, n: usize) -> Value {
        if text.contains("\"contract\"") {
            serde_json::to_value(descriptor).expect("json")
        } else {
            serde_json::json!({"n": n})
        }
    }

    fn finish(server: Seen) -> Vec<String> {
        unshare();
        server.join().expect("the fake server")
    }

    #[test]
    fn asks_one_after_another_over_the_same_socket() {
        let server = sharing(&[], None);

        let answers: Vec<Value> = (0..3).map(|_| ask("status", status()).expect("an answer")).collect();

        assert_eq!(answers.iter().map(|answer| answer["n"].clone()).collect::<Vec<_>>(), [0, 1, 2]);
        assert_eq!(finish(server).len(), 3);
    }

    #[test]
    fn asks_the_contract_once_for_every_check() {
        let server = sharing(&["focus-reveal", "accounts"], None);

        use_focus_reveal().expect("offered");
        use_accounts().expect("offered");
        use_focus_reveal().expect("offered");

        let seen = finish(server);
        assert_eq!(seen.iter().filter(|text| text.contains("\"contract\"")).count(), 1, "{seen:?}");
    }

    #[test]
    fn refuses_a_feature_the_server_lacks_like_one_shot_commands_do() {
        let server = sharing(&[], None);

        let error = use_accounts().expect_err("lacking");

        assert!(error.to_string().contains("has no account profiles"), "{error}");
        assert!(line_done().is_none());
        finish(server);
    }

    #[test]
    fn a_lines_view_and_boot_go_together() {
        let server = sharing(&["views", "boot"], None);

        use_view(7).expect("views");
        use_boot("1-2").expect("offered");
        ask("status", status()).expect("an answer");

        let seen = finish(server);
        let last = seen.last().expect("the status");
        assert!(last.contains("\"view\":7") && last.contains("\"boot\":\"1-2\""), "{seen:?}");
        assert_eq!(seen.iter().filter(|text| text.contains("\"contract\"")).count(), 1, "{seen:?}");
    }

    #[test]
    fn a_server_that_would_ignore_the_boot_is_refused_before_anything_but_the_contract_is_sent() {
        let server = sharing(&["views"], None);

        let error = use_boot("1-2").expect_err("lacking");

        assert!(error.to_string().contains("would ignore --boot"), "{error}");
        assert!(line_done().is_none());
        let seen = finish(server);
        assert_eq!(seen.len(), 1, "{seen:?}");
        assert!(seen[0].contains("\"contract\""), "{seen:?}");
    }

    #[rstest::rstest]
    #[case::view(|| use_view(7), "\"view\":7")]
    #[case::boot(|| use_boot("1-2"), "\"boot\":\"1-2\"")]
    fn what_a_line_asks_for_goes_with_that_line_only(#[case] use_it: fn() -> Result<()>, #[case] sent: &str) {
        let server = sharing(&["views", "boot"], None);

        use_it().expect("offered");
        ask("status", status()).expect("an answer");
        assert!(line_done().is_none());
        ask("status", status()).expect("an answer");

        let seen = finish(server);
        assert!(seen[0].contains("\"contract\"") && !seen[0].contains(sent), "{seen:?}");
        assert!(seen[1].contains(sent), "{seen:?}");
        assert!(!seen[2].contains(sent), "{seen:?}");
    }

    #[test]
    fn a_command_with_a_connection_of_its_own_is_refused_before_anything_is_sent() {
        let server = sharing(&[], None);

        let connected = connect().err().expect("refused");
        let followed = follow("events", status(), |_| Ok(false)).expect_err("refused");

        assert!(matches!(connected, Error::OwnConnection), "{connected:?}");
        assert!(matches!(followed, Error::OwnConnection), "{followed:?}");
        assert_eq!(finish(server), Vec::<String>::new());
    }

    #[test]
    fn a_lost_link_is_reported_when_the_line_ends() {
        let (ours, theirs) = UnixStream::pair().expect("a socket pair");
        share(Link::socket(ours).expect("a link"), None);
        drop(theirs);

        let error = ask("status", status()).expect_err("gone");

        assert!(matches!(error, Error::ServerGone), "{error:?}");
        assert!(matches!(line_done(), Some(Error::ServerGone)));
        assert!(line_done().is_none());
        unshare();
    }

    #[rstest::rstest]
    #[case::refused(r#"{"error":"no such pane"}"#, "no such pane")]
    #[case::refused_with_a_code(r#"{"error":{"code":"not-found","message":"no such pane"}}"#, "no such pane")]
    #[case::from_an_earlier_boot(r#"{"error":{"code":"stale-boot","message":"it restarted"}}"#, "it restarted")]
    fn a_command_error_does_not_lose_the_link(#[case] refusal: &'static str, #[case] message: &str) {
        let (ours, mut theirs) = UnixStream::pair().expect("a socket pair");
        share(Link::socket(ours).expect("a link"), None);
        let server = thread::spawn(move || {
            protocol::recv::<ClientMessage>(&mut theirs).expect("a request");
            protocol::send(&mut theirs, &ServerMessage::Response(refusal.into())).expect("an answer");
        });

        let error = ask("status", status()).expect_err("refused");

        server.join().expect("the fake server");
        assert_eq!(error.to_string(), message);
        assert!(line_done().is_none());
        unshare();
    }

    #[cfg(unix)]
    #[test]
    fn an_ask_ends_when_its_reader_leaves_and_the_server_reads_the_end() {
        let (reader, out) = io::pipe().expect("a pipe");
        let (ours, mut theirs) = UnixStream::pair().expect("a socket pair");
        let link = Link::socket(ours).expect("a link");
        let (asked, asking) = std::sync::mpsc::channel();
        let server = thread::spawn(move || {
            protocol::recv::<ClientMessage>(&mut theirs).expect("a request");
            asked.send(()).expect("told");
            protocol::recv::<ClientMessage>(&mut theirs)
        });
        let client = thread::spawn(move || {
            share(link, Some(out.into()));
            let error = ask("status", status()).expect_err("the reader left");
            let ended = line_done();
            unshare();
            (error, ended)
        });
        asking.recv_timeout(WAIT).expect("the request arrived");

        drop(reader);

        let (error, ended) = client.join().expect("the client");
        assert!(matches!(error, Error::ReaderGone), "{error:?}");
        assert!(matches!(ended, Some(Error::ReaderGone)));
        assert!(matches!(server.join().expect("the fake server"), Ok(None)));
    }

    #[test]
    fn a_server_without_a_contract_offers_no_features_but_keeps_the_link() {
        let (ours, mut theirs) = UnixStream::pair().expect("a socket pair");
        share(Link::socket(ours).expect("a link"), None);
        let server = thread::spawn(move || {
            protocol::recv::<ClientMessage>(&mut theirs).expect("a request");
            let refusal = serde_json::to_string(&Response::Error(control::unknown_command("contract"))).expect("json");
            protocol::send(&mut theirs, &ServerMessage::Response(refusal)).expect("an answer");
        });

        let error = use_accounts().expect_err("lacking");

        server.join().expect("the fake server");
        assert!(error.to_string().contains("has no account profiles"), "{error}");
        assert!(line_done().is_none());
        unshare();
    }
}

mod a_stream_from_an_earlier_run {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::the_first_answer_is_printed(true)]
    #[case::only_what_follows_is_printed(false)]
    fn is_refused_before_its_first_line(#[case] first: bool) {
        let (ours, mut theirs) = UnixStream::pair().expect("a socket pair");
        let server = thread::spawn(move || {
            protocol::recv::<ClientMessage>(&mut theirs).expect("a request");
            let refusal = control::contract::Refusal::new(control::contract::Code::StaleBoot, "it restarted");
            let refusal = control::contract::render(Err(refusal), true).expect("an answer");
            protocol::send(&mut theirs, &ServerMessage::Response(refusal)).expect("an answer");
            let line = serde_json::to_string(&Response::Ok(serde_json::json!({"line": 1}))).expect("json");
            let _ = protocol::send(&mut theirs, &ServerMessage::Response(line));
        });
        let mut printed = Vec::new();

        let followed = follow_link(
            &mut Link::socket(ours).expect("a link"),
            "events",
            control::Command::Events(control::Events::default()),
            first,
            |value| {
                printed.push(value);
                Ok(false)
            },
        );

        server.join().expect("the fake server");
        assert!(matches!(followed, Err(Error::StaleBoot(_))), "{followed:?}");
        assert!(printed.is_empty(), "{printed:?}");
    }
}
