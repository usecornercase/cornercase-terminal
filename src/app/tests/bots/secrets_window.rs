use super::*;
use crate::bots::routines::{Button, Hit};
use crate::bots::secrets::Kept;

const PASSWORD: &str = "p4ss-word-long";

fn menu_open(c: &mut Chat) {
    render(&mut c.app, AREA);
    let a = c.app.layout(AREA);
    let row = ui::bot_rows(a.bots_list, a.pitch, 1, 0).item(0);
    right_click(&mut c.app, Position::new(row.x + 3, row.y));
}

fn list_view(c: &Chat) -> ui::BotRoutines {
    let Some(overlay) = &c.app.overlay else { panic!("no dialog") };
    let Some(ui::Overlay::BotSecrets(view)) = c.app.overlay_view(overlay, AREA) else { panic!("not the list") };
    *view
}

fn form_view(c: &Chat) -> ui::Form {
    let Some(overlay) = &c.app.overlay else { panic!("no dialog") };
    let Some(ui::Overlay::Form(form)) = c.app.overlay_view(overlay, AREA) else { panic!("not a form") };
    form
}

fn spot(c: &Chat, wanted: Hit) -> Position {
    let view = list_view(c);
    (0..AREA.height)
        .flat_map(|y| (0..AREA.width).map(move |x| Position::new(x, y)))
        .find(|pos| ui::routines_hit(AREA, &view, *pos) == Some(wanted))
        .unwrap_or_else(|| panic!("nothing at {wanted:?}"))
}

fn press_button(c: &mut Chat, row: usize, button: Button) {
    let rect = ui::routine_button(AREA, &list_view(c), row, button);
    assert!(!rect.is_empty(), "no {button:?} on row {row}");
    click(&mut c.app, rect.as_position());
}

fn enter(c: &mut Chat) {
    send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
}

fn type_in(c: &mut Chat, text: &str) {
    for ch in text.chars() {
        send_key(&mut c.app, KeyCode::Char(ch), KeyModifiers::NONE);
    }
}

fn new_secret(c: &mut Chat) {
    let at = spot(c, Hit::New);
    click(&mut c.app, at);
}

fn open_from_menu(c: &mut Chat) {
    menu_open(c);
    pick(&mut c.app, "secrets…");
    render(&mut c.app, AREA);
}

#[test]
fn the_menu_opens_the_dialog_and_an_empty_one_says_how_to_add() {
    let mut c = chat(ANSWER);
    open_from_menu(&mut c);
    let view = list_view(&c);
    assert_eq!(view.title, "Secrets › PRs");
    assert!(view.rows.is_empty());
    assert!(view.empty.contains("PRs has no secrets yet"), "{}", view.empty);
    assert_eq!(view.new, "+ secret");
    assert!(view.hint.contains("The bot reads this value"), "{}", view.hint);
}

#[test]
fn a_secret_is_added_in_two_steps_and_listed_by_name_and_date() {
    let mut c = chat(ANSWER);
    open_from_menu(&mut c);
    new_secret(&mut c);
    assert_eq!(form_view(&c).label, "name");
    type_in(&mut c, "Bad Name");
    enter(&mut c);
    assert!(matches!(form_view(&c).note, Some(ui::Note::Error(_))), "a refused name stays on the step");
    for _ in 0..8 {
        send_key(&mut c.app, KeyCode::Backspace, KeyModifiers::NONE);
    }
    type_in(&mut c, "campus-password");
    enter(&mut c);
    assert_eq!(form_view(&c).label, "value");
    type_in(&mut c, PASSWORD);
    enter(&mut c);
    let view = list_view(&c);
    assert_eq!(view.rows.iter().map(|row| row.name.as_str()).collect::<Vec<_>>(), ["campus-password"]);
    assert!(view.rows[0].when.starts_with("saved "), "{}", view.rows[0].when);
    let listed = c.app.bot_secret_list(None, "prs").expect("list");
    assert_eq!(listed["secrets"][0]["name"], "campus-password");
    assert_eq!(c.app.panels.bots.live[0].secrets.find("campus-password").map(|e| e.value.0.as_str()), Some(PASSWORD));
}

#[test]
fn the_value_is_drawn_as_bullets_and_never_copied() {
    let mut c = chat(ANSWER);
    open_from_menu(&mut c);
    new_secret(&mut c);
    type_in(&mut c, "pw");
    enter(&mut c);
    type_in(&mut c, PASSWORD);
    let form = form_view(&c);
    assert_eq!(form.value.text(), "•".repeat(PASSWORD.len()));
    let drawn = format!("{}", render(&mut c.app, AREA).backend());
    assert!(!drawn.contains(PASSWORD) && drawn.contains("••••"), "{drawn}");
    assert!(c.app.overlay.as_ref().is_some_and(Overlay::hides_text));
    send_key(&mut c.app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    let field = Position::new(AREA.width / 2, AREA.height / 2);
    for item in [0, 1] {
        right_click(&mut c.app, field);
        let menu = ui::menu_area(AREA, field, &["copy", "cut", "select all"]);
        click(&mut c.app, ui::menu_item(menu, item).as_position());
    }
    let copied = clipboard::osc52(PASSWORD);
    assert!(c.app.take_host_writes().iter().all(|write| *write != copied), "nothing was copied");
    assert_eq!(c.app.overlay.as_ref().and_then(|overlay| overlay.hides_text().then_some(())), Some(()));
}

#[test]
fn a_paste_drops_its_last_line_break() {
    let mut c = chat(ANSWER);
    open_from_menu(&mut c);
    new_secret(&mut c);
    type_in(&mut c, "pw");
    enter(&mut c);
    c.app.handle_event(AppEvent::Input(Event::Paste(format!("{PASSWORD}\n"))), AREA).expect("paste");
    enter(&mut c);
    assert_eq!(c.app.panels.bots.live[0].secrets.find("pw").map(|e| e.value.0.as_str()), Some(PASSWORD));
}

#[test]
fn replacing_asks_for_the_value_only_and_removing_asks_first() {
    let mut c = chat(ANSWER);
    c.app.bot_secret_set(None, "prs", ("pw", "old-value-long")).expect("set");
    open_from_menu(&mut c);
    press_button(&mut c, 0, Button::Replace);
    assert_eq!(form_view(&c).label, "value");
    type_in(&mut c, "new-value-long");
    enter(&mut c);
    assert_eq!(c.app.panels.bots.live[0].secrets.find("pw").map(|e| e.value.0.as_str()), Some("new-value-long"));

    press_button(&mut c, 0, Button::Remove);
    let Some(overlay) = &c.app.overlay else { panic!("no dialog") };
    let Some(ui::Overlay::Confirm(confirm)) = c.app.overlay_view(overlay, AREA) else { panic!("it did not ask") };
    assert_eq!(confirm.message, "Remove pw from PRs? The bot can no longer read it.");
    assert_eq!(c.app.panels.bots.live[0].secrets.count(), 1);
    send_key(&mut c.app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(c.app.panels.bots.live[0].secrets.count(), 1);
    assert_eq!(list_view(&c).rows.len(), 1);
    press_button(&mut c, 0, Button::Remove);
    enter(&mut c);
    assert_eq!(c.app.panels.bots.live[0].secrets, Kept::Open(Vec::new()));
    assert!(list_view(&c).rows.is_empty());
}

#[test]
fn a_file_that_does_not_open_offers_remove_all_after_asking() {
    let mut c = chat(ANSWER);
    std::fs::write(c.dir.path().join("prs").join(folder::SECRETS_FILE), b"garbage").expect("write");
    c.app.set_bots_dir(c.dir.path());
    open_from_menu(&mut c);
    let view = list_view(&c);
    assert_eq!(view.rows[0].name, "the secrets cannot be read");
    press_button(&mut c, 0, Button::RemoveAll);
    let Some(overlay) = &c.app.overlay else { panic!("no dialog") };
    let Some(ui::Overlay::Confirm(confirm)) = c.app.overlay_view(overlay, AREA) else { panic!("it did not ask") };
    assert!(confirm.message.starts_with("Remove every secret of PRs?"), "{}", confirm.message);
    enter(&mut c);
    assert_eq!(c.app.panels.bots.live[0].secrets, Kept::Open(Vec::new()));
}

#[test]
fn the_edit_form_lists_its_secrets_and_gets_back_with_its_edits() {
    let mut c = chat(ANSWER);
    c.app.bot_secret_set(None, "prs", ("pw", PASSWORD)).expect("set");
    c.app.bot_secret_set(None, "prs", ("user", "someone-long")).expect("set");
    c.app.open_bot_form(Some(c.app.panels.bots.live[0].key));
    send_key(&mut c.app, KeyCode::Backspace, KeyModifiers::NONE);
    type_in(&mut c, "X");
    let Some(overlay) = &c.app.overlay else { panic!("no form") };
    let Some(ui::Overlay::BotForm(form)) = c.app.overlay_view(overlay, AREA) else { panic!("not the form") };
    assert_eq!(form.secrets, "2 saved: pw, user");
    for _ in 0..8 {
        send_key(&mut c.app, KeyCode::Tab, KeyModifiers::NONE);
        if matches!(&c.app.overlay, Some(Overlay::BotForm(form)) if form.focus == crate::bots::form::Field::Secrets) {
            break;
        }
    }
    enter(&mut c);
    assert_eq!(list_view(&c).rows.len(), 2);
    send_key(&mut c.app, KeyCode::Esc, KeyModifiers::NONE);
    let Some(Overlay::BotForm(form)) = &c.app.overlay else { panic!("the form did not come back") };
    assert_eq!(form.name.text(), "PRX");
}

#[test]
fn the_card_says_how_many_secrets_and_when_they_cannot_be_read() {
    let mut c = chat(ANSWER);
    let row = |c: &Chat| c.app.bot_card(0).rows.into_iter().find(|row| row.label == "secrets");
    assert!(row(&c).is_none());
    c.app.bot_secret_set(None, "prs", ("pw", PASSWORD)).expect("set");
    assert_eq!(row(&c).map(|row| row.value), Some(String::from("1 secret")));
    c.app.bot_secret_set(None, "prs", ("user", "someone-long")).expect("set");
    assert_eq!(row(&c).map(|row| row.value), Some(String::from("2 secrets")));
    c.app.panels.bots.live[0].secrets = Kept::Unreadable;
    assert_eq!(row(&c).map(|row| (row.value, row.red)), Some((String::from("cannot be read"), true)));
}
