use std::cell::RefCell;
use std::sync::mpsc::{self, Sender};

use rstest::rstest;

use super::*;
use crate::control::terminal::Key;

fn text(pane: Option<u64>, what: &str) -> control::Input {
    control::Input { pane, events: vec![InputEvent::Text(what.into())], ..control::Input::default() }
}

fn sent(input: &control::Input) -> Value {
    json!({"pane": input.pane, "bytes": 1, "skipped": []})
}

struct Typing {
    asked: Vec<control::Input>,
    out: String,
    err: String,
    result: Result<()>,
}

fn typing(
    lines: &[&str],
    json: bool,
    answer: impl FnMut(&control::Input, Option<&Sender<io::Result<Vec<u8>>>>) -> Result<Value>,
) -> Typing {
    typing_with(
        lines,
        &control::Input { pane: Some(1), ..control::Input::default() },
        Printer::new(json, Vec::new(), Vec::new()),
        answer,
    )
}

fn typing_with(
    lines: &[&str],
    template: &control::Input,
    mut printer: Printer<Vec<u8>, Vec<u8>>,
    answer: impl FnMut(&control::Input, Option<&Sender<io::Result<Vec<u8>>>>) -> Result<Value>,
) -> Typing {
    let (tx, rx) = mpsc::channel();
    for line in lines {
        tx.send(Ok(line.as_bytes().to_vec())).expect("queue a line");
    }
    let (asked, answer, tx) = (RefCell::new(Vec::new()), RefCell::new(answer), RefCell::new(Some(tx)));
    let result = typed(
        &rx,
        template,
        |input| {
            let value = (answer.borrow_mut())(&input, tx.borrow().as_ref());
            tx.borrow_mut().take();
            asked.borrow_mut().push(input);
            value
        },
        &mut printer,
    );
    let (out, err) =
        (String::from_utf8_lossy(&printer.out).into_owned(), String::from_utf8_lossy(&printer.err).into_owned());
    Typing { asked: asked.into_inner(), out, err, result }
}

fn objects(out: &str) -> Vec<Value> {
    out.lines().map(|line| serde_json::from_str(line).expect("one JSON object per line")).collect()
}

mod a_line {
    use super::*;

    #[rstest]
    #[case::none(r#"{"text":"x"}"#, None)]
    #[case::before(r#"{"pane":14,"text":"x"}"#, Some(14))]
    #[case::after(r#"{"text":"x","pane":14}"#, Some(14))]
    #[case::null(r#"{"pane":null,"text":"x"}"#, None)]
    fn takes_the_pane_beside_its_event(#[case] text: &str, #[case] pane: Option<u64>) {
        let Some(Line::Event(found, event)) = line(text.as_bytes()) else { panic!("not an event") };

        assert_eq!((found, event), (pane, InputEvent::Text("x".into())));
    }

    #[rstest]
    #[case::not_json(b"{".as_slice())]
    #[case::two_events(br#"{"text":"x","key":{"code":"a"}}"#)]
    #[case::an_unknown_key(br#"{"pane":1,"bogus":1,"text":"x"}"#)]
    #[case::a_pane_in_quotes(br#"{"pane":"14","text":"x"}"#)]
    #[case::a_pane_alone(br#"{"pane":14}"#)]
    #[case::an_array(br#"[{"text":"x"}]"#)]
    #[case::not_utf8(b"{\"text\":\"\xff\"}")]
    fn that_is_not_one_event_is_wrong(#[case] bytes: &[u8]) {
        let Some(Line::Wrong(message)) = line(bytes) else { panic!("not wrong") };

        assert!(message.starts_with("not an input event"), "{message}");
    }

    #[rstest]
    #[case::empty("")]
    #[case::spaces("  \r")]
    fn that_is_blank_is_no_event(#[case] text: &str) {
        assert!(line(text.as_bytes()).is_none());
    }
}

mod the_lines_waiting {
    use super::*;

    fn event(pane: Option<u64>, what: &str) -> Line {
        Line::Event(pane, InputEvent::Text(what.into()))
    }

    fn sends(parts: &[Part]) -> Vec<(Option<u64>, usize)> {
        parts
            .iter()
            .map(|part| match part {
                Part::Send(input) => (input.pane, input.events.len()),
                Part::Wrong(_) => (None, 0),
            })
            .collect()
    }

    #[test]
    fn go_in_one_request_while_they_name_one_pane() {
        let template = control::Input { pane: Some(1), ..control::Input::default() };

        let parts = parts([event(None, "a"), event(None, "b"), event(Some(1), "c")], &template);

        assert_eq!(sends(&parts), [(Some(1), 3)]);
        let Part::Send(input) = &parts[0] else { panic!("not a request") };
        assert_eq!(
            input.events,
            [InputEvent::Text("a".into()), InputEvent::Text("b".into()), InputEvent::Text("c".into())]
        );
    }

    #[test]
    fn start_a_request_at_each_change_of_pane_in_their_order() {
        let parts = parts([event(Some(1), "a"), event(Some(2), "b"), event(Some(1), "c")], &control::Input::default());

        assert_eq!(sends(&parts), [(Some(1), 1), (Some(2), 1), (Some(1), 1)]);
    }

    #[test]
    fn answer_a_wrong_line_on_its_own_between_the_requests_around_it() {
        let wrong = Line::Wrong("no".into());

        let parts = parts([event(Some(1), "a"), wrong, event(Some(1), "b")], &control::Input::default());

        assert_eq!(sends(&parts), [(Some(1), 1), (None, 0), (Some(1), 1)]);
    }

    #[test]
    fn take_the_default_pane_and_the_popup_for_lines_naming_none() {
        let template = control::Input { pane: Some(40), popup: true, ..control::Input::default() };

        let parts = parts([event(None, "a"), event(Some(41), "b")], &template);

        let [Part::Send(first), Part::Send(second)] = parts.as_slice() else { panic!("two requests") };
        assert_eq!(((first.pane, first.popup), (second.pane, second.popup)), ((Some(40), true), (Some(41), true)));
    }
}

mod typing {
    use super::*;

    #[expect(clippy::unnecessary_wraps, reason = "it stands where a fallible answer is taken")]
    fn quiet(input: &control::Input, _: Option<&Sender<io::Result<Vec<u8>>>>) -> Result<Value> {
        Ok(sent(input))
    }

    #[test]
    fn answers_each_request_with_one_line_saying_how_many_lines_it_answers() {
        let done = typing(&[r#"{"text":"a"}"#, "", r#"{"text":"b"}"#, r#"{"text":"c"}"#], true, quiet);

        assert_eq!(done.asked.len(), 1);
        assert_eq!(objects(&done.out), [json!({"pane": 1, "bytes": 1, "skipped": [], "events": 3})]);
        assert!(done.result.is_ok() && done.err.is_empty());
    }

    #[test]
    fn sends_a_line_that_arrives_during_a_round_trip_with_the_next_request() {
        let mut first = true;
        let done = typing(&[r#"{"pane":1,"text":"a"}"#], true, |input, tx| {
            if std::mem::take(&mut first) {
                tx.expect("a sender")
                    .send(Ok(br#"{"pane":1,"text":"b"}"#.to_vec()))
                    .expect("a line during the round trip");
            }
            Ok(sent(input))
        });

        assert_eq!(done.asked, [text(Some(1), "a"), text(Some(1), "b")]);
        let events: Vec<_> = objects(&done.out).iter().map(|answer| answer["events"].clone()).collect();
        assert_eq!(events, [json!(1), json!(1)]);
    }

    #[test]
    fn counts_skipped_events_from_the_first_line_read() {
        let lines = [r#"{"pane":1,"text":"a"}"#, r#"{"pane":2,"text":"b"}"#];
        let answer = |input: &control::Input, _: Option<&Sender<io::Result<Vec<u8>>>>| {
            let skipped = if input.pane == Some(2) { json!([{"event": 0, "why": "mouse"}]) } else { json!([]) };
            Ok(json!({"pane": input.pane, "bytes": 0, "skipped": skipped}))
        };

        let json = typing(&lines, true, answer);
        let text = typing(&lines, false, answer);

        assert_eq!(objects(&json.out)[1]["skipped"], json!([{"event": 1, "why": "mouse"}]));
        assert_eq!(text.out, "");
        assert_eq!(text.err, "warning: event 2 was not sent: the program does not read the mouse that way\n");
    }

    #[test]
    fn goes_on_after_a_refusal_and_ends_with_a_partial_answer() {
        let done = typing(&[r#"{"pane":9,"text":"a"}"#, r#"{"pane":1,"text":"b"}"#], true, |input, _| {
            if input.pane == Some(9) { Err(Error::Control("no pane 9".into())) } else { Ok(sent(input)) }
        });

        assert_eq!(done.asked.len(), 2);
        let answers = objects(&done.out);
        assert_eq!(answers[0], json!({"error": "no pane 9", "events": 1, "pane": 9}));
        assert_eq!(answers[1]["pane"], json!(1));
        assert!(matches!(done.result, Err(Error::Partial)), "{:?}", done.result);
    }

    #[test]
    fn says_which_lines_a_refusal_lost_without_json() {
        let done = typing(&[r#"{"text":"a"}"#, r#"{"text":"b"}"#], false, |_, _| Err(Error::Control("no pane".into())));

        assert_eq!(done.err, "error: events 1 to 2 were not sent: no pane\n");
        assert_eq!(done.out, "");
    }

    #[test]
    fn answers_a_wrong_line_without_asking() {
        let done = typing(&[r#"{"pane":1,"text":"a"}"#, "{", r#"{"pane":1,"text":"b"}"#], true, quiet);

        assert_eq!(done.asked.len(), 2);
        let answers = objects(&done.out);
        assert_eq!(answers.len(), 3);
        assert_eq!(answers[1]["events"], json!(1));
        assert!(answers[1]["error"].as_str().is_some_and(|e| e.starts_with("not an input event")));
        assert!(answers[1].get("pane").is_none());
    }

    #[test]
    fn ends_at_once_when_the_server_is_gone() {
        let done =
            typing(&[r#"{"pane":1,"text":"a"}"#, r#"{"pane":2,"text":"b"}"#], true, |_, _| Err(Error::ServerGone));

        assert_eq!(done.asked.len(), 1);
        assert!(matches!(done.result, Err(Error::ServerGone)), "{:?}", done.result);
    }

    #[test]
    fn ends_quietly_once_nothing_reads_its_answers() {
        let (tx, rx) = mpsc::channel();
        for line in [r#"{"pane":1,"text":"a"}"#, r#"{"pane":2,"text":"b"}"#] {
            tx.send(Ok(line.as_bytes().to_vec())).expect("queue a line");
        }
        let mut asks = 0;
        let mut printer = Printer::new(true, crate::test_util::BrokenPipe, Vec::new());

        let result = typed(
            &rx,
            &control::Input::default(),
            |input| {
                asks += 1;
                Ok(sent(&input))
            },
            &mut printer,
        );

        assert!(result.is_ok(), "{result:?}");
        assert_eq!(asks, 1);
    }

    #[test]
    fn answers_a_line_naming_no_pane_when_there_is_no_default_pane() {
        let lines = [r#"{"text":"a"}"#, r#"{"pane":1,"text":"b"}"#];
        let done = typing_with(&lines, &control::Input::default(), Printer::new(true, Vec::new(), Vec::new()), quiet);

        assert_eq!(done.asked, [text(Some(1), "b")]);
        let answers = objects(&done.out);
        assert!(answers[0]["error"].as_str().is_some_and(|e| e.contains("names no pane")), "{answers:?}");
        assert!(matches!(done.result, Err(Error::Partial)), "{:?}", done.result);
    }

    #[test]
    fn sends_the_lines_read_before_a_read_error_and_then_ends_with_it() {
        let (tx, rx) = mpsc::channel();
        tx.send(Ok(br#"{"pane":1,"text":"a"}"#.to_vec())).expect("queue a line");
        tx.send(Err(io::Error::other("the input broke"))).expect("queue an error");
        let mut asked = Vec::new();
        let mut printer = Printer::new(true, Vec::new(), Vec::new());

        let result = typed(
            &rx,
            &control::Input::default(),
            |input| {
                let answer = sent(&input);
                asked.push(input);
                Ok(answer)
            },
            &mut printer,
        );

        assert_eq!(asked, [text(Some(1), "a")]);
        assert!(matches!(result, Err(Error::Io(_))), "{result:?}");
    }

    #[test]
    fn ends_with_its_input_after_the_last_answer() {
        let done = typing(&[r#"{"pane":1,"key":{"code":"enter"}}"#], false, quiet);

        assert!(done.result.is_ok());
        assert_eq!(done.asked[0].events, [InputEvent::Key(Key { code: "enter".into(), ..Key::default() })]);
    }
}

mod usage {
    use super::*;

    #[rstest]
    #[case::only_standard_input(&["-"], true)]
    #[case::an_event(&[r#"{"text":"x"}"#], false)]
    #[case::two_of_them(&["-", "-"], false)]
    fn follow_reads_only_standard_input(#[case] events: &[&str], #[case] fine: bool) {
        let events: Vec<String> = events.iter().map(ToString::to_string).collect();

        let checked = only_standard_input(&events);

        assert_eq!(checked.is_ok(), fine);
        assert!(fine || matches!(checked, Err(Error::WrongUsage(_))));
    }
}
