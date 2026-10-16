use super::*;
use crate::control::contract::{Code, Structured};
use crate::control::start_server::{SizeFrom, Started};

fn ask(app: &mut App, text: &str) -> Vec<(u64, String)> {
    app.request(CLIENT, text, None, Instant::now());
    app.take_answers()
}

fn structured(text: &str) -> Structured {
    serde_json::from_str(text).expect("a structured answer")
}

#[test]
fn a_valid_size_waits_for_the_server_and_is_taken_once() {
    let (mut app, _rx) = app();

    let answers = ask(&mut app, r#"{"command":"start-server","args":{"cols":120}}"#);

    assert_eq!(answers, Vec::<(u64, String)>::new());
    assert_eq!(app.take_start(), Some((CLIENT, (120, 48))));
    assert_eq!(app.take_start(), None);
}

#[test]
fn the_server_answers_in_the_shape_the_client_asked_for() {
    let (mut app, _rx) = app();
    ask(&mut app, r#"{"contract":1,"command":"start-server","args":{}}"#);
    let (client, (cols, rows)) = app.take_start().expect("a start");
    let started = Started { cols, rows, size: SizeFrom::StartServer, ..Started::default() };

    app.answer_start(client, &started);

    let answers = app.take_answers();
    assert_eq!(answers.len(), 1, "{answers:?}");
    let Structured::Ok(value) = structured(&answers[0].1) else { panic!("refused: {answers:?}") };
    assert_eq!(
        (value["cols"].as_u64(), value["rows"].as_u64(), value["size"].as_str()),
        (Some(160), Some(48), Some("start-server"))
    );
}

#[test]
fn a_size_out_of_range_is_malformed_and_starts_nothing() {
    let (mut app, _rx) = app();

    let answers = ask(&mut app, r#"{"contract":1,"command":"start-server","args":{"cols":89}}"#);

    let Structured::Error(refusal) = structured(&answers[0].1) else { panic!("not refused: {answers:?}") };
    assert_eq!((refusal.code, refusal.message.as_str()), (Code::Malformed, "cols must be between 90 and 1000, not 89"));
    assert_eq!(app.take_start(), None);
}

#[test]
fn a_size_out_of_range_names_the_range_without_opting_in() {
    let (mut app, _rx) = app();

    let answers = ask(&mut app, r#"{"command":"start-server","args":{"rows":501}}"#);

    assert_eq!(answers, [(CLIENT, r#"{"error":"rows must be between 10 and 500, not 501"}"#.to_string())]);
}

#[test]
fn an_unknown_argument_is_refused_when_structured() {
    let (mut app, _rx) = app();

    let answers = ask(&mut app, r#"{"contract":1,"command":"start-server","args":{"bogus":1}}"#);

    let Structured::Error(refusal) = structured(&answers[0].1) else { panic!("not refused: {answers:?}") };
    assert_eq!(refusal.code, Code::UnknownField);
    assert!(refusal.message.contains("`args.bogus`"), "{}", refusal.message);
    assert_eq!(app.take_start(), None);
}
