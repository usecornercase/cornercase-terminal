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
    #[case::known_machines(r#"{"args": ["remote", "--known", "--json"]}"#, "`remote` cannot run")]
    #[case::remember_a_machine(r#"{"args": ["remote", "--remember", "devbox"]}"#, "`remote` cannot run")]
    #[case::skill(r#"{"args": ["skill"]}"#, "`skill` cannot run")]
    #[case::plugin_link(r#"{"args": ["plugin", "link", "/x"]}"#, "run `cornercase plugin link /x` on its own")]
    #[case::plugin_windows(
        r#"{"args": ["plugin", "windows", "--json"]}"#,
        "run `cornercase plugin windows` on its own"
    )]
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
    #[case::stale_boot(Err(Error::StaleBoot("it restarted".into())), 4)]
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
    use std::thread;

    use super::*;
    use crate::control;
    use crate::os::socket::UnixStream;
    use crate::protocol::{self, ClientMessage, ServerMessage};

    const NO_REQUEST_NAMES_THIS: &str = "no request names this";

    type Seen = thread::JoinHandle<Vec<String>>;

    fn fake_server(stream: UnixStream) -> Seen {
        serving(stream, control::contract::describe("boot", &[]), NO_REQUEST_NAMES_THIS)
    }

    fn lines_on(serve: impl FnOnce(UnixStream) -> Seen, lines: &[&str]) -> (Vec<Answer>, Vec<String>) {
        let (ours, theirs) = UnixStream::pair().expect("a socket pair");
        let server = serve(theirs);
        client::share_socket(ours);
        let answers = lines.iter().map(|line| answer_of(line).expect("an answer")).collect();
        client::unshare();
        (answers, server.join().expect("the fake server"))
    }

    fn answer_to(text: &str, features: &control::contract::Descriptor, stale: &str) -> String {
        if text.contains(stale) {
            let refusal = control::contract::Refusal::new(control::contract::Code::StaleBoot, "it restarted");
            return control::contract::render(Err(refusal), true).expect("an answer");
        }
        let value = serde_json::to_value(features).expect("json");
        serde_json::to_string(&control::Response::Ok(value)).expect("json")
    }

    fn serving(mut stream: UnixStream, features: control::contract::Descriptor, stale: &'static str) -> Seen {
        thread::spawn(move || {
            let mut seen = Vec::new();
            while let Ok(Some(ClientMessage::Request(text))) = protocol::recv::<ClientMessage>(&mut stream) {
                let answer = answer_to(&text, &features, stale);
                seen.push(text);
                protocol::send(&mut stream, &ServerMessage::Response(answer)).expect("an answer");
            }
            seen
        })
    }

    const FLOW_CLOSING: &str = r#"{"args": ["flow", "run", "Pair", "--prompt", "x", "--close-when-merged", "--json"]}"#;

    #[test]
    fn a_flow_that_closes_when_merged_is_refused_up_front_by_a_server_that_would_drop_it() {
        let (answer, seen) = in_a_view_on(FLOW_CLOSING, lacking("flow-close-when-merged"));

        assert_eq!(answer.status, 1, "{answer:?}");
        let error = answer.error.expect("an error");
        assert!(error.contains("cannot close a flow's agents") && error.contains("update cornercase"), "{error}");
        assert!(!seen.iter().any(|text| text.contains("flow-run")), "{seen:?}");
    }

    #[test]
    fn a_flow_that_closes_when_merged_sends_the_field() {
        let (answer, seen) = in_a_view_on(FLOW_CLOSING, control::contract::describe("boot", &[]));

        assert_eq!(answer.status, 0, "{answer:?}");
        let run = seen.iter().find(|text| text.contains("flow-run")).expect("the run");
        assert!(run.contains("\"close_when_merged\":true"), "{run}");
    }

    #[rstest::rstest]
    #[case::run(
        r#"{"args": ["flow", "run", "Pair", "--prompt", "x", "--close-when-merged", "--free-builds", "--json"]}"#,
        "flow-run"
    )]
    #[case::close(
        r#"{"args": ["flow", "close-when-merged", "--workspace", "7", "--free-builds", "--json"]}"#,
        "flow-close-when-merged"
    )]
    fn freeing_builds_is_refused_up_front_by_a_server_that_would_drop_it(#[case] args: &str, #[case] request: &str) {
        let (answer, seen) = in_a_view_on(args, lacking("flow-free-builds"));

        assert_eq!(answer.status, 1, "{answer:?}");
        let error = answer.error.expect("an error");
        assert!(error.contains("cannot free a flow's builds") && error.contains("update cornercase"), "{error}");
        assert!(!seen.iter().any(|text| text.contains(request)), "{seen:?}");
    }

    #[rstest::rstest]
    #[case::run(
        r#"{"args": ["flow", "run", "Pair", "--prompt", "x", "--close-when-merged", "--free-builds", "--json"]}"#,
        "flow-run"
    )]
    #[case::close(
        r#"{"args": ["flow", "close-when-merged", "--workspace", "7", "--free-builds", "--json"]}"#,
        "flow-close-when-merged"
    )]
    fn freeing_builds_sends_the_field(#[case] args: &str, #[case] request: &str) {
        let (answer, seen) = in_a_view_on(args, control::contract::describe("boot", &[]));

        assert_eq!(answer.status, 0, "{answer:?}");
        let sent = seen.iter().find(|text| text.contains(request)).expect("the request");
        assert!(sent.contains("\"free_builds\":true"), "{sent}");
    }

    const FRESH_START: &str = r#"{"args": ["new-workspace", "b", "--worktree", "--fresh", "--json"]}"#;
    const HEAD_START: &str = r#"{"args": ["new-workspace", "b", "--worktree", "--no-fresh", "--json"]}"#;

    #[test]
    fn a_fresh_branch_is_refused_up_front_by_a_server_that_would_drop_it() {
        let (answer, seen) = in_a_view_on(FRESH_START, lacking("fresh-branches"));

        assert_eq!(answer.status, 1, "{answer:?}");
        let error = answer.error.expect("an error");
        assert!(error.contains("up-to-date default branch") && error.contains("update cornercase"), "{error}");
        assert!(!seen.iter().any(|text| text.contains("new-workspace")), "{seen:?}");
    }

    #[test]
    fn a_fresh_branch_sends_the_field() {
        let (answer, seen) = in_a_view_on(FRESH_START, control::contract::describe("boot", &[]));

        assert_eq!(answer.status, 0, "{answer:?}");
        let new = seen.iter().find(|text| text.contains("new-workspace")).expect("the request");
        assert!(new.contains("\"fresh\":true"), "{new}");
    }

    #[test]
    fn a_branch_from_head_needs_no_feature() {
        let (answer, seen) = in_a_view_on(HEAD_START, lacking("fresh-branches"));

        assert_eq!(answer.status, 0, "{answer:?}");
        let new = seen.iter().find(|text| text.contains("new-workspace")).expect("the request");
        assert!(new.contains("\"fresh\":false"), "{new}");
    }

    fn refused_by_a_later_run(line: &str) {
        let refusing = |theirs| serving(theirs, control::contract::describe("boot", &[]), "\"boot\":\"old\"");

        let (answers, seen) = lines_on(refusing, &[line, r#"{"args": ["close", "--pane", "4"]}"#]);

        let (stale, next) = (&answers[0], &answers[1]);
        assert_eq!((stale.status, stale.error.as_deref()), (4, Some("it restarted")), "{stale:?}");
        assert_eq!(next.status, 0, "{next:?}");
        assert_eq!(seen.len(), 3, "the contract, then each line: {seen:?}");
        assert!(seen[0].contains("\"contract\"") && !seen[0].contains("\"boot\""), "{seen:?}");
        assert!(!seen[2].contains("\"boot\""), "{seen:?}");
    }

    fn never_sent_without_boot(line: &str) {
        let older = lacking("boot");

        let (mut answers, seen) = lines_on(|theirs| serving(theirs, older, NO_REQUEST_NAMES_THIS), &[line]);

        let answer = answers.remove(0);
        assert_eq!(answer.status, 1, "{answer:?}");
        let error = answer.error.expect("an error");
        assert!(error.contains("would ignore --boot"), "{error}");
        assert_eq!(seen.len(), 1, "only the contract: {seen:?}");
        assert!(seen[0].contains("\"contract\""), "{seen:?}");
    }

    #[rstest]
    #[case::close(r#"{"args": ["close", "--pane", "4", "--boot", "old"]}"#)]
    #[case::rename(r#"{"args": ["rename", "--tab", "4", "x", "--boot", "old"]}"#)]
    #[case::mark(r#"{"args": ["mark", "--pane", "4", "--boot", "old"]}"#)]
    #[case::move_pane(r#"{"args": ["move-pane", "--pane", "4", "--target", "5", "--place", "left", "--boot", "old"]}"#)]
    #[case::account_restart(r#"{"args": ["account", "restart", "claude", "--pane", "4", "--boot", "old"]}"#)]
    #[case::context(r#"{"args": ["context", "--pane", "4", "--boot", "old"]}"#)]
    #[case::collapse(r#"{"args": ["collapse", "--pane", "4", "--boot", "old"]}"#)]
    #[case::expand(r#"{"args": ["expand", "--worktree", "fix/login", "--boot", "old"]}"#)]
    #[case::send(r#"{"args": ["send", "--pane", "4", "--enter", "x", "--boot", "old"]}"#)]
    #[case::input(r#"{"args": ["input", "--pane", "4", "-", "--boot", "old"], "stdin": "{\"paste\": \"x\"}"}"#)]
    #[case::paste_file(r#"{"args": ["paste-file", "--pane", "4", "--boot", "old"], "stdin": "a picture"}"#)]
    fn a_boot_from_an_earlier_run_gives_status_4_and_the_next_line_runs_without_it(#[case] line: &str) {
        refused_by_a_later_run(line);
    }

    #[test]
    fn plugin_status_json_lands_in_json_not_text() {
        let (mut answers, _) = lines_on(fake_server, &[r#"{"args": ["plugin", "status", "--json"]}"#]);

        let answer = answers.remove(0);
        assert_eq!(answer.status, 0, "{answer:?}");
        assert!(answer.json.is_some(), "{answer:?}");
        assert_eq!(answer.text, "");
    }

    fn lacking(feature: &str) -> control::contract::Descriptor {
        let mut older = control::contract::describe("boot", &[]);
        older.features.retain(|offered| offered != feature);
        older
    }

    const PLUGIN_RUN: &str = r#"{"args": ["plugin", "run", "a", "b", "--view", "7"]}"#;
    const FLOW_RUN: &str = r#"{"args": ["flow", "run", "Pair", "--prompt", "x", "--focus", "--view", "7", "--json"]}"#;

    fn in_a_view_on(line: &str, features: control::contract::Descriptor) -> (Answer, Vec<String>) {
        let (mut answers, seen) = lines_on(|theirs| serving(theirs, features, NO_REQUEST_NAMES_THIS), &[line]);
        (answers.remove(0), seen)
    }

    #[test]
    fn plugin_run_sends_its_view_with_the_run_and_not_with_the_lookup_of_the_revision() {
        let (answer, seen) = in_a_view_on(PLUGIN_RUN, control::contract::describe("boot", &[]));

        assert_eq!(answer.status, 0, "{answer:?}");
        let lookup = seen.iter().find(|text| text.contains("plugin-actions")).expect("the lookup");
        assert!(!lookup.contains("\"view\""), "{lookup}");
        let run = seen.iter().find(|text| text.contains("plugin-run")).expect("the run");
        assert!(run.contains("\"view\":7"), "{run}");
    }

    #[test]
    fn plugin_run_sends_the_boot_with_the_lookup_of_the_revision_and_with_the_run() {
        let line = r#"{"args": ["plugin", "run", "a", "b", "--pane", "4", "--boot", "now"]}"#;

        let (answer, seen) = in_a_view_on(line, control::contract::describe("boot", &[]));

        assert_eq!(answer.status, 0, "{answer:?}");
        for request in ["plugin-actions", "plugin-run"] {
            let sent = seen.iter().find(|text| text.contains(request)).expect("the request");
            assert!(sent.contains("\"boot\":\"now\""), "{sent}");
        }
        let contract = seen.iter().find(|text| text.contains("\"contract\"")).expect("the contract");
        assert!(!contract.contains("\"boot\""), "{contract}");
    }

    #[test]
    fn flow_run_sends_its_view_with_the_run() {
        let (answer, seen) = in_a_view_on(FLOW_RUN, control::contract::describe("boot", &[]));

        assert_eq!(answer.status, 0, "{answer:?}");
        let run = seen.iter().find(|text| text.contains("flow-run")).expect("the run");
        assert!(run.contains("\"view\":7"), "{run}");
        let others: Vec<_> = seen.iter().filter(|text| !text.contains("flow-run")).collect();
        assert!(others.iter().all(|text| !text.contains("\"view\"")), "{others:?}");
    }

    #[rstest]
    #[case::plugin_run_without_views(PLUGIN_RUN, "plugin-run", "views", "has no views")]
    #[case::plugin_run_that_takes_no_view_there(
        PLUGIN_RUN,
        "plugin-run",
        "plugin-run-view",
        "cannot run a plugin's action in a view"
    )]
    #[case::flow_run_without_views(FLOW_RUN, "flow-run", "views", "has no views")]
    #[case::flow_run_that_takes_no_view_there(FLOW_RUN, "flow-run", "flow-run-view", "cannot start a flow in a view")]
    fn a_run_in_a_view_is_refused_up_front_by_a_server_that_would_drop_or_refuse_the_view(
        #[case] line: &str,
        #[case] request: &str,
        #[case] missing: &str,
        #[case] says: &str,
    ) {
        let (answer, seen) = in_a_view_on(line, lacking(missing));

        assert_eq!(answer.status, 1, "{answer:?}");
        let error = answer.error.expect("an error");
        assert!(error.contains(says) && error.contains("update cornercase"), "{error}");
        assert!(!seen.iter().any(|text| text.contains(request)), "{seen:?}");
    }

    #[rstest]
    #[case::close(r#"{"args": ["close", "--pane", "4", "--boot", "old"]}"#)]
    #[case::rename(r#"{"args": ["rename", "--tab", "4", "x", "--boot", "old"]}"#)]
    #[case::mark(r#"{"args": ["mark", "--tab", "4", "--reviewed", "--boot", "old"]}"#)]
    #[case::move_pane(r#"{"args": ["move-pane", "--pane", "4", "--target", "5", "--place", "swap", "--boot", "old"]}"#)]
    #[case::account_restart(
        r#"{"args": ["account", "restart", "claude", "work", "--pane", "4", "--json", "--boot", "old"]}"#
    )]
    #[case::context(r#"{"args": ["context", "--tab", "4", "--cached", "--boot", "old"]}"#)]
    #[case::collapse(r#"{"args": ["collapse", "--group", "4", "--boot", "old"]}"#)]
    #[case::expand(r#"{"args": ["expand", "--pane", "4", "--boot", "old"]}"#)]
    #[case::send(r#"{"args": ["send", "--tab", "4", "x", "--json", "--boot", "old"]}"#)]
    #[case::input(
        r#"{"args": ["input", "--popup", "--pane", "4", "{\"key\": {\"code\": \"enter\"}}", "--boot", "old"]}"#
    )]
    #[case::paste_file(
        r#"{"args": ["paste-file", "--pane", "4", "--name", "x.png", "--json", "--boot", "old"], "stdin": "a picture"}"#
    )]
    fn a_boot_for_a_server_that_would_ignore_it_never_sends_the_request(#[case] line: &str) {
        never_sent_without_boot(line);
    }

    #[rstest]
    #[case::keys(r#"{"args": ["keys", "--pane", "4", "enter", "--boot", "old"]}"#)]
    #[case::held(r#"{"args": ["held", "--pane", "4", "--send-now", "--boot", "old"]}"#)]
    #[case::todo_send(r#"{"args": ["todo", "send", "3", "--pane", "4", "--boot", "old"]}"#)]
    #[case::scroll(r#"{"args": ["scroll", "--pane", "4", "--up", "3", "--boot", "old"]}"#)]
    #[case::selection(r#"{"args": ["selection", "--pane", "4", "--clear", "--boot", "old"]}"#)]
    #[case::split(r#"{"args": ["split", "--pane", "4", "--boot", "old"]}"#)]
    #[case::focus(r#"{"args": ["focus", "--pane", "4", "--boot", "old"]}"#)]
    #[case::focus_and_reveal(r#"{"args": ["focus", "--pane", "4", "--reveal", "x", "--boot", "old"]}"#)]
    #[case::ratio(r#"{"args": ["ratio", "--tab", "4", "0.3", "--boot", "old"]}"#)]
    #[case::resize(r#"{"args": ["resize", "--surface", "4", "--cols", "80", "--rows", "24", "--boot", "old"]}"#)]
    #[case::redraw(r#"{"args": ["redraw", "--surface", "4", "--boot", "old"]}"#)]
    #[case::view_set(r#"{"args": ["view-set", "--view", "7", "--cols", "80", "--rows", "24", "--boot", "old"]}"#)]
    #[case::move_a_tab(r#"{"args": ["move", "--tab", "4", "--last", "--boot", "old"]}"#)]
    #[case::group_style(r#"{"args": ["group", "style", "4", "--icon", "★", "--boot", "old"]}"#)]
    #[case::group_delete(r#"{"args": ["group", "delete", "4", "--boot", "old"]}"#)]
    #[case::new_tab(r#"{"args": ["new-tab", "--workspace", "4", "--boot", "old"]}"#)]
    #[case::new_tab_by_branch(r#"{"args": ["new-tab", "--worktree", "fix/login", "--boot", "old"]}"#)]
    #[case::setup(r#"{"args": ["setup", "--workspace", "4", "--boot", "old"]}"#)]
    #[case::start(r#"{"args": ["start", "claude", "--workspace", "4", "--boot", "old"]}"#)]
    #[case::new_workspace(r#"{"args": ["new-workspace", "x", "--project", "4", "--boot", "old"]}"#)]
    #[case::open(r#"{"args": ["open", "/x", "--into", "2", "--boot", "old"]}"#)]
    #[case::agents_close(r#"{"args": ["agents", "close", "--workspace", "4", "--check", "--boot", "old"]}"#)]
    #[case::agents_reopen(r#"{"args": ["agents", "reopen", "--workspace", "4", "--boot", "old"]}"#)]
    #[case::agents_forget(r#"{"args": ["agents", "forget", "--workspace", "4", "--yes", "--boot", "old"]}"#)]
    #[case::changes_viewed(r#"{"args": ["changes", "viewed", "a.rs", "--workspace", "4", "--boot", "old"]}"#)]
    #[case::changes_stage(r#"{"args": ["changes", "stage", "a.rs", "--workspace", "4", "--boot", "old"]}"#)]
    #[case::changes_unstage(r#"{"args": ["changes", "unstage", "a.rs", "--workspace", "4", "--boot", "old"]}"#)]
    #[case::changes_discard(
        r#"{"args": ["changes", "discard", "a.rs", "--workspace", "4", "--yes", "--boot", "old"]}"#
    )]
    #[case::changes_base(r#"{"args": ["changes", "base", "main", "--workspace", "4", "--boot", "old"]}"#)]
    #[case::queue_add(
        r#"{"args": ["queue", "add", "--workspace", "4", "--prompt", "x", "--at", "10:00", "--boot", "old"]}"#
    )]
    #[case::queue_edit(r#"{"args": ["queue", "edit", "3", "--prompt", "y", "--boot", "old"]}"#)]
    #[case::account_add(r#"{"args": ["account", "add", "claude", "work", "--boot", "old"]}"#)]
    #[case::account_login(r#"{"args": ["account", "login", "claude", "work", "--workspace", "4", "--boot", "old"]}"#)]
    #[case::free_builds(r#"{"args": ["worktrees", "free-builds", "--workspace", "4", "--boot", "old"]}"#)]
    #[case::plugin_run(r#"{"args": ["plugin", "run", "p", "a", "--pane", "4", "--boot", "old"]}"#)]
    #[case::config_project(r#"{"args": ["config", "project", "--project", "4", "--setup", "make", "--boot", "old"]}"#)]
    #[case::plugin_run_with_a_revision(
        r#"{"args": ["plugin", "run", "p", "a", "--pane", "4", "--revision", "1", "--boot", "old"]}"#
    )]
    fn every_command_that_acts_is_refused_by_a_later_run_and_by_a_server_without_boot(#[case] line: &str) {
        refused_by_a_later_run(line);
        never_sent_without_boot(line);
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
    #[case::config_pair(r#"{"args": ["config", "pair", "browsers"]}"#)]
    fn a_stream_is_refused_before_it_sends_anything(#[case] line: &str) {
        let (mut answers, seen) = lines_on(fake_server, &[line]);

        let answer = answers.remove(0);
        assert_eq!(answer.status, 2, "{answer:?}");
        let error = answer.error.expect("an error");
        assert!(error.contains("connection of its own"), "{error}");
        assert!(seen.iter().all(|text| text.contains("\"contract\"")), "{seen:?}");
    }

    const PLACED: &str = r#"{"args": ["start", "claude", "--placed", "--json"]}"#;
    const PLAIN: &str = r#"{"args": ["start", "claude", "--json"]}"#;

    fn start_on(features: control::contract::Descriptor, line: &str) -> (Answer, Vec<String>) {
        let (mut answers, seen) = lines_on(|theirs| serving(theirs, features, NO_REQUEST_NAMES_THIS), &[line]);
        (answers.remove(0), seen)
    }

    #[test]
    fn a_placed_start_checks_the_server_then_sends_the_field() {
        let (answer, seen) = start_on(control::contract::describe("boot", &[]), PLACED);

        assert_eq!(answer.status, 0, "{answer:?}");
        let [asked, start] = &seen[..] else { panic!("not a check and a start: {seen:?}") };
        assert!(asked.contains("\"contract\"") && start.contains("\"placed\":true"), "{seen:?}");
    }

    #[test]
    fn a_placed_start_is_refused_before_it_is_sent_when_the_server_would_drop_the_field() {
        let (answer, seen) = start_on(lacking("machine-start"), PLACED);

        assert_eq!(answer.status, 1, "{answer:?}");
        let error = answer.error.expect("an error");
        assert!(error.contains("cannot answer a start as soon as its tab is open"), "{error}");
        let [asked] = &seen[..] else { panic!("not only the check: {seen:?}") };
        assert!(asked.contains("\"contract\""), "{seen:?}");
    }

    const QUEUE_DEFAULT: &str =
        r#"{"args": ["queue", "add", "--at", "in 2h", "--prompt", "x", "--mode", "default", "--json"]}"#;
    const QUEUE_PLAN: &str =
        r#"{"args": ["queue", "add", "--at", "in 2h", "--prompt", "x", "--mode", "plan", "--json"]}"#;

    #[test]
    fn a_queued_default_mode_checks_the_server_then_sends_it() {
        let (answer, seen) = start_on(control::contract::describe("boot", &[]), QUEUE_DEFAULT);

        assert_eq!(answer.status, 0, "{answer:?}");
        let [asked, queued] = &seen[..] else { panic!("not a check and a request: {seen:?}") };
        assert!(asked.contains("\"contract\"") && queued.contains("\"mode\":\"default\""), "{seen:?}");
    }

    #[rstest]
    #[case::refused(QUEUE_DEFAULT, 1)]
    #[case::other_mode(QUEUE_PLAN, 0)]
    fn a_server_without_mode_default_is_asked_only_for_the_default_mode(#[case] line: &str, #[case] status: u8) {
        let (answer, seen) = start_on(lacking("mode-default"), line);

        assert_eq!(answer.status, status, "{answer:?}");
        if status == 0 {
            let [queued] = &seen[..] else { panic!("not only the request: {seen:?}") };
            assert!(queued.contains("\"queue\""), "{seen:?}");
            return;
        }
        let error = answer.error.expect("an error");
        assert!(error.contains("cannot queue an agent with no mode arguments") && error.contains("update cornercase"));
        let [asked] = &seen[..] else { panic!("not only the check: {seen:?}") };
        assert!(asked.contains("\"contract\""), "{seen:?}");
    }

    #[test]
    fn a_plain_start_needs_no_check_and_sends_no_field() {
        let (answer, seen) = start_on(lacking("machine-start"), PLAIN);

        assert_eq!(answer.status, 0, "{answer:?}");
        let [start] = &seen[..] else { panic!("not only the start: {seen:?}") };
        assert!(start.contains("\"start\"") && !start.contains("placed"), "{seen:?}");
    }
}
