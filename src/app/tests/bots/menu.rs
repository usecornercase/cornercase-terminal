use super::*;

fn open_menu(c: &mut Chat) {
    render(&mut c.app, AREA);
    let a = c.app.layout(AREA);
    let row = ui::bot_rows(a.bots_list, a.pitch, 1, 0).item(0);
    right_click(&mut c.app, Position::new(row.x + 3, row.y));
}

fn drawn(app: &mut App) -> String {
    format!("{}", render(app, AREA).backend())
}

#[test]
fn a_right_click_opens_the_menu_with_its_seven_items() {
    let mut c = chat(ANSWER);
    open_menu(&mut c);
    assert_eq!(
        menu_labels(&c.app),
        ["rename", "edit…", "routines…", "secrets…", "new conversation", "open folder", "remove…"]
    );
}

#[test]
fn new_conversation_is_greyed_while_a_turn_runs() {
    let mut c = chat(ANSWER);
    c.hold();
    c.say("long job");
    c.started(1);
    open_menu(&mut c);
    assert_eq!(menu_disabled(&c.app), [false, false, false, false, true, false, false]);
    std::fs::remove_file(c.claude.with_extension("hold")).expect("release");
}

#[test]
fn the_menu_button_of_a_row_opens_the_same_menu() {
    let mut c = chat(ANSWER);
    render(&mut c.app, AREA);
    let a = c.app.layout(AREA);
    let row = ui::bot_rows(a.bots_list, a.pitch, 1, 0).item(0);
    let button = ui::bot_menu(row, a.pitch);
    assert!(!button.is_empty());
    click(&mut c.app, button.as_position());
    assert_eq!(menu_labels(&c.app).len(), 7);
}

#[test]
fn rename_keeps_the_id_and_the_shown_chat() {
    let mut c = chat(ANSWER);
    c.app.window_attached();
    c.app.show_bot(0);
    open_menu(&mut c);
    pick(&mut c.app, "rename");
    for _ in 0..3 {
        send_key(&mut c.app, KeyCode::Backspace, KeyModifiers::NONE);
    }
    for ch in "Reviews".chars() {
        send_key(&mut c.app, KeyCode::Char(ch), KeyModifiers::NONE);
    }
    send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
    let bot = &c.app.panels.bots.live[0].bot;
    assert_eq!((bot.id.as_str(), bot.name.as_str()), ("prs", "Reviews"));
    assert_eq!(c.app.shown_bot(), Some(0));
}

#[test]
fn rename_to_a_taken_name_shows_the_error_in_the_form() {
    let mut c = chat(ANSWER);
    let other =
        crate::control::bots::NewBot { name: "Taken".into(), agent: Some("claude".into()), ..Default::default() };
    c.app.bot_new(&other).expect("another bot");
    open_menu(&mut c);
    pick(&mut c.app, "rename");
    for _ in 0..3 {
        send_key(&mut c.app, KeyCode::Backspace, KeyModifiers::NONE);
    }
    for ch in "taken".chars() {
        send_key(&mut c.app, KeyCode::Char(ch), KeyModifiers::NONE);
    }
    send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(matches!(&c.app.overlay, Some(Overlay::RenameBot { error: Some(_), .. })));
    assert!(drawn(&mut c.app).contains("already"));
}

#[test]
fn remove_asks_then_deletes_the_bot_and_leaves_the_chat() {
    let mut c = chat(ANSWER);
    c.app.window_attached();
    c.app.show_bot(0);
    let folder = c.app.panels.bots.live[0].folder.clone();
    open_menu(&mut c);
    pick(&mut c.app, "remove…");
    let text = confirmation(&c.app).expect("a confirmation");
    assert_eq!(
        text,
        "Remove bot PRs? Its chat, its memory and its folder are deleted. Agents it started keep running."
    );
    send_key(&mut c.app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(folder.exists() && c.app.panels.bots.live.len() == 1);
    open_menu(&mut c);
    pick(&mut c.app, "remove…");
    send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(!folder.exists());
    assert!(c.app.panels.bots.live.is_empty() && !c.app.bot_shown());
}

#[test]
fn removing_while_it_answers_stops_it_first() {
    let mut c = chat(ANSWER);
    c.hold();
    c.say("long job");
    c.started(1);
    open_menu(&mut c);
    pick(&mut c.app, "remove…");
    match overlay_shown(&c.app, AREA) {
        ui::Overlay::Confirm(confirm) => assert_eq!(confirm.submit, "stop it first"),
        _ => panic!("a confirmation"),
    }
    send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
    c.settle();
    assert!(matches!(c.app.overlay, Some(Overlay::RemoveBot { .. })));
    send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(c.app.panels.bots.live.is_empty());
    std::fs::remove_file(c.claude.with_extension("hold")).expect("release");
}
