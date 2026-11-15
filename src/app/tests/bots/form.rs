use super::*;
use crate::bots::form::Field;
use crate::editor::Editor;

fn bot_form(app: &App) -> &crate::bots::form::Form {
    match &app.overlay {
        Some(Overlay::BotForm(form)) => form,
        other => panic!("the bot form is not open: {other:?}"),
    }
}

fn typed(c: &mut Chat, text: &str) {
    for ch in text.chars() {
        send_key(&mut c.app, KeyCode::Char(ch), KeyModifiers::NONE);
    }
}

fn plus(c: &mut Chat) {
    render(&mut c.app, AREA);
    let title = c.app.layout(AREA).bots_title;
    click(&mut c.app, Position::new(title.right() - 2, title.y));
}

fn drawn(app: &mut App) -> String {
    format!("{}", render(app, AREA).backend())
}

#[test]
fn the_plus_of_the_section_opens_the_form() {
    let mut c = chat(ANSWER);
    plus(&mut c);
    assert!(bot_form(&c.app).editing.is_none());
}

#[test]
fn more_and_its_new_bot_open_the_form() {
    let mut c = chat(ANSWER);
    from_more(&mut c.app, "new bot…");
    assert!(matches!(c.app.overlay, Some(Overlay::BotForm(_))));
}

#[test]
fn b_in_the_keys_menu_opens_the_form() {
    let mut c = chat(ANSWER);
    c.app.config.prefix_key = "ctrl+]".into();
    send_key(&mut c.app, KeyCode::Char(']'), KeyModifiers::CONTROL);
    send_key(&mut c.app, KeyCode::Char('b'), KeyModifiers::NONE);
    assert!(matches!(c.app.overlay, Some(Overlay::BotForm(_))));
}

#[test]
fn creating_a_bot_shows_its_empty_chat_at_once() {
    let mut c = chat(ANSWER);
    plus(&mut c);
    typed(&mut c, "Digest");
    send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(c.app.overlay.is_none());
    assert_eq!(c.app.panels.bots.live.len(), 2);
    assert_eq!(c.app.shown_bot(), Some(1));
    assert!(drawn(&mut c.app).contains("Message Digest…"));
}

#[test]
fn a_name_already_taken_shows_the_servers_text_and_keeps_the_form() {
    let mut c = chat(ANSWER);
    plus(&mut c);
    typed(&mut c, "prs");
    send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(bot_form(&c.app).error.as_deref().is_some_and(|text| text.contains("already")));
    assert!(drawn(&mut c.app).contains("already"));
}

#[test]
fn the_fifty_first_bot_is_refused_under_the_form() {
    let mut c = chat(ANSWER);
    for n in 0..49 {
        let new = crate::control::bots::NewBot {
            name: format!("bot{n}"),
            agent: Some("claude".into()),
            ..crate::control::bots::NewBot::default()
        };
        c.app.bot_new(&new).expect("a bot");
    }
    plus(&mut c);
    typed(&mut c, "one-more");
    send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(bot_form(&c.app).error.as_deref().is_some_and(|text| text.contains("50")));
}

#[test]
fn an_agent_that_cannot_be_a_bot_cannot_be_chosen() {
    let mut c = chat(ANSWER);
    plus(&mut c);
    let form = bot_form(&c.app);
    let grok = form.agents.iter().position(|info| info.agent == "grok").expect("grok");
    assert!(form.agent.note(grok).is_some());
    let area = AREA;
    let Some(Overlay::BotForm(form)) = &mut c.app.overlay else { panic!("form") };
    form.focus_on(Field::Agent);
    form.agent.open();
    form.agent.choose(grok);
    assert_eq!(form.info().map(|info| info.agent.as_str()), Some("claude"));
    let _ = area;
}

#[test]
fn editing_saves_the_name_the_model_and_what_it_is_for() {
    let mut c = chat(ANSWER);
    c.app.open_bot_form(Some(c.app.panels.bots.live[0].key));
    let form = bot_form(&c.app);
    assert_eq!(form.editing.as_deref(), Some("prs"));
    let Some(Overlay::BotForm(form)) = &mut c.app.overlay else { panic!("form") };
    form.name = Editor::new("Reviews");
    form.about = Editor::new("New purpose");
    form.model.choose(1);
    send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(c.app.overlay.is_none());
    let bot = &c.app.panels.bots.live[0].bot;
    assert_eq!((bot.name.as_str(), bot.model.as_deref()), ("Reviews", Some("opus")));
    let (about, _) = folder::notes(&c.app.panels.bots.live[0].folder);
    assert_eq!(about, "New purpose");
}

#[test]
fn the_agent_is_not_focusable_when_editing() {
    let mut c = chat(ANSWER);
    c.app.open_bot_form(Some(c.app.panels.bots.live[0].key));
    send_key(&mut c.app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(bot_form(&c.app).focus, Field::Model);
}

#[test]
fn a_paste_into_what_it_is_for_keeps_its_line_breaks() {
    let mut c = chat(ANSWER);
    plus(&mut c);
    for _ in 0..5 {
        send_key(&mut c.app, KeyCode::Tab, KeyModifiers::NONE);
    }
    assert_eq!(bot_form(&c.app).focus, Field::About);
    c.app.handle_event(AppEvent::Input(Event::Paste("a\nb".into())), AREA).expect("paste");
    assert_eq!(bot_form(&c.app).about.text(), "a\nb");
}
