use rstest::rstest;

use super::*;

fn question(buttons: &[&str], timeout: Option<f64>) -> Command {
    Command::Confirm(wire::Confirm {
        title: "Ana shares PR #485".into(),
        body: "## Fix the login\n\nIt runs `npm install` first.".into(),
        buttons: buttons.iter().map(|b| (*b).to_string()).collect(),
        timeout,
    })
}

fn asked(buttons: &[&str]) -> (App, Receiver<AppEvent>) {
    let (mut app, rx) = app();
    app.set_windows(1);
    ask(&mut app, None, question(buttons, None));
    (app, rx)
}

fn button(app: &App, hit: ui::QuestionHit) -> Position {
    let Some(Overlay::Question { key, .. }) = app.overlay else { panic!("no question shown") };
    let buttons = app.question(key).expect("the question").buttons.clone();
    let cells = (0..AREA.height).flat_map(|y| (0..AREA.width).map(move |x| Position::new(x, y)));
    cells.into_iter().find(|pos| ui::question_hit(AREA, &buttons, *pos) == Some(hit)).expect("the button")
}

fn chosen(app: &mut App) -> wire::Chosen {
    match answers(app).pop().expect("an answer") {
        Response::Ok(value) => serde_json::from_value(value).expect("a choice"),
        Response::Error(message) => panic!("the question failed: {message}"),
    }
}

#[test]
fn a_click_on_a_button_answers_its_label() {
    let (mut app, _rx) = asked(&["set up and run", "check out only"]);
    assert!(answers(&mut app).is_empty(), "it waits for the person");

    let at = button(&app, ui::QuestionHit::Button(1));
    click_in(&mut app, at, AREA);

    assert_eq!(chosen(&mut app), wire::Chosen { choice: Some("check out only".into()), cancelled: false });
    assert!(app.overlay.is_none());
}

#[rstest]
#[case::by_its_button(None)]
#[case::by_a_press_outside(Some(Position::new(1, 1)))]
fn cancel_answers_cancelled(#[case] outside: Option<Position>) {
    let (mut app, _rx) = asked(&["deploy"]);

    let at = outside.unwrap_or_else(|| button(&app, ui::QuestionHit::Cancel));
    click_in(&mut app, at, AREA);

    assert_eq!(chosen(&mut app), wire::Chosen { choice: None, cancelled: true });
}

#[test]
fn esc_cancels_and_enter_answers_nothing() {
    let (mut app, _rx) = asked(&["deploy"]);

    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let after_enter = (answers(&mut app).len(), matches!(app.overlay, Some(Overlay::Question { .. })));
    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    assert_eq!(after_enter, (0, true));
    assert!(chosen(&mut app).cancelled);
}

#[test]
fn nobody_answering_times_out_and_takes_the_dialog_away() {
    let (mut app, _rx) = app();
    app.set_windows(1);
    ask(&mut app, None, question(&["deploy"], Some(1.0)));

    app.check_requests(Instant::now() + Duration::from_secs(2));

    let message = error(answers(&mut app).pop().expect("an answer"));
    assert_eq!(message, "timed out after 1s: nobody answered the question in the window");
    assert!(app.overlay.is_none());
}

#[test]
fn without_a_window_it_fails_at_once() {
    let (mut app, _rx) = app();

    let message = error(now(&mut app, None, question(&["deploy"], None)));

    assert!(message.starts_with("no cornercase window is open"), "{message}");
    assert!(app.overlay.is_none());
}

#[test]
fn the_last_window_closing_fails_it() {
    let (mut app, _rx) = asked(&["deploy"]);

    app.set_windows(0);
    app.check_requests(Instant::now());

    assert_eq!(
        error(answers(&mut app).pop().expect("an answer")),
        "the last cornercase window closed before anyone answered"
    );
    assert!(app.overlay.is_none());
}

#[test]
fn it_waits_behind_a_dialog_already_open() {
    let (mut app, _rx) = app();
    app.set_windows(1);
    app.overlay = Some(Overlay::Usage);

    ask(&mut app, None, question(&["deploy"], None));
    let behind = matches!(app.overlay, Some(Overlay::Usage));
    app.overlay = None;
    app.check_requests(Instant::now());

    assert!(behind);
    assert!(matches!(app.overlay, Some(Overlay::Question { .. })));
}

#[test]
fn a_second_question_shows_once_the_first_is_answered() {
    let (mut app, _rx) = asked(&["first"]);
    ask(&mut app, None, question(&["second"], None));

    let at = button(&app, ui::QuestionHit::Button(0));
    click_in(&mut app, at, AREA);
    let first = chosen(&mut app).choice;

    assert_eq!(first.as_deref(), Some("first"));
    let Some(Overlay::Question { key, .. }) = app.overlay else { panic!("the second question") };
    assert_eq!(app.question(key).expect("shown").buttons, ["second"]);
}

#[test]
fn the_command_leaving_takes_the_dialog_away() {
    let (mut app, _rx) = asked(&["deploy"]);

    app.forget(CLIENT);

    assert!(app.overlay.is_none());
    assert_eq!(app.take_answers(), Vec::new());
}

#[rstest]
#[case::no_button(&[], "the question needs a button")]
#[case::a_cancel_button(&["Cancel"], "`Cancel` is what cancelling prints")]
#[case::the_same_label_twice(&["go", "go"], "two buttons say `go`")]
#[case::too_many(&["a", "b", "c", "d"], "at most 3 buttons fit")]
#[case::a_blank_label(&["\u{1b}"], "a button needs a label")]
fn wrong_buttons_are_refused(#[case] buttons: &[&str], #[case] start: &str) {
    let (mut app, _rx) = app();
    app.set_windows(1);

    let message = error(now(&mut app, None, question(buttons, None)));

    assert!(message.starts_with(start), "{message}");
}

#[test]
fn what_others_wrote_is_shown_without_control_or_bidi_characters() {
    let (mut app, _rx) = app();
    app.set_windows(1);
    let command = Command::Confirm(wire::Confirm {
        title: "PR \u{202E}gnp.exe\u{1b}[2J".into(),
        body: "\u{1b}]52;c;aGk=\u{7}text".into(),
        buttons: vec!["ok\u{2066}".into()],
        timeout: None,
    });

    ask(&mut app, None, command);

    let Some(Overlay::Question { key, .. }) = app.overlay else { panic!("no question shown") };
    let question = app.question(key).expect("the question");
    assert_eq!((question.title.as_str(), question.body.as_str()), ("PR gnp.exe[2J", "]52;c;aGk=text"));
    assert_eq!(question.buttons, ["ok"]);
}
