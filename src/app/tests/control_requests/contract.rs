use super::*;
use crate::control::contract::{
    CONTRACT, Code, Descriptor, EVENTS_VERSION, Refusal, SURFACE_VERSION, Stream, Structured,
};

fn raw(app: &mut App, text: &str) -> String {
    app.request(CLIENT, text, Some(AREA), Instant::now());
    let mut answers = app.take_answers();
    assert_eq!(answers.len(), 1, "an answer at once: {answers:?}");
    answers.remove(0).1
}

fn refused(app: &mut App, text: &str) -> (Code, String) {
    match serde_json::from_str(&raw(app, text)).expect("a structured answer") {
        Structured::Error(refusal) => (refusal.code, refusal.message),
        Structured::Ok(value) => panic!("the request did not fail: {value}"),
    }
}

fn legacy(message: &str) -> String {
    serde_json::to_string(&Response::Error(message.to_string())).expect("json")
}

fn notified(app: &mut App) -> Vec<String> {
    app.take_notifications().into_iter().map(|n| n.text).collect()
}

mod without_opting_in {
    use super::*;

    #[test]
    fn an_unknown_command_answers_the_text_it_always_did() {
        let (mut app, _rx) = app();

        let answer = raw(&mut app, r#"{"command":"bogus","args":{}}"#);

        assert_eq!(answer, legacy(&wire::unknown_command("bogus")));
    }

    #[test]
    fn a_request_that_is_not_json_answers_a_plain_error() {
        let (mut app, _rx) = app();

        let answer = raw(&mut app, "{");

        let Response::Error(message) = serde_json::from_str(&answer).expect("a legacy answer") else {
            panic!("the request did not fail: {answer}")
        };
        assert!(message.starts_with("the request is not JSON: "), "{message}");
    }

    #[test]
    fn an_unknown_field_is_ignored() {
        let (mut app, _rx) = app();

        let answer = raw(&mut app, r#"{"command":"notify","args":{"text":"hi","bogus":1}}"#);

        assert_eq!((answer.as_str(), notified(&mut app)), (r#"{"ok":{}}"#, vec!["hi".to_string()]));
    }
}

mod opting_in {
    use super::*;

    #[test]
    fn an_answer_keeps_its_ok_shape() {
        let (mut app, _rx) = app();

        let answer = raw(&mut app, r#"{"contract":1,"command":"notify","args":{"text":"hi"}}"#);

        assert_eq!(answer, r#"{"ok":{}}"#);
    }

    #[test]
    fn an_unknown_command_is_unsupported() {
        let (mut app, _rx) = app();

        let refusal = refused(&mut app, r#"{"contract":1,"command":"bogus","args":{}}"#);

        assert_eq!(refusal, (Code::Unsupported, wire::unknown_command("bogus")));
    }

    #[test]
    fn arguments_of_the_wrong_type_are_malformed() {
        let (mut app, _rx) = app();

        let (code, message) = refused(&mut app, r#"{"contract":1,"command":"notify","args":{"text":5}}"#);

        assert_eq!(code, Code::Malformed);
        assert!(message.contains("cannot read this `notify` request"), "{message}");
    }

    #[rstest::rstest]
    #[case::in_the_arguments(r#"{"contract":1,"command":"notify","args":{"text":"hi","bogus":1}}"#, "args.bogus")]
    #[case::in_the_envelope(r#"{"contract":1,"extra":true,"command":"notify","args":{"text":"hi"}}"#, "extra")]
    fn an_unknown_field_is_refused_and_does_nothing(#[case] text: &str, #[case] field: &str) {
        let (mut app, _rx) = app();

        let (code, message) = refused(&mut app, text);

        assert_eq!(code, Code::UnknownField);
        assert!(message.contains(&format!("`{field}`")), "{message}");
        assert_eq!(notified(&mut app), Vec::<String>::new());
    }

    #[test]
    fn a_null_contract_or_boot_is_not_an_unknown_field() {
        let (mut app, _rx) = app();
        let token = wire::server_token();

        let answers = [
            raw(&mut app, r#"{"contract":1,"boot":null,"command":"notify","args":{"text":"one"}}"#),
            raw(
                &mut app,
                &format!(r#"{{"contract":null,"boot":"{token}","command":"notify","args":{{"text":"two"}}}}"#),
            ),
        ];

        assert_eq!(answers, [r#"{"ok":{}}"#, r#"{"ok":{}}"#]);
        assert_eq!(notified(&mut app), ["one", "two"]);
    }

    #[test]
    fn a_newer_contract_is_unsupported_and_does_nothing() {
        let (mut app, _rx) = app();

        let refusal =
            refused(&mut app, &format!(r#"{{"contract":{},"command":"notify","args":{{"text":"hi"}}}}"#, CONTRACT + 1));

        assert_eq!(
            refusal,
            (
                Code::Unsupported,
                format!(
                    "this server speaks contract {CONTRACT}, not {}; update cornercase and restart the server",
                    CONTRACT + 1
                )
            )
        );
        assert_eq!(notified(&mut app), Vec::<String>::new());
    }

    #[test]
    fn a_refused_handler_fails_with_its_text() {
        let (mut app, _rx) = app();

        let refusal = refused(&mut app, r#"{"contract":1,"command":"notify","args":{"text":"  "}}"#);

        assert_eq!(refusal, (Code::Failed, "the notification needs a text".to_string()));
    }

    #[test]
    fn the_next_request_of_the_client_chooses_its_own_mode() {
        let (mut app, _rx) = app();
        refused(&mut app, r#"{"contract":1,"command":"bogus","args":{}}"#);

        let answer = raw(&mut app, r#"{"command":"bogus","args":{}}"#);

        assert_eq!(answer, legacy(&wire::unknown_command("bogus")));
    }
}

fn sent(
    app: &mut App,
    caller: Option<u64>,
    server: Option<&str>,
    contract: Option<u32>,
    boot: Option<&str>,
    command: Command,
) -> String {
    let (server, boot) = (server.map(str::to_string), boot.map(str::to_string));
    let text =
        serde_json::to_string(&wire::Request { caller, server, contract, boot, view: None, command }).expect("json");
    raw(app, &text)
}

fn close_pane(id: u64) -> Command {
    Command::Close(wire::Close { item: Item::Pane(id), remove_worktree: false, force: false })
}

fn alive(app: &mut App, rx: &Receiver<AppEvent>, id: u64) {
    assert!(app.projects.pane_by_mut(id).expect("the pane").write(b"echo still-\"\"here\r"), "the shell takes it");
    pump_until(app, rx, "the shell still answers", |a| pane_screen(a, id).contains("still-here"));
}

mod boot {
    use super::*;

    const OLD: &str = "1-1";

    #[test]
    fn a_close_from_an_earlier_boot_is_refused_and_closes_nothing() {
        let (mut app, rx) = app();
        let id = first(&app);

        let (code, message) = match serde_json::from_str(&sent(&mut app, None, None, None, Some(OLD), close_pane(id)))
            .expect("a structured answer")
        {
            Structured::Error(refusal) => (refusal.code, refusal.message),
            Structured::Ok(value) => panic!("the request did not fail: {value}"),
        };

        assert_eq!(code, Code::StaleBoot);
        assert!(message.contains(OLD) && message.contains(wire::server_token()), "{message}");
        alive(&mut app, &rx, id);
    }

    #[test]
    fn a_send_from_an_earlier_boot_is_refused_and_types_nothing() {
        let (mut app, _rx) = app();
        let id = first(&app);
        let before = pane(&app, id).input_at;

        let answer = sent(&mut app, None, None, Some(1), Some(OLD), send_text(id, "hello", true, false));

        let Structured::Error(refusal) = serde_json::from_str(&answer).expect("a structured answer") else {
            panic!("the request did not fail: {answer}")
        };
        assert_eq!((refusal.code, pane(&app, id).input_at), (Code::StaleBoot, before));
    }

    #[test]
    fn a_request_of_this_boot_acts() {
        let (mut app, rx) = app();
        let id = first(&app);

        let answer = sent(&mut app, None, None, None, Some(wire::server_token()), close_pane(id));

        assert_eq!(answer, r#"{"ok":{}}"#);
        pump_until(&mut app, &rx, "the pane closes", |a| a.projects.pane_by(id).is_none());
    }

    #[test]
    fn a_legacy_send_naming_another_server_still_reaches_its_pane() {
        let (mut app, rx) = app();
        let id = first(&app);
        type_line(&mut app, "echo cat-\"\"starts; cat -v");
        pump_until(&mut app, &rx, "cat runs", |a| pane_screen(a, id).contains("cat-starts"));

        let answer = sent(&mut app, Some(id), Some(OLD), None, None, send_text(id, "hello", false, false));

        assert_eq!(answer, format!(r#"{{"ok":{{"pane":{id}}}}}"#));
        pump_until(&mut app, &rx, "cat echoes it", |a| pane_screen(a, id).contains("hello"));
    }
}

mod not_found {
    use super::*;

    #[rstest::rstest]
    #[case::a_pane(close_pane(999), "pane")]
    #[case::a_tab(Command::Focus(wire::Focus { item: Item::Tab(999) }), "tab")]
    #[case::a_workspace(Command::Close(wire::Close { item: Item::Workspace(999), remove_worktree: false, force: false }), "workspace")]
    #[case::a_sent_pane(send_text(999, "hello", true, false), "pane")]
    #[case::a_followed_pane(Command::Events(wire::Events { panes: vec![999] }), "pane")]
    fn an_id_that_names_nothing_is_not_found(#[case] command: Command, #[case] kind: &str) {
        let (mut app, _rx) = app();
        let message = format!("there is no {kind} 999; `cornercase status` lists them");

        let structured = sent(&mut app, None, None, Some(1), None, command.clone());
        let legacy_answer = sent(&mut app, None, None, None, None, command);

        assert_eq!(
            serde_json::from_str::<Structured>(&structured).expect("a structured answer"),
            Structured::Error(Refusal::new(Code::NotFound, message.clone()))
        );
        assert_eq!(legacy_answer, legacy(&message));
    }
}

mod discovery {
    use super::*;

    fn ok<T: serde::de::DeserializeOwned>(answer: &str) -> T {
        match serde_json::from_str(answer).expect("an answer") {
            Response::Ok(value) => serde_json::from_value(value).expect("its shape"),
            Response::Error(message) => panic!("the request failed: {message}"),
        }
    }

    #[test]
    fn contract_answers_without_a_window() {
        let (mut app, _rx) = app();

        app.request(CLIENT, r#"{"command":"contract","args":{}}"#, None, Instant::now());
        let answers = app.take_answers();

        let [(client, answer)] = &answers[..] else { panic!("one answer at once: {answers:?}") };
        let keys: Vec<String> = match serde_json::from_str::<Value>(answer).expect("json")["ok"].clone() {
            Value::Object(map) => map.keys().cloned().collect(),
            other => panic!("not an object: {other}"),
        };
        let descriptor: Descriptor = ok(answer);
        assert_eq!(*client, CLIENT);
        assert_eq!(keys, ["boot", "contract", "features", "operations", "streams", "version"]);
        assert_eq!(
            (descriptor.contract, descriptor.boot.as_str(), descriptor.version.as_str()),
            (CONTRACT, wire::server_token(), version::CURRENT)
        );
        assert!(descriptor.streams.contains(&Stream { name: "events".into(), version: EVENTS_VERSION }));
        assert!(descriptor.streams.contains(&Stream { name: "surface".into(), version: SURFACE_VERSION }));
        assert!(descriptor.operations.iter().any(|op| op.name == "contract" && op.available));
    }

    #[test]
    fn status_names_the_boot() {
        let (mut app, _rx) = app();

        let report: Report = ok(&raw(&mut app, r#"{"command":"status","args":{}}"#));

        assert_eq!(report.boot, wire::server_token());
    }

    #[test]
    fn events_opted_in_start_with_an_ok() {
        let (mut app, _rx) = app();

        let answer = raw(&mut app, r#"{"command":"events","args":{},"contract":1}"#);

        assert_eq!(answer, r#"{"ok":{}}"#);
    }
}
