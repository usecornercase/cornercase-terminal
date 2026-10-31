use std::io;
use std::sync::mpsc;

use rstest::rstest;

use super::*;

fn answer_of(line: &str) -> Option<Answer> {
    match ran(line.as_bytes())? {
        Ran::Answer(answer) => Some(answer),
        Ran::Ended(error) => panic!("ended: {error}"),
    }
}

mod a_line {
    use super::*;

    #[test]
    fn that_is_blank_gets_no_answer() {
        assert!(answer_of("").is_none());
        assert!(answer_of("  \r").is_none());
    }

    #[rstest]
    #[case::not_json("not json", "not a request line")]
    #[case::args_not_a_list(r#"{"args": "status"}"#, "not a request line")]
    #[case::an_unknown_key(r#"{"args": ["status"], "stdni": "x"}"#, "unknown field `stdni`")]
    #[case::no_args(r#"{"stdin": "x"}"#, "missing field `args`")]
    #[case::no_command(r#"{"args": []}"#, "needs a command")]
    #[case::remote(r#"{"args": ["--remote", "devbox", "status"]}"#, "takes no --remote")]
    #[case::remote_command(r#"{"args": ["--remote-command", "cc", "status"]}"#, "takes no --remote")]
    #[case::kill_server(r#"{"args": ["kill-server"]}"#, "run `cornercase kill-server` on its own")]
    #[case::update(r#"{"args": ["update", "--yes"]}"#, "run `cornercase update` on its own")]
    #[case::restart(r#"{"args": ["restart", "--when-idle"]}"#, "run `cornercase restart` on its own")]
    #[case::logs(r#"{"args": ["logs"]}"#, "run `cornercase logs` on its own")]
    #[case::start_server(r#"{"args": ["start-server"]}"#, "run `cornercase start-server` first")]
    #[case::remote_window(r#"{"args": ["remote", "devbox"]}"#, "`remote` cannot run")]
    #[case::skill(r#"{"args": ["skill"]}"#, "`skill` cannot run")]
    #[case::plugin_here(r#"{"args": ["plugin", "link", "/x"]}"#, "acts on this machine")]
    fn that_is_wrong_or_refused_gets_status_2_and_why(#[case] line: &str, #[case] why: &str) {
        let answer = answer_of(line).expect("an answer");

        assert_eq!(answer.status, 2, "{answer:?}");
        let error = answer.error.expect("an error");
        assert!(error.contains(why), "{error}");
    }

    #[test]
    fn with_a_usage_error_gives_clap_s_message_without_its_prefix() {
        let answer = answer_of(r#"{"args": ["focus", "--bogus"]}"#).expect("an answer");

        assert_eq!(answer.status, 2);
        let error = answer.error.expect("an error");
        assert!(error.starts_with("unexpected argument '--bogus'"), "{error}");
        assert!(!error.starts_with("error:"), "{error}");
    }

    #[test]
    fn asking_for_help_prints_it_with_status_0() {
        let answer = answer_of(r#"{"args": ["focus", "--help"]}"#).expect("an answer");

        assert_eq!(answer.status, 0);
        assert!(answer.text.contains("Examples:"), "{}", answer.text);
        assert!(answer.error.is_none());
    }

    #[test]
    fn asking_for_the_version_prints_it_with_status_0() {
        let answer = answer_of(r#"{"args": ["--version"]}"#).expect("an answer");

        assert_eq!(answer.status, 0);
        assert!(answer.text.starts_with("cornercase "), "{}", answer.text);
    }
}

mod an_outcome {
    use super::*;

    fn of(result: Result<bool>) -> Answer {
        outcome(result, Printed::default(), "changes list")
    }

    #[rstest]
    #[case::done(Ok(true), 0)]
    #[case::failed(Ok(false), 1)]
    #[case::partial(Err(Error::Partial), 3)]
    #[case::exited(Err(Error::Exited(7)), 7)]
    #[case::wrong_usage(Err(Error::WrongUsage("no".into())), 2)]
    #[case::own_connection(Err(Error::OwnConnection), 2)]
    #[case::other(Err(Error::Control("bad".into())), 1)]
    fn maps_to_the_status_the_command_would_have_had(#[case] result: Result<bool>, #[case] status: u8) {
        assert_eq!(of(result).status, status);
    }

    #[test]
    fn a_failure_without_a_reason_has_no_error() {
        assert!(of(Ok(false)).error.is_none());
    }

    #[test]
    fn a_stream_names_the_command_to_run_on_its_own() {
        let error = of(Err(Error::OwnConnection)).error.expect("an error");

        assert!(error.contains("`changes list` keeps a connection of its own"), "{error}");
    }

    #[test]
    fn keeps_what_the_command_printed() {
        let printed = Printed { json: Some(serde_json::json!({"a": 1})), text: "t".into(), stderr: "s".into() };

        let answer = outcome(Ok(true), printed, "x");

        let text = serde_json::to_string(&answer).expect("json");
        assert_eq!(text, r#"{"status":0,"json":{"a":1},"text":"t","stderr":"s"}"#);
    }
}

mod serving {
    use super::*;

    fn lines(lines: Vec<io::Result<&str>>) -> Lines {
        let (tx, rx) = mpsc::channel();
        for line in lines {
            tx.send(line.map(|text| text.as_bytes().to_vec())).expect("sent");
        }
        rx
    }

    fn answering(status: u8) -> Ran {
        Ran::Answer(Answer { status, ..Answer::default() })
    }

    fn served(input: Vec<io::Result<&str>>, mut run: impl FnMut(&[u8]) -> Option<Ran>) -> (Result<()>, String) {
        let mut out = Vec::new();
        let done = serve(&lines(input), &mut out, &mut run);
        (done, String::from_utf8(out).expect("text"))
    }

    #[test]
    fn answers_each_non_blank_line_in_order_in_one_line_each() {
        let mut count = 0;
        let (done, out) = served(vec![Ok("a"), Ok(""), Ok("b")], |bytes| {
            count += 1;
            if bytes.is_empty() { None } else { Some(answering(0)) }
        });

        assert!(done.is_ok());
        assert_eq!(out, "{\"status\":0}\n{\"status\":0}\n");
        assert_eq!(count, 3);
    }

    #[test]
    fn a_refused_line_does_not_stop_the_next_and_ends_with_3() {
        let mut statuses = [2, 0].into_iter();
        let (done, out) = served(vec![Ok("a"), Ok("b")], |_| Some(answering(statuses.next().expect("a status"))));

        assert!(matches!(done, Err(Error::Partial)), "{done:?}");
        assert_eq!(out.lines().count(), 2);
    }

    #[test]
    fn the_server_leaving_ends_it_with_no_answer_for_that_line() {
        let (done, out) = served(vec![Ok("a"), Ok("b")], |_| Some(Ran::Ended(Error::ServerGone)));

        assert!(matches!(done, Err(Error::ServerGone)), "{done:?}");
        assert_eq!(out, "");
    }

    #[test]
    fn the_reader_leaving_ends_it_quietly() {
        let (done, out) = served(vec![Ok("a")], |_| Some(Ran::Ended(Error::ReaderGone)));

        assert!(done.is_ok(), "{done:?}");
        assert_eq!(out, "");
    }

    #[test]
    fn a_closed_output_ends_it_quietly() {
        let done = serve(&lines(vec![Ok("a"), Ok("b")]), &mut crate::test_util::BrokenPipe, |_| Some(answering(0)));

        assert!(done.is_ok(), "{done:?}");
    }

    #[test]
    fn a_read_error_comes_after_the_lines_before_it() {
        let input = vec![Ok("a"), Ok("b"), Err(io::Error::other("broken input"))];

        let (done, out) = served(input, |_| Some(answering(0)));

        assert!(matches!(done, Err(Error::Io(_))), "{done:?}");
        assert_eq!(out.lines().count(), 2);
    }
}

mod streams {
    use std::os::unix::net::UnixStream;
    use std::thread;

    use super::*;
    use crate::control;
    use crate::protocol::{self, ClientMessage, ServerMessage};

    fn fake_server(mut stream: UnixStream) -> thread::JoinHandle<Vec<String>> {
        let features = control::contract::describe("boot", &[]);
        thread::spawn(move || {
            let mut seen = Vec::new();
            while let Ok(Some(ClientMessage::Request(text))) = protocol::recv::<ClientMessage>(&mut stream) {
                seen.push(text);
                let value = serde_json::to_value(&features).expect("json");
                let answer = serde_json::to_string(&control::Response::Ok(value)).expect("json");
                protocol::send(&mut stream, &ServerMessage::Response(answer)).expect("an answer");
            }
            seen
        })
    }

    #[test]
    fn plugin_status_json_lands_in_json_not_text() {
        let (ours, theirs) = UnixStream::pair().expect("a socket pair");
        let server = fake_server(theirs);
        client::share_socket(ours);

        let answer = answer_of(r#"{"args": ["plugin", "status", "--json"]}"#).expect("an answer");

        client::unshare();
        server.join().expect("the fake server");
        assert_eq!(answer.status, 0, "{answer:?}");
        assert!(answer.json.is_some(), "{answer:?}");
        assert_eq!(answer.text, "");
    }

    #[rstest]
    #[case::events(r#"{"args": ["events"]}"#)]
    #[case::surface(r#"{"args": ["surface", "--tab", "1", "--cols", "80", "--rows", "24"]}"#)]
    #[case::view(r#"{"args": ["view"]}"#)]
    #[case::input_follow(r#"{"args": ["input", "--follow", "-"]}"#)]
    #[case::status_follow(r#"{"args": ["status", "--follow"]}"#)]
    #[case::changes_list(r#"{"args": ["changes", "list"]}"#)]
    #[case::flow_messages(r#"{"args": ["flow", "messages", "--follow"]}"#)]
    #[case::requests(r#"{"args": ["requests", "--follow", "-"]}"#)]
    fn a_stream_is_refused_before_it_sends_anything(#[case] line: &str) {
        let (ours, theirs) = UnixStream::pair().expect("a socket pair");
        let server = fake_server(theirs);
        client::share_socket(ours);

        let answer = answer_of(line).expect("an answer");

        client::unshare();
        let seen = server.join().expect("the fake server");
        assert_eq!(answer.status, 2, "{answer:?}");
        let error = answer.error.expect("an error");
        assert!(error.contains("connection of its own"), "{error}");
        assert!(seen.iter().all(|text| text.contains("\"contract\"")), "{seen:?}");
    }
}
