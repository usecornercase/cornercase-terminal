use super::*;
use crate::bots::memory::Focus;

const KEPT: &str = "- prefers squash merges\n";

fn memory_of(c: &Chat) -> String {
    folder::notes(&c.app.panels.bots.live[0].folder).1
}

fn with_memory() -> Chat {
    let mut c = chat(ANSWER);
    folder::write_memory(&c.app.panels.bots.live[0].folder, KEPT).expect("memory");
    c.app.window_attached();
    c.app.show_bot(0);
    c
}

fn opened(c: &mut Chat) {
    let key = c.app.panels.bots.live[0].key;
    c.app.open_bot_memory(key);
}

fn dialog(c: &Chat) -> &crate::bots::memory::Dialog {
    match &c.app.overlay {
        Some(Overlay::BotMemory(dialog)) => dialog,
        _ => panic!("the memory dialog is not open"),
    }
}

fn save(c: &mut Chat) {
    send_key(&mut c.app, KeyCode::Tab, KeyModifiers::NONE);
    send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
}

fn button(wanted: ui::bot_memory::Hit) -> Position {
    let spots = (0..AREA.height).flat_map(|y| (0..AREA.width).map(move |x| Position::new(x, y)));
    spots.into_iter().find(|pos| ui::bot_memory::hit(AREA, *pos) == Some(wanted)).expect("the button")
}

#[test]
fn the_bot_menu_opens_its_memory_with_the_text_it_keeps() {
    let mut c = with_memory();
    menu::open_menu(&mut c);
    pick(&mut c.app, "memory…");
    assert_eq!(dialog(&c).text.text(), KEPT);
    let ui::Overlay::BotMemory(view) = overlay_shown(&c.app, AREA) else { panic!("the memory dialog") };
    assert_eq!(view.title, "Memory › PRs");
    let text = format!("{}", render(&mut c.app, AREA).backend());
    assert!(text.contains("Memory › PRs") && text.contains("prefers squash merges"), "{text}");
}

#[test]
fn saving_writes_memory_md_and_tells_the_next_message() {
    let mut c = with_memory();
    opened(&mut c);
    type_into(&mut c.app, "- runs tests first\n");
    save(&mut c);
    assert!(c.app.overlay.is_none());
    assert_eq!(memory_of(&c), format!("- runs tests first\n{KEPT}"));
    assert_eq!(c.app.panels.bots.live[0].bot.memory_edits, 1);
}

#[test]
fn save_and_cancel_are_buttons_too() {
    let mut c = with_memory();
    opened(&mut c);
    type_into(&mut c.app, "x");
    let cancel = button(ui::bot_memory::Hit::Cancel);
    click(&mut c.app, cancel);
    assert!(c.app.overlay.is_none());
    assert_eq!(memory_of(&c), KEPT);
    opened(&mut c);
    type_into(&mut c.app, "y");
    let save = button(ui::bot_memory::Hit::Save);
    click(&mut c.app, save);
    assert_eq!(memory_of(&c), format!("y{KEPT}"));
}

#[test]
fn an_untouched_memory_closes_without_writing() {
    let mut c = with_memory();
    opened(&mut c);
    save(&mut c);
    assert!(c.app.overlay.is_none());
    assert_eq!(c.app.panels.bots.live[0].bot.memory_edits, 0);
}

#[test]
fn esc_leaves_the_memory_as_it_was() {
    let mut c = with_memory();
    opened(&mut c);
    type_into(&mut c.app, "gone");
    send_key(&mut c.app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(c.app.overlay.is_none());
    assert_eq!(memory_of(&c), KEPT);
}

#[test]
fn a_memory_the_bot_changed_meanwhile_is_kept_until_saved_again() {
    let mut c = with_memory();
    opened(&mut c);
    type_into(&mut c.app, "mine ");
    folder::write_memory(&c.app.panels.bots.live[0].folder, "- the bot's own\n").expect("the bot writes");
    save(&mut c);
    assert_eq!(memory_of(&c), "- the bot's own\n");
    assert!(dialog(&c).error.as_deref().is_some_and(|error| error.contains("save again")));
    assert_eq!(dialog(&c).focus, Focus::Save);
    send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(c.app.overlay.is_none());
    assert_eq!(memory_of(&c), format!("mine {KEPT}"));
}

#[test]
fn removing_the_bot_closes_its_memory() {
    let mut c = with_memory();
    opened(&mut c);
    c.app.bot_remove("prs").expect("removed");
    assert!(c.app.overlay.is_none());
}

#[test]
fn the_keys_menu_opens_the_memory_of_the_bot_on_screen_or_says_none_is() {
    let mut c = with_memory();
    c.app.config.prefix_key = "ctrl+]".into();
    for key in [']', 'b', 'm'] {
        let modifiers = if key == ']' { KeyModifiers::CONTROL } else { KeyModifiers::NONE };
        send_key(&mut c.app, KeyCode::Char(key), modifiers);
    }
    assert_eq!(dialog(&c).text.text(), KEPT);
    c.app.overlay = None;
    c.app.leave_bot();
    for key in [']', 'b', 'm'] {
        let modifiers = if key == ']' { KeyModifiers::CONTROL } else { KeyModifiers::NONE };
        send_key(&mut c.app, KeyCode::Char(key), modifiers);
    }
    assert!(c.app.overlay.is_none());
    assert_eq!(toast(&c.app), Some("no bot's chat is on screen"));
}

fn type_into(app: &mut App, text: &str) {
    app.handle_event(AppEvent::Input(Event::Paste(text.to_string())), AREA).expect("paste");
}
