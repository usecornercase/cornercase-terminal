use super::asks::{asking, giving};
use super::secrets::{OWNER, VALUE, turn};
use super::secrets_window::type_in;
use super::*;
use crate::app::overlay::Overlay;
use crate::bots::asks::{self, Hit};
use crate::control::bots::SecretAnswer;

fn asking_chat() -> Chat {
    let mut c = chat(ANSWER);
    c.app.panels.bots.live[0].turn = Some(turn());
    c.app.clients.bots.owned(OWNER, "prs");
    c
}

fn ask(c: &mut Chat, names: &[&str]) -> u64 {
    let answer = c.app.bot_secret_ask(Some(OWNER), None, &asking(names)).expect("asked");
    answer["ask"].as_u64().expect("an id")
}

fn shown(c: &mut Chat) {
    c.app.window_attached();
    c.app.show_bot(0);
}

fn view(c: &Chat) -> ui::bot_ask::BotAsk {
    let Some(overlay) = &c.app.overlay else { panic!("no dialog") };
    let Some(ui::Overlay::BotAsk(view)) = c.app.overlay_view(overlay, AREA) else { panic!("not the ask") };
    *view
}

fn asks_open(c: &Chat) -> bool {
    matches!(c.app.overlay, Some(Overlay::BotAsk(_)))
}

fn spot(c: &Chat, wanted: Hit) -> Position {
    let view = view(c);
    let found = (AREA.y..AREA.bottom()).flat_map(|y| (AREA.x..AREA.right()).map(move |x| Position::new(x, y))).find(
        |pos| match (ui::bot_ask::hit(AREA, &view, *pos), wanted) {
            (Some(Hit::Field { at, .. }), Hit::Field { at: want, .. }) => at == want,
            (found, wanted) => found == Some(wanted),
        },
    );
    found.expect("the spot")
}

fn give_at(app: &mut App) -> Option<Position> {
    render(app, AREA);
    let areas = app.chat_geometry(AREA);
    let shown = app.shown_chat(AREA).expect("a chat");
    (areas.lines.y..areas.lines.bottom())
        .flat_map(|y| (areas.lines.x..areas.lines.right()).map(move |x| Position::new(x, y)))
        .find(|pos| matches!(ui::chat_hit(&areas, &shown.view, *pos), Some(ChatHit::Row(RowHit::Give(_)))))
}

mod opening {
    use super::*;

    #[test]
    fn a_window_showing_the_chat_opens_the_dialog_without_a_toast() {
        let mut c = asking_chat();
        shown(&mut c);
        ask(&mut c, &["user", "pw"]);
        assert!(asks_open(&c));
        assert!(c.app.messages.toast.is_none());
        assert_eq!(c.app.panels.bots.live[0].unread, 0);
        let view = view(&c);
        assert_eq!(view.title, "PRs asks for a secret");
        assert_eq!(
            (view.agent.as_str(), view.access.as_str(), view.full),
            ("Claude Code", "through cornercase", false)
        );
        assert_eq!(view.about, "to log in");
        assert_eq!(view.fields.iter().map(|field| field.name.as_str()).collect::<Vec<_>>(), ["user", "pw"]);
    }

    #[test]
    fn a_draft_or_another_dialog_keeps_it_closed() {
        let mut c = asking_chat();
        shown(&mut c);
        c.app.panels.bots.live[0].draft = Editor::new("half a message");
        ask(&mut c, &["pw"]);
        assert!(c.app.overlay.is_none());
        c.app.panels.bots.live[0].draft = Editor::default();
        c.app.overlay = Some(Overlay::Usage);
        ask(&mut c, &["user"]);
        assert!(matches!(c.app.overlay, Some(Overlay::Usage)));
    }

    #[test]
    fn elsewhere_it_makes_a_toast_whose_go_opens_the_dialog() {
        let mut c = asking_chat();
        ask(&mut c, &["pw"]);
        let toast = c.app.messages.toast.as_ref().expect("a toast");
        assert_eq!(toast.message.as_str(), "PRs asks for a secret: pw");
        let token = toast.go.clone().expect("a go");
        assert_eq!(c.app.panels.bots.live[0].unread, 1);
        assert_eq!(c.app.outbox.take_notifications().len(), 1);
        c.app.notification_clicked(&token);
        assert_eq!(c.app.shown_bot(), Some(0));
        assert!(asks_open(&c));
    }

    #[test]
    fn give_on_the_notice_opens_it_until_the_ask_ends() {
        let mut c = asking_chat();
        shown(&mut c);
        c.app.panels.bots.live[0].draft = Editor::new("typing");
        let ask = ask(&mut c, &["pw"]);
        let at = give_at(&mut c.app).expect("a give button");
        click(&mut c.app, at);
        assert!(asks_open(&c));
        c.app.bot_secret_decline(None, &SecretAnswer { bot: "prs".into(), ask }).expect("declined");
        assert!(c.app.overlay.is_none(), "an ask ended elsewhere closes its dialog");
        assert_eq!(give_at(&mut c.app), None);
    }

    #[test]
    fn the_row_and_the_card_say_it_asks() {
        let mut c = asking_chat();
        ask(&mut c, &["pw"]);
        let bots = c.app.bots_view().expect("bots");
        assert_eq!((bots.entries[0].detail.as_str(), bots.entries[0].asking), ("asks for a secret", true));
        let card = c.app.bot_card(0);
        let row = card.rows.iter().find(|row| row.label == "asks").expect("an asks row");
        assert_eq!(row.value, "for a secret: pw");
    }
}

mod answering {
    use super::*;

    fn open(c: &mut Chat, names: &[&str]) -> u64 {
        shown(c);
        let ask = ask(c, names);
        render(&mut c.app, AREA);
        ask
    }

    #[test]
    fn typing_and_save_saves_and_closes_and_the_value_is_bullets() {
        let mut c = asking_chat();
        open(&mut c, &["pw"]);
        type_in(&mut c, VALUE);
        assert_eq!(view(&c).fields[0].value.text(), "•".repeat(VALUE.len()));
        let drawn = format!("{}", render(&mut c.app, AREA).backend());
        assert!(!drawn.contains(VALUE) && drawn.contains("PRs asks for a secret"), "{drawn}");
        let at = spot(&c, Hit::Button(asks::SAVE));
        click(&mut c.app, at);
        assert!(c.app.overlay.is_none());
        assert_eq!(c.app.panels.bots.live[0].secrets.find("pw").map(|entry| entry.value.0.as_str()), Some(VALUE));
        assert_eq!(c.app.panels.bots.live[0].asks.len(), 0);
    }

    #[test]
    fn enter_moves_to_an_empty_field_before_it_saves() {
        let mut c = asking_chat();
        open(&mut c, &["user", "pw"]);
        type_in(&mut c, "me");
        send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(asks_open(&c));
        assert_eq!(view(&c).focus, 1);
        type_in(&mut c, VALUE);
        send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(c.app.overlay.is_none());
        assert_eq!(c.app.panels.bots.live[0].secrets.count(), 2);
    }

    #[test]
    fn a_refusal_shows_under_the_form_which_stays() {
        let mut c = asking_chat();
        for i in 0..19 {
            c.app.bot_secret_set(None, "prs", (&format!("s{i}"), "value")).expect("set");
        }
        open(&mut c, &["pw"]);
        c.app.bot_secret_set(None, "prs", ("s19", "value")).expect("the twentieth");
        type_in(&mut c, VALUE);
        send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(asks_open(&c));
        let error = view(&c).error.expect("an error");
        assert_eq!(error, "prs has 20 secrets and 1 more would pass 20, the most there can be");
    }

    #[test]
    fn not_now_and_esc_close_it_and_the_ask_waits() {
        let mut c = asking_chat();
        open(&mut c, &["pw"]);
        let at = spot(&c, Hit::Button(asks::LATER));
        click(&mut c.app, at);
        assert!(c.app.overlay.is_none());
        let key = c.app.panels.bots.live[0].key;
        let ask = c.app.panels.bots.live[0].asks[0].id;
        c.app.open_bot_ask(key, ask);
        send_key(&mut c.app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(c.app.overlay.is_none());
        assert_eq!(c.app.panels.bots.live[0].asks.len(), 1);
    }

    #[test]
    fn never_declines() {
        let mut c = asking_chat();
        open(&mut c, &["pw"]);
        let at = spot(&c, Hit::Button(asks::NEVER));
        click(&mut c.app, at);
        assert!(c.app.overlay.is_none() && c.app.panels.bots.live[0].asks.is_empty());
        assert_eq!(c.lines().pop().map(|line| line.text), Some(String::from("secret not given: pw")));
    }

    #[test]
    fn a_click_on_a_field_moves_the_focus() {
        let mut c = asking_chat();
        open(&mut c, &["user", "pw"]);
        let at = spot(&c, Hit::Field { at: 1, col: 0 });
        click(&mut c.app, at);
        assert_eq!(view(&c).focus, 1);
    }

    #[test]
    fn a_paste_drops_its_last_line_break_and_a_tab_is_refused() {
        let mut c = asking_chat();
        open(&mut c, &["pw"]);
        c.app.handle_event(AppEvent::Input(Event::Paste(String::from("a\tb"))), AREA).expect("paste");
        assert!(view(&c).error.is_some_and(|error| error.contains("no tab")));
        c.app.handle_event(AppEvent::Input(Event::Paste(format!("{VALUE}\n"))), AREA).expect("paste");
        let Some(Overlay::BotAsk(form)) = &c.app.overlay else { panic!("the ask") };
        assert_eq!(form.values()["pw"].0, VALUE);
    }

    #[test]
    fn an_ask_given_from_another_client_closes_it() {
        let mut c = asking_chat();
        let ask = open(&mut c, &["pw"]);
        c.app.bot_secret_give(None, &giving("prs", ask, &[("pw", VALUE)])).expect("given");
        assert!(c.app.overlay.is_none());
    }
}
