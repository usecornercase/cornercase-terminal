use super::*;
use crate::app::messenger::{self as wiring, By, MessengerEvent};
use crate::messenger::link::Event as LinkEvent;
use crate::messenger::{Chat, Keyboard, Message, Order, Press, Update, Via};

const CHAT: i64 = 42;
const USER: i64 = 9;

struct Phone {
    orders: Receiver<Order>,
    next: i64,
    seen: Vec<(i64, Order)>,
}

fn chat() -> Chat {
    Chat { id: CHAT, user: USER, name: "@ada".into(), paired: 1 }
}

fn html(order: &Order) -> &str {
    match order {
        Order::Send { text, .. } | Order::Edit { text, .. } => text,
        _ => "",
    }
}

fn keyboard(order: &Order) -> &Keyboard {
    static NONE: Keyboard = Vec::new();
    match order {
        Order::Send { keyboard, .. } | Order::Edit { keyboard, .. } => keyboard,
        _ => &NONE,
    }
}

fn button(order: &Order, text: &str) -> String {
    let key = keyboard(order).iter().flatten().find(|key| key.text.contains(text));
    key.unwrap_or_else(|| panic!("no `{text}` button in {order:?}")).data.clone()
}

fn update(app: &mut App, update: Update) {
    let event = AppEvent::Messenger(MessengerEvent::Link(Via::Telegram, LinkEvent::Updates(1, vec![update])));
    app.handle_event(event, AREA).expect("the update is handled");
}

impl Phone {
    fn set_up(app: &mut App, chat: Option<Chat>) -> Self {
        wiring::set_up(app, "1:t", "cornercase_bot", chat);
        Self { orders: wiring::capture(app), next: 1000, seen: Vec::new() }
    }

    fn paired(app: &mut App) -> Self {
        Self::set_up(app, Some(chat()))
    }

    fn read(&mut self, app: &mut App) -> Vec<(i64, Order)> {
        let mut got = Vec::new();
        while let Ok(order) = self.orders.try_recv() {
            self.next += 1;
            if let Some(tag) = order.tag() {
                let sent = LinkEvent::Sent { link: 1, tag, message: self.next };
                app.handle_event(AppEvent::Messenger(MessengerEvent::Link(Via::Telegram, sent)), AREA).expect("sent");
            }
            got.push((self.next, order));
        }
        self.seen.extend(got.iter().cloned());
        got
    }

    fn message(&mut self, app: &mut App, text: &str, reply_to: Option<i64>, from: (i64, i64)) -> i64 {
        self.next += 1;
        let (chat, user) = from;
        let message = Message {
            id: self.next,
            chat,
            user,
            private: true,
            date: 0,
            name: "@ada".into(),
            text: text.into(),
            reply_to,
        };
        update(app, Update::Message(message));
        self.next
    }

    fn says(&mut self, app: &mut App, text: &str) -> i64 {
        self.message(app, text, None, (CHAT, USER))
    }

    fn replies(&mut self, app: &mut App, to: i64, text: &str) -> i64 {
        self.message(app, text, Some(to), (CHAT, USER))
    }

    fn taps(app: &mut App, message: i64, data: &str) {
        let press = Press { id: format!("q{message}"), chat: CHAT, user: USER, message, data: data.into() };
        update(app, Update::Press(press));
    }

    fn waits(&mut self, agent: &mut Agent, what: &str, pick: impl Fn(&Order) -> bool) -> (i64, Order) {
        let mut found = None;
        wait_until(what, || {
            drain(&mut agent.app, &agent.rx);
            agent.app.refresh(Instant::now());
            found = self.read(&mut agent.app).into_iter().find(|(_, order)| pick(order));
            found.is_some()
        });
        found.expect("an order")
    }

    fn says_in_html(&mut self, agent: &mut Agent, what: &str, text: &str) -> (i64, Order) {
        self.waits(agent, what, |order| html(order).contains(text))
    }
}

fn reacted(message: i64) -> impl Fn(&Order) -> bool {
    move |order| matches!(order, Order::React { message: m, .. } if *m == message)
}

mod pairing {
    use super::*;

    fn code(app: &App) -> String {
        let url = app.pairing_view(Via::Telegram).url.expect("a pairing link");
        url.split_once("?start=").map(|(_, token)| token.to_string()).expect("a token")
    }

    #[test]
    fn the_code_in_the_dialog_pairs_the_phone_that_sends_it() {
        let dir = TempDir::new();
        let (mut app, _rx) = new_app(dir.path().join("config.json"));
        let mut phone = Phone::set_up(&mut app, None);
        assert_eq!(app.pair(Via::Telegram), None);
        assert!(
            app.pairing_view(Via::Telegram)
                .url
                .is_some_and(|url| url.starts_with("https://t.me/cornercase_bot?start="))
        );

        let token = code(&app);
        phone.says(&mut app, &format!("/start {token}"));

        let saved = std::fs::read_to_string(dir.path().join("telegram.json")).expect("the paired chat is saved");
        assert!(
            app.messenger(Via::Telegram).paired() && app.overlay.is_none() && saved.contains(r#""id":42"#),
            "{saved}"
        );
        assert_eq!(toast(&app), Some("Telegram connected with @ada"));
        assert!(phone.read(&mut app).iter().any(|(_, order)| html(order).contains("cornercase is connected")));
    }

    #[test]
    fn a_wrong_code_a_group_or_a_closed_dialog_pairs_nothing() {
        let (mut app, _rx) = empty_app();
        let mut phone = Phone::set_up(&mut app, None);
        app.pair(Via::Telegram);
        let token = code(&app);

        phone.says(&mut app, "/start 00000000000000000000000000000000");
        let group = Message {
            id: 1,
            chat: -5,
            user: USER,
            private: false,
            date: 0,
            name: String::new(),
            text: format!("/start {token}"),
            reply_to: None,
        };
        update(&mut app, Update::Message(group));
        app.overlay = None;
        app.refresh(Instant::now());
        phone.says(&mut app, &format!("/start {token}"));

        assert!(!app.messenger(Via::Telegram).paired());
        assert_eq!(phone.read(&mut app).len(), 0);
    }

    #[test]
    fn once_paired_only_that_chat_is_heard() {
        let (mut app, _rx) = empty_app();
        let mut phone = Phone::paired(&mut app);

        phone.message(&mut app, "/status", None, (CHAT, 77));
        phone.message(&mut app, "/status", None, (77, USER));
        assert_eq!(phone.read(&mut app).len(), 0);

        phone.says(&mut app, "/status");
        assert!(phone.read(&mut app).iter().any(|(_, order)| html(order).contains("<b>Agents</b>")));
    }
}

mod away {
    use super::*;

    #[test]
    fn the_remote_menu_turns_away_on_and_off_and_typing_changes_nothing() {
        let (mut app, _rx) = app();
        assert_eq!(app.remote_button(), None);
        let mut phone = Phone::paired(&mut app);
        assert_eq!(app.remote_button(), Some(false));

        click(&mut app, ui::remote_button(areas().usage, false).as_position());
        let labels = menu_labels(&app);
        pick(&mut app, "continue on Telegram");
        let away = (app.messenger(Via::Telegram).away, app.remote_button(), toast(&app).map(String::from));
        send_key(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
        let still = app.messenger(Via::Telegram).away;
        click(&mut app, ui::remote_button(areas().usage, true).as_position());
        pick(&mut app, "stop messaging on Telegram");

        assert_eq!(labels, ["set up your phone's browser…", "continue on Telegram"]);
        assert_eq!(away, (true, Some(true), Some("away: agents message you on Telegram".into())));
        assert!(still && !app.messenger(Via::Telegram).away);
        let said: Vec<String> = phone.read(&mut app).iter().map(|(_, order)| html(order).to_string()).collect();
        let starts: Vec<&str> = said.iter().map(|html| html.split("</b>").next().unwrap_or_default()).collect();
        assert_eq!(starts, ["<b>Away", "<b>Back"], "{said:?}");
    }

    #[test]
    fn what_is_not_set_up_opens_its_row_in_settings() {
        let (mut app, _rx) = app();
        let _phone = Phone::set_up(&mut app, None);

        click(&mut app, ui::remote_button(areas().usage, false).as_position());
        pick(&mut app, "set up Telegram…");

        let Some(Overlay::Settings(settings)) = &app.overlay else { panic!("settings are not open") };
        let row = settings.rows().get(settings.cursor).cloned();
        assert_eq!((settings.page, row), (Page::Remote, Some(crate::settings::Row::TelegramBot)));
    }

    #[test]
    fn the_away_command_needs_a_paired_phone() {
        let (mut app, _rx) = app();
        let away = |off| Command::Away(wire::Away { off });

        let refused = error(now(&mut app, None, away(false)));
        let _phone = Phone::paired(&mut app);
        let on = value(&mut app, None, away(false));
        let off = value(&mut app, None, away(true));

        assert!(refused.starts_with("Telegram is not set up"), "{refused}");
        assert_eq!((on["away"].as_bool(), off["away"].as_bool()), (Some(true), Some(false)));
    }

    #[test]
    fn a_refused_token_turns_nothing_on_and_leads_to_the_bot_row() {
        let (mut app, _rx) = app();
        let _phone = Phone::paired(&mut app);
        let refused = AppEvent::Messenger(MessengerEvent::Link(Via::Telegram, LinkEvent::Rejected(1)));
        app.handle_event(refused, AREA).expect("the refusal is handled");

        let message = error(now(&mut app, None, Command::Away(wire::Away { off: false })));
        click(&mut app, ui::remote_button(areas().usage, false).as_position());
        let labels = menu_labels(&app);

        assert!(
            message.starts_with("Telegram refused the bot token") && !app.messenger(Via::Telegram).away,
            "{message}"
        );
        assert_eq!(labels[1], "set up Telegram…");
    }

    #[test]
    fn back_from_the_phone_stops_the_messages() {
        let (mut app, _rx) = app();
        let mut phone = Phone::paired(&mut app);
        phone.says(&mut app, "/away");
        let away = app.messenger(Via::Telegram).away;

        phone.says(&mut app, "/back");

        assert!(away && !app.messenger(Via::Telegram).away);
        assert!(phone.read(&mut app).iter().any(|(_, order)| html(order).starts_with("<b>Back</b>")));
    }
}

mod agents {
    use super::*;

    fn seen_agent() -> (Agent, Phone, u64) {
        let mut agent = Agent::new();
        let phone = Phone::paired(&mut agent.app);
        let id = agent.start(None);
        agent.refreshing("the agent is seen", |a| pane(a, id).agent.status().is_some());
        (agent, phone, id)
    }

    fn away(agent: &mut Agent, phone: &mut Phone) {
        agent.app.go_away(Via::Telegram, By::Command).expect("away");
        phone.read(&mut agent.app);
    }

    #[test]
    fn a_protocol_failure_is_sent_as_a_failure_instead_of_a_finished_turn() {
        let (mut agent, mut phone, id) = seen_agent();
        away(&mut agent, &mut phone);
        let term = agent.app.projects.pane_by_mut(id).expect("agent's pane");
        term.feed(b"\x1b]7501;state=error\x07");
        let (_, order) = phone.says_in_html(&mut agent, "the failure notice", "claude failed");
        assert!(html(&order).starts_with("<b>! claude failed</b> · "));
        assert!(button(&order, "screen").ends_with(&format!(":s:{id}")));
    }

    #[test]
    fn an_explicit_done_messages_the_phone_even_without_a_working_sample_or_an_unseen_tab() {
        let (mut agent, mut phone, id) = seen_agent();
        away(&mut agent, &mut phone);
        let term = agent.app.projects.pane_by_mut(id).expect("agent's pane");
        term.feed(b"\x1b]7501;state=done\x07");
        let (_, order) = phone.says_in_html(&mut agent, "the explicit finish", "claude finished");
        assert!(html(&order).starts_with("<b>✓ claude finished</b> · "));
    }

    #[test]
    fn reported_idle_cancels_a_turn_without_sending_a_finished_message() {
        let (mut agent, mut phone, id) = seen_agent();
        away(&mut agent, &mut phone);
        let now = Instant::now();
        for (offset, bytes) in [(1, b"working".as_slice()), (2, b"idle".as_slice())] {
            let term = agent.app.projects.pane_by_mut(id).expect("agent's pane");
            let mut report = b"\x1b]7501;state=".to_vec();
            report.extend_from_slice(bytes);
            report.push(0x07);
            term.feed(&report);
            let at = now + Duration::from_secs(offset);
            agent.app.watch_agents(at);
            agent.app.watch_messengers(at);
        }
        agent.app.watch_messengers(now + Duration::from_secs(4));
        assert_eq!(phone.read(&mut agent.app), []);
    }

    #[test]
    fn a_text_reaches_the_only_agent_and_its_finished_turn_comes_back_with_its_message() {
        let (mut agent, mut phone, id) = seen_agent();
        away(&mut agent, &mut phone);

        let asked = phone.says(&mut agent.app, "fix the login");
        phone.waits(&mut agent, "the prompt is confirmed", reacted(asked));
        agent.transcript(id, &[
            r#"{"type":"assistant","timestamp":"2026-10-13T10:00:00.000Z","message":{"id":"t1","model":"claude-opus-5-5","content":[{"type":"text","text":"Fixed **the login**."}]}}"#,
        ]);
        agent.finish();
        let (notice, order) = phone.says_in_html(&mut agent, "the finished notice", "finished");
        let next = phone.replies(&mut agent.app, notice, "now add a test");
        phone.waits(&mut agent, "the reply is confirmed", reacted(next));

        assert!(html(&order).starts_with("<b>✓ claude finished</b> · "), "{}", html(&order));
        assert!(html(&order).ends_with("\n\nFixed <b>the login</b>."), "{}", html(&order));
        assert!(button(&order, "screen").ends_with(&format!(":s:{id}")));
    }

    #[test]
    fn a_header_that_would_pass_the_limit_goes_apart_from_the_message() {
        let (mut agent, mut phone, id) = seen_agent();
        agent.app.projects[0].name = Some("p".repeat(300));
        away(&mut agent, &mut phone);
        let asked = phone.says(&mut agent.app, "write a lot");
        phone.waits(&mut agent, "the prompt is confirmed", reacted(asked));
        let long = "word ".repeat(799);
        let line = format!(
            r#"{{"type":"assistant","timestamp":"2026-10-13T10:00:00.000Z","message":{{"id":"t2","model":"claude-opus-5-5","content":[{{"type":"text","text":"{}"}}]}}}}"#,
            long.trim()
        );
        agent.transcript(id, &[&line]);
        agent.finish();

        let (_, header) = phone.says_in_html(&mut agent, "the finished notice", "finished");
        let body = phone.seen.iter().map(|(_, order)| order).find(|order| html(order).starts_with("word word"));

        let size = |order: &Order| crate::telegram::html::length(&crate::telegram::html::plain(html(order)));
        let body = body.expect("the message comes apart from its header");
        assert!(!html(&header).contains("word") && size(&header) < 200, "{}", html(&header));
        assert!(size(body) <= crate::telegram::html::MAX, "{}", size(body));
    }

    #[test]
    fn a_waiting_agent_sends_its_screen_with_keys_and_refuses_text() {
        let mut agent = Agent::new();
        let mut phone = Phone::paired(&mut agent.app);
        let id = agent.start(Some("please ask me"));
        away(&mut agent, &mut phone);
        agent.refreshing("the agent works", |a| pane(a, id).agent.status() == Some(activity::Status::Working));
        agent.finish();

        let (notice, order) = phone.says_in_html(&mut agent, "the question", "needs you");
        let text = phone.replies(&mut agent.app, notice, "yes");
        let refused = phone.waits(&mut agent, "the text is refused", |o| html(o).contains("waiting for an answer"));
        Phone::taps(&mut agent.app, notice, &button(&order, "esc"));
        let pressed = phone.waits(&mut agent, "the key is pressed", |o| matches!(o, Order::Answer { .. }));

        assert!(html(&order).starts_with("<b>! claude needs you</b> · ") && html(&order).contains("<pre>"));
        assert!(matches!(refused.1, Order::Send { reply_to: Some(to), .. } if to == text));
        assert!(matches!(&pressed.1, Order::Answer { text, .. } if text == "sent esc to claude"), "{pressed:?}");
    }

    fn asking(agent: &mut Agent, phone: &mut Phone) -> (u64, i64, Order) {
        let id = agent.start(Some("two questions"));
        away(agent, phone);
        agent.refreshing("the agent works", |a| pane(a, id).agent.status() == Some(activity::Status::Working));
        agent.finish();
        let (notice, order) = phone.says_in_html(agent, "the first question", "Question 1 of 2");
        (id, notice, order)
    }

    fn questions(phone: &Phone) -> usize {
        let sent = phone.seen.iter().filter(|(_, order)| matches!(order, Order::Send { .. }));
        sent.filter(|(_, order)| html(order).starts_with("<b>! claude needs you</b>")).count()
    }

    #[test]
    fn the_next_question_comes_once_a_button_answers_the_first() {
        let mut agent = Agent::new();
        let mut phone = Phone::paired(&mut agent.app);
        let (_, notice, first) = asking(&mut agent, &mut phone);

        Phone::taps(&mut agent.app, notice, &button(&first, "1 · Yes"));
        let (_, second) = phone.says_in_html(&mut agent, "the second question", "Question 2 of 2");

        assert!(html(&second).starts_with("<b>! claude needs you</b> · "), "{}", html(&second));
        assert_eq!(button(&second, "2 · No"), button(&first, "2 · No"));
        assert_eq!(questions(&phone), 2);
    }

    #[test]
    fn a_redraw_of_the_same_question_sends_nothing() {
        let mut agent = Agent::new();
        let mut phone = Phone::paired(&mut agent.app);
        let (id, notice, first) = asking(&mut agent, &mut phone);

        done(now(&mut agent.app, None, press_keys(id, &["x"])));
        let settled = Instant::now() + Duration::from_millis(2_500);
        agent.refreshing("the redraw settles", |_| Instant::now() >= settled);
        phone.read(&mut agent.app);
        let after_redraw = questions(&phone);
        Phone::taps(&mut agent.app, notice, &button(&first, "1 · Yes"));
        phone.says_in_html(&mut agent, "the second question", "Question 2 of 2");

        assert_eq!(after_redraw, 1);
    }

    #[test]
    fn a_question_drawn_before_the_input_is_not_taken_for_its_answer() {
        let mut agent = Agent::new();
        let mut phone = Phone::paired(&mut agent.app);
        let (id, _, _) = asking(&mut agent, &mut phone);

        let term = agent.app.projects.pane_by_mut(id).expect("the pane");
        term.feed(b"\x1b[H\x1b[2JQuestion 9 of 2\r\n\xe2\x9d\xaf 1. Yes\r\n  2. No\r\n");
        term.input_at = Some(Instant::now() + Duration::from_millis(10));
        let settled = Instant::now() + Duration::from_millis(2_500);
        agent.refreshing("the input settles", |_| Instant::now() >= settled);
        phone.read(&mut agent.app);

        assert_eq!(questions(&phone), 1);
    }

    #[test]
    fn nothing_is_sent_while_not_away() {
        let mut agent = Agent::new();
        let mut phone = Phone::paired(&mut agent.app);
        let id = agent.start(Some("work"));
        agent.refreshing("the agent works", |a| pane(a, id).agent.status() == Some(activity::Status::Working));
        agent.finish();
        agent.refreshing("the agent stops", |a| pane(a, id).agent.status() != Some(activity::Status::Working));
        let settled = Instant::now() + Duration::from_millis(1_500);
        agent.refreshing("the turn settles", |_| Instant::now() >= settled);

        assert!(phone.read(&mut agent.app).iter().all(|(_, order)| !matches!(order, Order::Send { .. })));
    }

    #[test]
    fn with_several_agents_a_text_asks_which_one_and_goes_to_the_one_tapped() {
        let mut agent = Agent::new();
        let mut phone = Phone::paired(&mut agent.app);
        let (first, second) = (agent.start(None), agent.start(None));
        agent.refreshing("both are seen", |a| [first, second].iter().all(|&id| pane(a, id).agent.status().is_some()));

        let asked = phone.says(&mut agent.app, "hello");
        let (which, order) = phone.says_in_html(&mut agent, "the question", "Which agent is this for?");
        let data = keyboard(&order)[1][0].data.clone();
        Phone::taps(&mut agent.app, which, &data);
        phone.waits(&mut agent, "the text is confirmed", reacted(asked));

        assert!(data.ends_with(&format!(":d:{second}:1")), "{data}");
        assert!(phone.seen.iter().any(|(_, o)| matches!(o, Order::Edit { message, .. } if *message == which)));
    }

    #[test]
    fn status_lists_the_agents_and_a_button_shows_a_screen() {
        let (mut agent, mut phone, _) = seen_agent();

        phone.says(&mut agent.app, "/status");
        let (status, order) = phone.says_in_html(&mut agent, "the list", "<b>Agents</b>");
        Phone::taps(&mut agent.app, status, &button(&order, "screen"));
        let (_, screen) = phone.says_in_html(&mut agent, "the screen", "<pre>");

        assert!(html(&order).contains("○ claude · "), "{}", html(&order));
        assert!(keyboard(&order).iter().flatten().any(|key| key.text.starts_with("✎ claude")));
        assert!(html(&screen).contains("❯"), "{}", html(&screen));
    }

    #[test]
    fn a_button_from_before_a_restart_is_refused() {
        let (mut app, _rx) = app();
        let mut phone = Phone::paired(&mut app);
        let stale = if wiring::run(&app) == "ffff" { "eeee:s:1" } else { "ffff:s:1" };

        Phone::taps(&mut app, 5, stale);

        let told = phone.read(&mut app);
        assert!(matches!(&told[0].1, Order::Answer { text, .. } if text.contains("before cornercase restarted")));
    }

    #[test]
    fn new_starts_an_agent_with_the_prompt_the_phone_replies() {
        let mut agent = Agent::new();
        agent.app.config.agent = "claude".into();
        let mut phone = Phone::paired(&mut agent.app);

        phone.says(&mut agent.app, "/new");
        let (question, order) = phone.says_in_html(&mut agent, "the prompt question", "What should <b>claude</b> do");
        phone.replies(&mut agent.app, question, "write the docs");
        let (_, started) = phone.says_in_html(&mut agent, "the agent starts", "Started <b>claude</b>");

        assert!(matches!(&order, Order::Send { ask: Some(placeholder), .. } if placeholder == "prompt for claude"));
        let terms: Vec<u64> =
            agent.app.projects.iter().flat_map(|p| &p.workspaces).flat_map(Workspace::terms).map(|t| t.id).collect();
        assert_eq!(terms.len(), 2, "a second tab runs the agent");
        assert!(html(&started).contains("Reply to this message to write to it."));
    }
}
