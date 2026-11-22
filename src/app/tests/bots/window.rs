use super::*;
use crate::agents::headless::Action;

fn drawn(app: &mut App) -> String {
    format!("{}", render(app, AREA).backend())
}

fn click_bot(app: &mut App, i: usize) {
    let row = bot_row(app, i);
    render(app, AREA);
    click(app, Position::new(row.x + 3, row.y));
}

fn show(c: &mut Chat) {
    c.app.window_attached();
    click_bot(&mut c.app, 0);
}

#[cfg(unix)]
fn project_chat(out: &str) -> Chat {
    let mut c = chat(out);
    c.app.open_here(AREA).expect("open the first project");
    c
}

fn send(c: &mut Chat, text: &str) {
    for ch in text.chars() {
        send_key(&mut c.app, KeyCode::Char(ch), KeyModifiers::NONE);
    }
    send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
}

#[test]
fn a_click_on_a_bot_shows_its_chat_and_marks_it_seen() {
    let mut c = chat(ANSWER);
    c.app.panels.bots.live[0].unread = 2;
    show(&mut c);
    assert_eq!(c.app.shown_bot(), Some(0));
    assert_eq!(c.app.panels.bots.live[0].unread, 0);
    assert_eq!(c.app.app_state().bots[0].unread, 0);
    assert!(c.app.visible_tab().is_none());
    assert_eq!(c.app.report(None).shown_bot.as_deref(), Some("prs"));
}

#[cfg(unix)]
#[test]
fn the_chat_is_drawn_in_place_of_the_panes_with_the_card_beside_it() {
    let mut c = project_chat(ANSWER);
    show(&mut c);
    let text = drawn(&mut c.app);
    assert!(text.contains("Message PRs…"), "{text}");
    assert!(text.contains("A bot is an agent of your own"), "{text}");
    assert!(text.contains("Claude Code"), "{text}");
}

#[test]
fn typing_and_enter_send_the_message_and_the_answer_is_drawn() {
    let mut c = chat(ANSWER);
    show(&mut c);
    send(&mut c, "which PRs are blocked?");
    assert_eq!(c.lines()[0].text, "which PRs are blocked?");
    c.settle();
    let text = drawn(&mut c.app);
    assert!(text.contains("Two are blocked."), "{text}");
    assert!(c.app.messages.toast.is_none());
    assert_eq!(c.app.panels.bots.live[0].unread, 0);
    assert!(c.app.panels.bots.live[0].draft.is_empty());
}

#[test]
fn an_empty_message_sends_nothing() {
    let mut c = chat(ANSWER);
    show(&mut c);
    send(&mut c, "  ");
    assert_eq!(c.lines(), []);
}

#[test]
fn text_sent_while_a_turn_runs_is_held_until_its_turn_starts() {
    let mut c = chat(ANSWER);
    show(&mut c);
    c.hold();
    send(&mut c, "first");
    c.started(1);
    send(&mut c, "second");
    let text = drawn(&mut c.app);
    assert!(text.contains("held") && text.contains("thinking"), "{text}");
    c.release(ANSWER);
    c.started(2);
    c.settle();
    assert!(!drawn(&mut c.app).contains("held"));
}

#[test]
fn stop_ends_the_turn_and_new_conversation_resets_it() {
    let mut c = chat(ANSWER);
    show(&mut c);
    c.hold();
    send(&mut c, "long job");
    c.started(1);
    let pane = c.app.layout(AREA).pane;
    let areas = ui::chat_areas(pane, 1, 1);
    let thinking = ui::thinking_row(areas.lines);
    let stop = ui::stop_button(areas.lines, &ui::Thinking { text: String::from("thinking · 1s"), stopping: false });
    assert!(!stop.is_empty() && stop.y == thinking.y);
    click(&mut c.app, Position::new(stop.x + 1, stop.y));
    c.settle();
    assert_eq!(c.lines().last().map(|line| line.text.as_str()), Some("stopped"));
    std::fs::remove_file(c.claude.with_extension("hold")).expect("release");
    click(&mut c.app, Position::new(areas.new_conversation.x + 2, areas.new_conversation.y));
    assert_eq!(c.lines().last().map(|line| line.text.as_str()), Some("new conversation"));
}

#[test]
fn new_conversation_while_a_turn_runs_explains_and_writes_nothing() {
    let mut c = chat(ANSWER);
    show(&mut c);
    c.hold();
    send(&mut c, "long job");
    c.started(1);
    let before = c.lines().len();
    let areas = ui::chat_areas(c.app.layout(AREA).pane, 1, 1);
    click(&mut c.app, Position::new(areas.new_conversation.x + 2, areas.new_conversation.y));
    assert_eq!(toast(&c.app), Some("PRs is answering: stop it first"));
    assert_eq!(c.lines().len(), before);
    std::fs::remove_file(c.claude.with_extension("hold")).expect("release");
}

#[test]
fn an_answer_in_a_chat_that_is_not_shown_makes_a_toast_with_go_that_shows_it() {
    let mut c = chat(ANSWER);
    c.say("hi");
    c.settle();
    let toast = c.app.messages.toast.as_ref().expect("a toast");
    let token = toast.go.clone().expect("a go");
    assert_eq!(c.app.panels.bots.live[0].unread, 1);
    c.app.notification_clicked(&token);
    assert_eq!(c.app.shown_bot(), Some(0));
    assert_eq!(c.app.panels.bots.live[0].unread, 0);
}

#[test]
fn clicking_the_toast_go_shows_the_chat() {
    let mut c = chat(ANSWER);
    c.say("hi");
    c.settle();
    let token = c.app.messages.toast.as_ref().and_then(|toast| toast.go.clone()).expect("a go");
    c.app.window_attached();
    c.app.notification_clicked(&token);
    assert!(c.app.bot_shown());
}

#[test]
fn an_answer_on_screen_is_neither_unread_nor_notified() {
    let mut c = chat(ANSWER);
    show(&mut c);
    c.say("hi");
    c.settle();
    assert_eq!(c.app.panels.bots.live[0].unread, 0);
    assert!(c.app.messages.toast.is_none());
    assert_eq!(c.app.outbox.take_notifications().len(), 0);
    assert_eq!(c.app.panels.bots.shown.as_ref().map(|shown| shown.lines.len()), Some(4));
}

#[cfg(unix)]
#[test]
fn projects_on_the_page_goes_back_to_the_panes() {
    let mut c = project_chat(ANSWER);
    show(&mut c);
    assert!(c.app.bot_shown());
    press_back(&mut c.app);
    assert!(!c.app.bot_shown() && !c.app.bots_page());
    assert!(c.app.visible_tab().is_some());
}

#[test]
fn esc_clears_a_draft_and_then_does_nothing() {
    let mut c = chat(ANSWER);
    show(&mut c);
    for ch in "abc".chars() {
        send_key(&mut c.app, KeyCode::Char(ch), KeyModifiers::NONE);
    }
    send_key(&mut c.app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(c.app.panels.bots.live[0].draft.is_empty());
    send_key(&mut c.app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(c.app.bot_shown());
}

#[test]
fn esc_on_an_empty_draft_closes_the_side_panel() {
    let mut c = chat(ANSWER);
    show(&mut c);
    c.app.toggle_todo();
    assert!(c.app.panel_shown());
    send_key(&mut c.app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(!c.app.panel_shown());
}

#[test]
fn a_paste_keeps_its_line_breaks_in_the_draft() {
    let mut c = chat(ANSWER);
    show(&mut c);
    c.app.handle_event(AppEvent::Input(Event::Paste("one\ntwo".into())), AREA).expect("paste");
    assert_eq!(c.app.panels.bots.live[0].draft.text(), "one\ntwo");
}

#[test]
fn an_example_fills_the_field_and_sends_nothing() {
    let mut c = chat(ANSWER);
    show(&mut c);
    render(&mut c.app, AREA);
    let shown = c.app.shown_chat(AREA).expect("a chat");
    let areas = ui::chat_areas(c.app.layout(AREA).pane, 1, 1);
    let y = ui::row_y(areas.lines, shown.view.rows.len(), true, shown.view.rows.len() - 1);
    click(&mut c.app, Position::new(areas.lines.x + 20, y));
    assert_eq!(c.app.panels.bots.live[0].draft.text(), ui::EXAMPLES[3]);
    assert_eq!(c.lines(), []);
}

#[cfg(unix)]
fn pane_shows_nothing_of(c: &mut Chat, words: &[&str]) {
    type_line(&mut c.app, "markerend");
    shows(&mut c.app, &c.rx, "markerend");
    let text = screen(&mut c.app);
    for word in words {
        assert!(!text.contains(word), "the pane got {word}: {text}");
    }
}

#[cfg(unix)]
#[test]
fn keys_while_a_chat_shows_never_reach_a_pane() {
    let mut c = project_chat(ANSWER);
    type_line(&mut c.app, "echo cat-\"\"starts; cat -v");
    shows(&mut c.app, &c.rx, "cat-starts");
    show(&mut c);
    for ch in "abc".chars() {
        send_key(&mut c.app, KeyCode::Char(ch), KeyModifiers::NONE);
    }
    send_key(&mut c.app, KeyCode::Char('b'), KeyModifiers::CONTROL);
    c.app.handle_event(AppEvent::Input(Event::Paste("pasted".into())), AREA).expect("paste");
    send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
    press_back(&mut c.app);
    assert_eq!(c.lines().first().map(|line| line.kind), Some(chat::Kind::You));
    pane_shows_nothing_of(&mut c, &["abc", "pasted", "^B"]);
}

#[cfg(unix)]
#[test]
fn a_key_while_a_bot_shows_but_the_chat_has_no_keys_reaches_no_pane() {
    let mut c = project_chat(ANSWER);
    type_line(&mut c.app, "echo cat-\"\"starts; cat -v");
    shows(&mut c.app, &c.rx, "cat-starts");
    show(&mut c);
    c.app.screen.nav = Some(ui::Nav::Bots);
    assert!(!c.app.chat_typing());
    send_key(&mut c.app, KeyCode::Char('q'), KeyModifiers::NONE);
    c.app.handle_event(AppEvent::Input(Event::Paste("pasted".into())), AREA).expect("paste");
    c.app.screen.nav = None;
    press_back(&mut c.app);
    pane_shows_nothing_of(&mut c, &["pasted", "qmarker"]);
}

#[test]
fn the_text_read_stays_put_when_steps_arrive_while_scrolled_up() {
    let mut c = chat(ANSWER);
    c.app.window_attached();
    questions(&mut c, 30);
    c.app.show_bot(0);
    c.hold();
    c.say("long job");
    c.started(1);
    c.app.panels.bots.shown.as_mut().expect("shown").scroll = 6;
    let before = drawn(&mut c.app);
    let row = |text: &str| text.lines().position(|line| line.contains("question 24"));
    let turn = c.app.panels.bots.live[0].turn.as_ref().map(|turn| turn.id).expect("a turn");
    for n in 0..6 {
        let step = Action { tool: "Bash".into(), text: format!("step {n}") };
        c.app.bot_event(crate::app::bots::BotEvent::Step { bot: "prs".into(), turn, step });
    }
    let after = drawn(&mut c.app);
    std::fs::remove_file(c.claude.with_extension("hold")).expect("release");
    assert_eq!(row(&before), row(&after), "{before}\n{after}");
}

#[cfg(unix)]
fn started_line(c: &Chat, pane: u64, boot: &str) -> Line {
    let agent = chat::AgentRef { kind: "claude".into(), pane, workspace: 1, project: 1, boot: boot.into() };
    let path = c.app.panels.bots.live[0].folder.join(folder::CHAT_FILE);
    let line = Line::started(1, 10, "▶ started claude in cornercase", agent, None);
    chat::append(&path, &line).expect("append");
    line
}

#[cfg(unix)]
fn find(text: &str, needle: &str) -> Position {
    let (y, row) = text.lines().enumerate().find(|(_, row)| row.contains(needle)).expect("the text on screen");
    let x = row[..row.find(needle).expect("found")].chars().count();
    Position::new(u16::try_from(x).expect("x") + 1, u16::try_from(y).expect("y"))
}

#[cfg(unix)]
#[test]
fn a_click_on_show_shows_that_tab_and_leaves_the_chat() {
    let mut c = project_chat(ANSWER);
    let pane = c.app.term().expect("a pane").id;
    started_line(&c, pane, crate::control::server_token());
    started_line(&c, 999, "another-boot");
    c.app.set_bots_dir(c.dir.path());
    show(&mut c);
    let text = drawn(&mut c.app);
    assert!(text.contains("▶ started claude in cornercase  show") || text.contains("show "), "{text}");
    assert_eq!(c.app.shown_bot(), Some(0));
    let at = find(&text, " show ");
    click(&mut c.app, at);
    assert_eq!(c.app.shown_bot(), None);
    assert_eq!(c.app.term().map(|term| term.id), Some(pane));
}

#[cfg(unix)]
#[test]
fn a_line_of_another_run_of_the_server_reads_closed() {
    let mut c = project_chat(ANSWER);
    let pane = c.app.term().expect("a pane").id;
    started_line(&c, pane, "another-boot");
    c.app.set_bots_dir(c.dir.path());
    show(&mut c);
    let text = drawn(&mut c.app);
    assert!(text.contains("closed") && !text.contains(" show "), "{text}");
}

#[test]
fn retry_under_a_failed_message_sends_it_again() {
    let mut c = chat(FAILED);
    show(&mut c);
    c.say("hi");
    c.retried();
    assert_eq!(c.lines().last().map(|line| line.kind), Some(chat::Kind::Bot));
    let argv = &c.argvs()[1];
    assert!(argv.trim_end().ends_with("## The user says\\n\\nhi\"]"), "{argv}");
    assert!(!argv.contains("## You were just made"), "{argv}");
}
