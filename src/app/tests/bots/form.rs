use super::*;
use crate::bots::form::Field;
use crate::editor::Editor;

use crate::ui::bot_form as dialog;

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
    c.app.panels.bots.page = true;
    render(&mut c.app, AREA);
    let a = c.app.layout(AREA);
    let new = ui::bots_new(a.bots_list, a.pitch, c.app.panels.bots.live.len(), 0);
    click(&mut c.app, Position::new(new.x + 3, new.y));
}

fn drawn(app: &mut App) -> String {
    format!("{}", render(app, AREA).backend())
}

#[test]
fn new_on_the_page_opens_the_form() {
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
    assert!(c.app.panels.bots.live[1].turn.is_none());
    assert!(drawn(&mut c.app).contains("Message Digest…"));
}

#[test]
fn creating_a_bot_with_what_it_is_for_shows_it_setting_up() {
    let mut c = chat(ANSWER);
    c.app.window_attached();
    plus(&mut c);
    typed(&mut c, "Digest");
    let Some(Overlay::BotForm(form)) = &mut c.app.overlay else { panic!("form") };
    form.about = Editor::new("Review my PRs every weekday at 9");
    send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(c.app.shown_bot(), Some(1));
    assert!(c.app.panels.bots.live[1].turn.is_some());
    let text = drawn(&mut c.app);
    assert!(text.contains("setting up") && !text.contains(ui::EXAMPLES[0]), "{text}");
    let Chat { app, rx, .. } = &mut c;
    wait_until("the setup ends", || {
        drain(app, rx);
        app.panels.bots.live[1].turn.is_none()
    });
    assert!(drawn(&mut c.app).contains("Two are blocked."));
    assert_eq!(c.app.panels.bots.live[1].unread, 0);
    assert!(c.app.messages.toast.is_none());
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
    assert!(c.app.panels.bots.live[0].turn.is_none(), "a new For runs no setup");
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
    for _ in 0..6 {
        send_key(&mut c.app, KeyCode::Tab, KeyModifiers::NONE);
    }
    assert_eq!(bot_form(&c.app).focus, Field::About);
    c.app.handle_event(AppEvent::Input(Event::Paste("a\nb".into())), AREA).expect("paste");
    assert_eq!(bot_form(&c.app).about.text(), "a\nb");
}

#[test]
fn a_click_on_follow_toggles_it() {
    let mut c = chat(ANSWER);
    plus(&mut c);
    render(&mut c.app, AREA);
    let at = dialog::rect(AREA, bot_form(&c.app), Field::Follow);
    click(&mut c.app, Position::new(at.x + 2, at.y));
    assert!(!bot_form(&c.app).follow);
    click(&mut c.app, Position::new(at.x + 2, at.y));
    assert!(bot_form(&c.app).follow);
}

#[test]
fn a_paired_chat_app_makes_the_chat_apps_row_a_list_and_its_choice_is_saved() {
    let mut c = chat(ANSWER);
    from_more(&mut c.app, "new bot…");
    assert!(!bot_form(&c.app).chat_ready);
    assert!(drawn(&mut c.app).contains("pair Telegram or Discord in Settings › Remote"));
    c.app.overlay = None;
    let paired = crate::messenger::Chat { id: 42, user: 9, name: "@ada".into(), paired: 1 };
    messenger::set_up(&mut c.app, crate::messenger::Via::Telegram, "1:t", "cornercase_bot", Some(paired));
    c.app.open_bot_form(Some(c.app.panels.bots.live[0].key));
    assert!(bot_form(&c.app).chat_ready);
    assert!(drawn(&mut c.app).contains("Chat apps while away"));
    let Some(Overlay::BotForm(form)) = &mut c.app.overlay else { panic!("form") };
    form.chat_apps.choose(1);
    send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(c.app.overlay.is_none());
    assert_eq!(c.app.panels.bots.live[0].bot.chat_apps, crate::bots::ChatApps::Always);
}
