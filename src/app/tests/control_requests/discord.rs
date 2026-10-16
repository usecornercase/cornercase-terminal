use super::*;
use crate::app::messenger::By;
use crate::project::Workspace;
use crate::test_util::FakeHttp;

const TOKEN: &str = "MTIzNDU2Nzg5.Gx.SECRET-token";
const OWNER: &str = r#"{"id":"123456789","owner":{"id":"9","username":"ada"}}"#;
const CLOSED: &str = r#"{"message":"Cannot send messages to this user","code":50007}"#;
const PUBLIC: &str = r#"{"id":"123456789","owner":{"id":"9","username":"ada"},"bot_public":true,
    "integration_types_config":{"0":{},"1":{}}}"#;

mod pairing {
    use super::*;

    fn pairing(app: &mut App, server: &FakeHttp) -> Phone {
        let phone = Phone::on(app, Via::Discord, None);
        wiring::set_up(app, Via::Discord, TOKEN, "cornercase", None);
        wiring::point(app, Via::Discord, &server.url());
        assert_eq!(app.pair(Via::Discord), None);
        phone
    }

    fn checked(apps: &[&'static str]) -> (TempDir, App, Receiver<AppEvent>, FakeHttp) {
        let dir = TempDir::new();
        let (mut app, rx) = new_app(dir.path().join("config.json"));
        let mut routes = vec![("GET /users/@me", 200, r#"{"id":"123456789","username":"cornercase"}"#)];
        routes.extend(apps.iter().map(|app| ("GET /oauth2/applications/@me", 200, *app)));
        routes.push(("GET /gateway/bot", 429, r#"{"retry_after":60}"#));
        let server = FakeHttp::start(routes);
        wiring::point(&mut app, Via::Discord, &server.url());
        app.check_messenger(Via::Discord, Secret(TOKEN.into()));
        (dir, app, rx, server)
    }

    fn closed_public() -> Vec<(&'static str, u16, &'static str)> {
        vec![
            ("GET /oauth2/applications/@me", 200, PUBLIC),
            ("POST /users/@me/channels", 200, r#"{"id":"1200"}"#),
            ("POST /channels/1200/messages", 403, CLOSED),
        ]
    }

    fn app_reads(server: &FakeHttp) -> usize {
        server.requests().iter().filter(|r| r.starts_with("GET /oauth2/applications/@me")).count()
    }

    #[test]
    fn the_owner_is_paired_once_the_app_is_installed_without_a_second_welcome() {
        let dir = TempDir::new();
        let (mut app, rx) = new_app(dir.path().join("config.json"));
        let server = FakeHttp::start(vec![
            ("GET /oauth2/applications/@me", 200, OWNER),
            ("POST /users/@me/channels", 200, r#"{"id":"1200"}"#),
            ("POST /channels/1200/messages", 403, CLOSED),
            ("POST /channels/1200/messages", 200, r#"{"id":"55"}"#),
        ]);
        let mut phone = Phone::on(&mut app, Via::Discord, None);
        wiring::set_up(&mut app, Via::Discord, TOKEN, "cornercase", None);
        wiring::point(&mut app, Via::Discord, &server.url());

        assert_eq!(app.pair(Via::Discord), None);
        let url = app.pairing_view(Via::Discord).url.unwrap_or_default();
        wait_until("Discord answers that it cannot write yet", || {
            drain(&mut app, &rx);
            app.refresh(Instant::now());
            server.requests().len() == 3
        });
        drain(&mut app, &rx);
        let waiting = (app.messenger(Via::Discord).paired(), app.pairing_view(Via::Discord).lines.len(), toast(&app));
        assert_eq!(waiting, (false, 2, None));
        wait_until("the owner is paired", || {
            drain(&mut app, &rx);
            app.refresh(Instant::now());
            app.messenger(Via::Discord).paired()
        });

        assert!(url.starts_with("https://discord.com/oauth2/authorize?client_id=123456789"), "{url}");
        let saved = std::fs::read_to_string(dir.path().join("discord.json")).expect("the paired chat is saved");
        assert!(app.overlay.is_none() && saved.contains(r#""id":1200"#), "{saved}");
        assert_eq!(toast(&app), Some("Discord connected with ada"));
        assert_eq!(phone.read(&mut app).len(), 0);
        assert_eq!(server.requests().len(), 6);
    }

    #[test]
    fn a_checked_token_of_a_public_app_says_how_to_make_it_private() {
        let (_dir, mut app, rx, _server) = checked(&[PUBLIC]);
        wait_until("the token is checked", || {
            drain(&mut app, &rx);
            app.messenger(Via::Discord).bot().is_some()
        });

        assert_eq!(app.messenger_settings(Via::Discord).note, crate::discord::PUBLIC);
        assert_eq!(app.pair(Via::Discord), None);
        let lines = app.pairing_view(Via::Discord).lines;
        assert_eq!(lines.last().map(String::as_str), Some(crate::discord::PUBLIC), "{lines:?}");
        app.overlay = None;
    }

    #[test]
    fn a_paired_public_app_says_so_once_its_link_starts() {
        let (mut app, rx) = empty_app();
        let server = FakeHttp::start(vec![
            ("GET /oauth2/applications/@me", 200, PUBLIC),
            ("GET /gateway/bot", 429, r#"{"retry_after":60}"#),
        ]);
        wiring::set_up(&mut app, Via::Discord, TOKEN, "cornercase", Some(chat()));
        wiring::point(&mut app, Via::Discord, &server.url());
        assert_eq!(app.messenger_settings(Via::Discord).note, "connected");

        wait_until("the app is read as the link starts", || {
            drain(&mut app, &rx);
            app.refresh(Instant::now());
            app.messenger_settings(Via::Discord).note == crate::discord::PUBLIC
        });
        assert_eq!(app_reads(&server), 1);
    }

    #[test]
    fn a_link_that_finds_the_app_private_clears_the_advice_of_the_token_check() {
        let (_dir, mut app, rx, server) = checked(&[PUBLIC, OWNER]);
        let answer = rx.recv_timeout(Duration::from_secs(10)).expect("the token check answers");
        app.handle_event(answer, AREA).expect("handle event");
        assert_eq!(app.messenger_settings(Via::Discord).note, crate::discord::PUBLIC);
        wait_until("the link reads the private app", || {
            drain(&mut app, &rx);
            app_reads(&server) == 2 && app.messenger_settings(Via::Discord).note == "connected"
        });
    }

    #[test]
    fn the_pairing_dialog_drops_its_advice_once_the_app_is_private() {
        let (mut app, rx) = empty_app();
        let mut routes = closed_public();
        routes.extend([
            ("GET /oauth2/applications/@me", 200, OWNER),
            ("POST /users/@me/channels", 200, r#"{"id":"1200"}"#),
            ("POST /channels/1200/messages", 403, CLOSED),
        ]);
        let server = FakeHttp::start(routes);
        let _phone = pairing(&mut app, &server);
        let advised = |app: &App| app.pairing_view(Via::Discord).lines.iter().any(|l| l == crate::discord::PUBLIC);
        wait_until("the dialog says the app is public", || {
            drain(&mut app, &rx);
            app.refresh(Instant::now());
            advised(&app)
        });
        assert_eq!(app.messenger_settings(Via::Discord).note, crate::discord::PUBLIC);
        wait_until("the private app is read again", || {
            drain(&mut app, &rx);
            app.refresh(Instant::now());
            server.requests().len() == 6 && !app.messenger(Via::Discord).paired() && !advised(&app)
        });

        assert_eq!(app.messenger_settings(Via::Discord).note, "connected");
    }

    #[test]
    fn a_pairing_read_that_fails_keeps_the_advice() {
        let (mut app, rx) = empty_app();
        let mut routes = closed_public();
        routes.push(("GET /oauth2/applications/@me", 500, r#"{"message":"500: Internal Server Error"}"#));
        let server = FakeHttp::start(routes);
        let _phone = pairing(&mut app, &server);
        wait_until("the second read fails", || {
            drain(&mut app, &rx);
            app.refresh(Instant::now());
            server.requests().len() == 4 && app.pairing_view(Via::Discord).lines.len() == 4
        });

        let lines = app.pairing_view(Via::Discord).lines;
        assert_eq!(lines.last().map(String::as_str), Some(crate::discord::PUBLIC), "{lines:?}");
        assert_eq!(app.messenger_settings(Via::Discord).note, crate::discord::PUBLIC);
    }

    #[test]
    fn a_failure_shows_in_the_dialog_and_a_closed_dialog_stops_asking() {
        let (mut app, rx) = empty_app();
        let server = FakeHttp::start(vec![("GET /oauth2/applications/@me", 401, r#"{"message":"401: Unauthorized"}"#)]);
        let _phone = pairing(&mut app, &server);
        wait_until("the failure shows", || {
            drain(&mut app, &rx);
            app.refresh(Instant::now());
            app.pairing_view(Via::Discord).lines.len() == 3
        });
        app.overlay = None;
        app.refresh(Instant::now());

        assert_eq!(app.tick(Instant::now()).filter(Duration::is_zero), None);
        assert_eq!(server.requests().len(), 1);
        assert!(!app.messenger(Via::Discord).paired());
    }
}

mod chat {
    use super::*;

    struct Listed {
        agent: Agent,
        phone: Phone,
        panes: (u64, u64),
        status: i64,
        order: Order,
    }

    fn two_listed() -> Listed {
        let mut agent = Agent::new();
        let mut phone = Phone::on(&mut agent.app, Via::Discord, Some(chat()));
        let (first, second) = (agent.start(None), agent.start(None));
        agent.refreshing("both are seen", |a| [first, second].iter().all(|&id| pane(a, id).agent.status().is_some()));
        phone.says(&mut agent.app, "/status");
        let (status, order) = phone.says_in_html(&mut agent, "the list", "**Agents**");
        Listed { agent, phone, panes: (first, second), status, order }
    }

    fn one_seen() -> (Agent, Phone, u64) {
        let mut agent = Agent::new();
        agent.app.config.agent = "claude".into();
        let phone = Phone::on(&mut agent.app, Via::Discord, Some(chat()));
        let first = agent.start(None);
        agent.refreshing("the agent is seen", |a| pane(a, first).agent.status().is_some());
        (agent, phone, first)
    }

    fn asked_to_write() -> (Agent, Phone, Order) {
        let Listed { mut agent, mut phone, status, order, .. } = two_listed();
        let write = keyboard(&order).iter().flatten().find(|key| key.text.starts_with("✎")).map(|key| key.data.clone());
        phone.taps(&mut agent.app, status, &write.expect("a write button"));
        let (_, question) = phone.says_in_html(&mut agent, "the question", "Write to **claude**");
        phone.says(&mut agent.app, "/status");
        (agent, phone, question)
    }

    #[test]
    fn a_message_and_a_button_go_both_ways() {
        let mut agent = Agent::new();
        let mut phone = Phone::on(&mut agent.app, Via::Discord, Some(chat()));
        let id = agent.start(None);
        agent.refreshing("the agent is seen", |a| pane(a, id).agent.status().is_some());

        phone.message(&mut agent.app, "/status", None, (CHAT, 77));
        phone.says(&mut agent.app, "/status");
        let (status, order) = phone.says_in_html(&mut agent, "the list", "**Agents**");
        phone.taps(&mut agent.app, status, &button(&order, "screen"));
        let (_, screen) = phone.says_in_html(&mut agent, "the screen", "```");

        assert!(html(&order).contains("○ claude · "), "{}", html(&order));
        assert!(html(&screen).contains("❯"), "{}", html(&screen));
        assert_eq!(phone.seen.iter().filter(|(_, order)| html(order).contains("**Agents**")).count(), 1);
    }

    #[test]
    fn new_takes_its_prompt_from_a_plain_message_while_another_agent_runs() {
        let (mut agent, mut phone, _) = one_seen();

        phone.says(&mut agent.app, "/new");
        phone.says_in_html(&mut agent, "the prompt question", "What should **claude** do");
        let prompt = phone.says(&mut agent.app, "write the docs");
        let (_, started) = phone.says_in_html(&mut agent, "the agent starts", "Started **claude**");

        let terms = agent.app.projects.iter().flat_map(|p| &p.workspaces).flat_map(Workspace::terms).count();
        assert_eq!(terms, 3, "a third tab runs the new agent");
        assert!(!phone.seen.iter().any(|(_, order)| reacted(prompt)(order)), "the first agent got the prompt");
        assert!(matches!(started, Order::Send { reply_to: Some(to), .. } if to == prompt));
    }

    #[test]
    fn a_slash_command_starts_the_new_agent_questions() {
        let (mut agent, mut phone, _) = one_seen();
        let d = serde_json::json!({ "id": "1300000000000000001", "token": "tok", "type": 2, "channel_id": CHAT.to_string(),
            "user": { "id": USER.to_string(), "username": "ada" }, "data": { "name": "new", "type": 1 } });

        let command = crate::discord::update("INTERACTION_CREATE", &d).expect("a command");
        update(&mut agent.app, Via::Discord, command);

        phone.says_in_html(&mut agent, "the prompt question", "What should **claude** do");
    }

    #[test]
    fn a_plain_message_after_a_command_is_no_answer_to_an_older_question() {
        let (mut agent, mut phone, _) = asked_to_write();

        phone.says(&mut agent.app, "hello");

        phone.says_in_html(&mut agent, "which agent", "Which agent is this for?");
    }

    #[test]
    fn a_notice_about_an_agent_closes_the_question_asked_before_it() {
        let (mut agent, mut phone, first) = one_seen();
        agent.app.go_away(Via::Discord, By::Command).expect("away");
        phone.read(&mut agent.app);

        phone.says(&mut agent.app, "/new");
        phone.says_in_html(&mut agent, "the prompt question", "What should **claude** do");
        let term = agent.app.projects.pane_by_mut(first).expect("agent's pane");
        term.feed(b"\x1b]7501;state=done\x07");
        phone.says_in_html(&mut agent, "the finished notice", "claude finished");
        let text = phone.says(&mut agent.app, "now run the tests");
        phone.waits(&mut agent, "the text reaches the agent", reacted(text));

        let terms = agent.app.projects.iter().flat_map(|p| &p.workspaces).flat_map(Workspace::terms).count();
        assert_eq!(terms, 2, "no tab was opened for a new agent");
    }

    #[test]
    fn a_reply_to_another_agent_leaves_the_question_open() {
        let Listed { mut agent, mut phone, panes: (first, second), status, order } = two_listed();
        let tap = |action: &str, id: u64| {
            let data = format!(":{action}:{id}");
            keyboard(&order).iter().flatten().find(|key| key.data.ends_with(&data)).map(|key| key.data.clone())
        };
        phone.taps(&mut agent.app, status, &tap("s", second).expect("a screen button"));
        let (screen, _) = phone.says_in_html(&mut agent, "the screen", "```");
        phone.taps(&mut agent.app, status, &tap("w", first).expect("a write button"));
        phone.says_in_html(&mut agent, "the question", "Write to **claude**");

        let reply = phone.replies(&mut agent.app, screen, "for the second");
        phone.waits(&mut agent, "the reply reaches its agent", reacted(reply));
        let plain = phone.says(&mut agent.app, "for the first");
        phone.waits(&mut agent, "the plain message answers the question", reacted(plain));
    }

    #[test]
    fn a_reply_to_a_message_holding_the_last_buttons_goes_where_the_first_one_does() {
        let (mut agent, mut phone, question) = asked_to_write();

        let tag = question.tag().expect("the question is tagged");
        let sent = LinkEvent::Sent { link: 1, tag, message: 9999 };
        agent.app.handle_event(AppEvent::Messenger(MessengerEvent::Link(Via::Discord, sent)), AREA).expect("sent");
        let reply = phone.replies(&mut agent.app, 9999, "hello");

        phone.waits(&mut agent, "the text is confirmed", reacted(reply));
    }
}

mod away {
    use super::*;

    #[test]
    fn the_away_command_turns_on_what_is_paired_and_skips_the_rest() {
        let (mut app, _rx) = app();
        let _telegram = Phone::paired(&mut app);
        let _discord = Phone::on(&mut app, Via::Discord, None);

        let on = value(&mut app, None, Command::Away(wire::Away { off: false }));

        assert_eq!(on["away"].as_bool(), Some(true));
        assert_eq!(on["platforms"], serde_json::json!(["Telegram"]));
        assert!(app.messenger(Via::Telegram).away && !app.messenger(Via::Discord).away);
    }

    #[test]
    fn the_away_key_turns_every_ready_platform_on_and_off() {
        let (mut app, _rx) = app();
        let _telegram = Phone::paired(&mut app);
        let _discord = Phone::on(&mut app, Via::Discord, Some(chat()));

        app.toggle_away_everywhere();
        let away = (app.messenger(Via::Telegram).away, app.messenger(Via::Discord).away);
        let said = toast(&app).map(String::from);
        app.toggle_away_everywhere();

        assert_eq!(away, (true, true));
        assert_eq!(said.as_deref(), Some("away: agents message you on Telegram and Discord"));
        assert!(!app.away_anywhere());
        assert_eq!(toast(&app), Some("back: agents no longer message you on Telegram and Discord"));
    }

    #[test]
    fn the_away_key_without_a_ready_platform_opens_the_first_bot_row() {
        let (mut app, _rx) = app();

        app.toggle_away_everywhere();

        let Some(Overlay::Settings(settings)) = &app.overlay else { panic!("settings are not open") };
        let row = settings.rows().get(settings.cursor).cloned();
        assert_eq!(row, Some(crate::settings::Row::Bot(Via::Telegram)));
    }
}
