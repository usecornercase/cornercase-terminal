use super::*;

fn cat_pane(app: &mut App, rx: &Receiver<AppEvent>) -> u64 {
    cat_with(app, rx, "")
}

fn quiet_cat_pane(app: &mut App, rx: &Receiver<AppEvent>) -> u64 {
    cat_with(app, rx, "stty -echo; ")
}

fn cat_with(app: &mut App, rx: &Receiver<AppEvent>, before: &str) -> u64 {
    let id = first(app);
    type_line(app, &format!("{before}echo cat-\"\"starts; cat -v"));
    pump_until(app, rx, "cat runs", |a| pane_screen(a, id).contains("cat-starts"));
    pump_until(app, rx, "cat is in the foreground", |a| pane(a, id).program(&a.config).as_deref() == Some("cat"));
    id
}

fn drafting() -> (App, Receiver<AppEvent>, u64) {
    let (mut app, rx) = app();
    let id = cat_pane(&mut app, &rx);
    type_text(&mut app, "mine");
    (app, rx, id)
}

fn claude_shown() -> (Agent, u64) {
    let mut agent = Agent::new();
    let id = agent.start(None);
    agent.app.show(Item::Pane(id)).expect("show the agent");
    agent.refreshing("the input box is seen", |a| pane(a, id).input_seen.is_some());
    (agent, id)
}

fn notice(app: &App) -> (Position, Option<Position>) {
    let pane = app.tab().and_then(|t| t.shown(app.layout(AREA).pane).first().map(|(_, r)| *r)).expect("a pane");
    let queued = app.held_label(first(app)).is_some_and(|(_, queued)| queued);
    let bar = ui::held_notice(pane, 0, queued).expect("the bar fits");
    (bar.send.as_position(), bar.wait.map(Rect::as_position))
}

fn screen_lines(app: &App, id: u64) -> Vec<String> {
    pane_screen(app, id).lines().map(str::trim_end).map(str::to_string).collect()
}

fn held_now(app: &mut App) {
    app.refresh(Instant::now());
    assert_eq!(answers(app), Vec::new(), "the send is held");
}

#[test]
fn a_send_waits_for_the_persons_draft_and_follows_it() {
    let (mut app, rx) = app();
    let id = quiet_cat_pane(&mut app, &rx);
    type_text(&mut app, "mine");

    ask(&mut app, None, send_text(id, "theirs", true, false));
    held_now(&mut app);
    let row = app.report(None).panes().into_iter().find(|row| row.pane == id).expect("the row");
    assert_eq!((row.held_sends, row.draft), (1, true));

    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    done(answered(&mut app, &rx, "the send goes out after the draft"));

    pump_until(&mut app, &rx, "cat echoes both lines", |a| screen_lines(a, id).iter().any(|l| l == "theirs"));
    let lines = screen_lines(&app, id);
    let at = |text: &str| lines.iter().position(|l| l == text);
    assert!(at("mine") < at("theirs") && at("mine").is_some(), "{lines:?}");
}

#[test]
fn recent_typing_holds_a_send_for_a_moment() {
    let (mut app, rx) = app();
    let id = cat_pane(&mut app, &rx);
    app.typing_holds = Duration::from_millis(300);
    type_text(&mut app, "x");
    send_key(&mut app, KeyCode::Char('u'), KeyModifiers::CONTROL);
    let typed = Instant::now();

    ask(&mut app, None, send_text(id, "late", false, false));
    held_now(&mut app);
    done(answered(&mut app, &rx, "the send goes out once the person stops"));

    assert!(typed.elapsed() >= Duration::from_millis(300), "it went out after {:?}", typed.elapsed());
}

#[test]
fn held_sends_go_out_one_at_a_time_in_the_order_they_came() {
    let (mut app, rx, id) = drafting();

    ask(&mut app, None, send_text(id, "first-", false, false));
    ask(&mut app, None, send_text(id, "second", false, false));
    held_now(&mut app);
    assert_eq!(app.held_label(id).map(|(text, _)| text).as_deref(), Some("2 messages waiting · goes after yours"));
    send_key(&mut app, KeyCode::Char('u'), KeyModifiers::CONTROL);

    done(answered(&mut app, &rx, "the first goes out"));
    done(answered(&mut app, &rx, "the second goes out"));
    pump_until(&mut app, &rx, "both reach cat in order", |a| pane_screen(a, id).contains("first-second"));
}

#[test]
fn a_send_that_comes_while_another_is_being_entered_waits_its_turn() {
    let (mut app, rx) = app();
    let id = quiet_cat_pane(&mut app, &rx);

    ask(&mut app, None, send_text(id, "one", true, false));
    ask(&mut app, None, send_text(id, "two", true, false));
    assert_eq!(app.held_sends(id), 1, "the second waits for the first's Enter");

    done(answered(&mut app, &rx, "the first is entered"));
    done(answered(&mut app, &rx, "the second is entered"));
    pump_until(&mut app, &rx, "both lines echo", |a| {
        let lines = screen_lines(a, id);
        let at = |text: &str| lines.iter().position(|l| l == text);
        at("one").is_some() && at("one") < at("two")
    });
}

#[test]
fn keys_from_the_window_are_dropped_between_a_paste_and_its_enter() {
    let (mut app, rx) = app();
    let id = quiet_cat_pane(&mut app, &rx);

    ask(&mut app, None, send_text(id, "sent", true, false));
    type_text(&mut app, "zz");

    assert!(app.toast.as_ref().is_some_and(|t| t.message.contains("cornercase send")), "a toast says why");
    done(answered(&mut app, &rx, "the send is entered"));
    pump_until(&mut app, &rx, "the line reaches cat", |a| screen_lines(a, id).iter().any(|l| l == "sent"));
    assert!(!pane_screen(&app, id).contains("zz"));
}

#[test]
fn force_skips_the_hold() {
    let (mut app, rx, id) = drafting();

    let send = wire::SendText { pane: Some(id), text: Some("now".into()), force: true, ..wire::SendText::default() };
    done(now(&mut app, None, Command::Send(send)));

    pump_until(&mut app, &rx, "it joins the draft", |a| pane_screen(a, id).contains("minenow"));
}

#[test]
fn force_still_waits_for_an_earlier_sends_enter_and_passes_held_ones() {
    let (mut app, rx) = app();
    let id = quiet_cat_pane(&mut app, &rx);

    ask(&mut app, None, send_text(id, "one", true, false));
    app.projects.pane_by_mut(id).expect("the pane").typed_text("mine");
    ask(&mut app, None, send_text(id, "late", true, false));
    let forced = wire::SendText {
        pane: Some(id),
        text: Some("two".into()),
        enter: true,
        force: true,
        ..wire::SendText::default()
    };
    ask(&mut app, None, Command::Send(forced));
    assert_eq!(app.held_sends(id), 2, "the forced send waits for the first's Enter");

    done(answered(&mut app, &rx, "the first is entered"));
    done(answered(&mut app, &rx, "the forced one is entered"));
    assert_eq!(app.held_sends(id), 1, "the send behind the draft still waits");
    pump_until(&mut app, &rx, "both lines echo", |a| {
        let lines = screen_lines(a, id);
        lines.iter().any(|l| l == "one") && lines.iter().any(|l| l == "two")
    });
}

#[test]
fn a_timeout_says_the_person_has_a_draft_and_nothing_was_sent() {
    let (mut app, rx, id) = drafting();

    let send =
        wire::SendText { pane: Some(id), text: Some("x".into()), timeout: Some(0.05), ..wire::SendText::default() };
    ask(&mut app, None, Command::Send(send));

    let message = error(answered(&mut app, &rx, "the hold times out"));
    assert!(message.contains("has a draft") && message.contains("nothing was sent"), "{message}");
    assert_eq!(app.held_sends(id), 0);
}

#[test]
fn send_now_in_the_band_sends_the_first_message_over_the_draft() {
    let (mut app, rx, id) = drafting();
    ask(&mut app, None, send_text(id, "now", false, false));
    held_now(&mut app);

    let (send, _) = notice(&app);
    click(&mut app, send);

    done(answers(&mut app).remove(0));
    pump_until(&mut app, &rx, "it reaches cat", |a| pane_screen(a, id).contains("minenow"));
}

#[test]
fn wait_turn_keeps_the_bar_queued_until_another_message_is_held_and_still_sends() {
    let (mut app, rx, id) = drafting();
    ask(&mut app, Some(id), send_text(id, "first-", false, false));
    held_now(&mut app);
    let label = format!("1 message waiting from pane {id} · goes after yours");
    assert_eq!(app.held_label(id), Some((label, false)));

    let wait = notice(&app).1.expect("a wait turn button");
    click(&mut app, wait);
    assert_eq!(app.held_label(id), Some(("queued: goes right after yours".into(), true)));
    assert_eq!(notice(&app).1, None, "the wait turn button goes");

    ask(&mut app, Some(id), send_text(id, "second", false, false));
    held_now(&mut app);
    assert_eq!(app.held_label(id).map(|(_, queued)| queued), Some(false), "a new message asks again");

    send_key(&mut app, KeyCode::Char('u'), KeyModifiers::CONTROL);
    done(answered(&mut app, &rx, "the first goes out after the person"));
    done(answered(&mut app, &rx, "the second follows"));
    pump_until(&mut app, &rx, "both reach cat", |a| pane_screen(a, id).contains("first-second"));
}

#[test]
fn keys_stay_immediate() {
    let (mut app, rx, id) = drafting();

    done(now(&mut app, None, press_keys(id, &["ctrl+b"])));
    pump_until(&mut app, &rx, "the key reaches cat", |a| pane_screen(a, id).contains("mine^B"));
}

#[test]
fn claude_codes_emptied_box_ends_the_draft_without_an_enter() {
    let (mut agent, id) = claude_shown();
    type_text(&mut agent.app, "para que");

    ask(&mut agent.app, None, send_text(id, "report", true, false));
    held_now(&mut agent.app);
    for _ in 0.."para que".len() {
        send_key(&mut agent.app, KeyCode::Backspace, KeyModifiers::NONE);
    }
    agent.refreshing("the empty box ends the draft", |a| !pane(a, id).draft(Instant::now()));

    done(agent.answered("the report is recorded"));
    assert_eq!(agent.app.held_sends(id), 0);
}

#[test]
fn claude_codes_dim_suggestion_is_not_a_draft() {
    let (mut agent, id) = claude_shown();
    let term = agent.app.projects.pane_by_mut(id).expect("the pane");
    term.emulator.feed(b"\x1b[2mtry fixing the tests\x1b[0m\r\x1b[2C");
    term.typed_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    term.settle_draft(Instant::now());

    assert!(!pane(&agent.app, id).draft(Instant::now()));
}
