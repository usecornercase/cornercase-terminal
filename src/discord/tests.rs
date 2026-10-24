use serde_json::json;

use super::*;
use crate::test_util::FakeHttp;

pub(super) const TOKEN: &str = "MTIzNDU2Nzg5.Gx.SECRET-token";

pub(super) fn bot(server: &FakeHttp) -> Bot {
    Bot::new(&server.url(), TOKEN)
}

pub(super) fn body(request: &str) -> Value {
    let (_, body) = request.split_once("\r\n\r\n").expect("a body");
    serde_json::from_str(body).expect("a JSON body")
}

fn key(text: &str, data: &str) -> Key {
    Key { text: text.into(), data: data.into() }
}

fn send(keyboard: Keyboard) -> Order {
    Order::Send { chat: 42, text: "**hi**".into(), keyboard, reply_to: None, ask: None, tag: Some(1) }
}

mod updates {
    use super::*;

    #[test]
    fn a_direct_message_is_read_with_its_reply() {
        let d = json!({"id": "1300000000000000001", "channel_id": "1200", "type": 19, "content": "/status",
            "timestamp": "2026-10-15T10:00:00.000000+00:00", "author": {"id": "9", "username": "ada"},
            "message_reference": {"message_id": "1300000000000000000"}});

        let update = update("MESSAGE_CREATE", &d, None);

        let message = Message {
            id: 1_300_000_000_000_000_001,
            chat: 1200,
            user: 9,
            private: true,
            guild: None,
            date: time::parse_time("2026-10-15T10:00:00Z").expect("a time"),
            name: "ada".into(),
            text: "/status".into(),
            reply_to: Some(1_300_000_000_000_000_000),
        };
        assert_eq!(update, Some(Update::Message(message)));
    }

    #[rstest::rstest]
    #[case::in_a_server(json!({"id": "1", "channel_id": "2", "guild_id": "3", "author": {"id": "9"}}))]
    #[case::from_a_bot(json!({"id": "1", "channel_id": "2", "author": {"id": "9", "bot": true}}))]
    #[case::a_system_message(json!({"id": "1", "channel_id": "2", "type": 7, "author": {"id": "9"}}))]
    fn other_messages_are_left_out(#[case] d: Value) {
        assert_eq!(update("MESSAGE_CREATE", &d, None), None);
    }

    fn in_a_guild(guild: &str) -> Value {
        json!({"id": "1", "channel_id": "2", "guild_id": guild, "content": "go on",
            "timestamp": "2026-10-15T10:00:00+00:00", "author": {"id": "9", "username": "ada"}})
    }

    #[rstest::rstest]
    #[case::in_direct_messages_only(None)]
    #[case::in_another_server(Some(4))]
    fn a_message_in_a_server_that_is_not_the_forum_s_is_left_out(#[case] forum: Option<i64>) {
        assert_eq!(update("MESSAGE_CREATE", &in_a_guild("3"), forum), None);
    }

    #[test]
    fn a_message_in_the_forum_s_server_is_read_with_its_guild() {
        let Some(Update::Message(message)) = update("MESSAGE_CREATE", &in_a_guild("3"), Some(3)) else {
            panic!("a message")
        };

        assert_eq!((message.guild, message.private, message.chat, message.text.as_str()), (Some(3), false, 2, "go on"));
    }

    #[test]
    fn a_direct_message_has_no_guild_in_forum_mode_too() {
        let d = json!({"id": "1", "channel_id": "2", "author": {"id": "9"}});

        let Some(Update::Message(message)) = update("MESSAGE_CREATE", &d, Some(3)) else { panic!("a message") };

        assert_eq!((message.guild, message.private), (None, true));
    }

    #[test]
    fn a_button_press_carries_the_interaction_and_its_token() {
        let d = json!({"id": "77", "token": "tok", "type": 3, "channel_id": "1200",
            "user": {"id": "9"}, "message": {"id": "55"}, "data": {"custom_id": "ab12:s:5"}});

        let press = Press { id: "77:tok".into(), chat: 1200, user: 9, message: 55, data: "ab12:s:5".into() };
        assert_eq!(update("INTERACTION_CREATE", &d, None), Some(Update::Press(press)));
    }

    #[test]
    fn a_member_presses_through_its_member_user_and_other_interactions_are_ignored() {
        let d = json!({"id": "77", "token": "tok", "type": 3, "channel": {"id": "1200"},
            "member": {"user": {"id": "9"}}, "message": {"id": "55"}, "data": {"custom_id": "x"}});
        let Some(Update::Press(press)) = update("INTERACTION_CREATE", &d, None) else { panic!("a press") };
        assert_eq!((press.user, press.chat), (9, 1200));

        assert_eq!(
            update("INTERACTION_CREATE", &json!({"id": "1", "token": "t", "type": 4, "channel_id": "2"}), None),
            None
        );
        assert_eq!(update("TYPING_START", &json!({}), None), None);
    }

    #[test]
    fn a_slash_command_is_read_as_its_typed_text() {
        let d = json!({"id": "1300000000000000001", "token": "tok", "type": 2, "application_id": "123456789",
            "channel_id": "1200", "user": {"id": "9", "username": "ada"}, "data": {"name": "new", "type": 1}});

        let message = Message {
            id: 1_300_000_000_000_000_001,
            chat: 1200,
            user: 9,
            private: true,
            guild: None,
            date: (1_300_000_000_000_000_001_i64 >> 22) / 1000 + 1_420_070_400,
            name: "ada".into(),
            text: "/new".into(),
            reply_to: None,
        };
        assert_eq!(update("INTERACTION_CREATE", &d, None), Some(Update::Message(message)));
        let expected = Invoked { id: "1300000000000000001:tok".into(), app: Some("123456789".into()) };
        assert_eq!(invoked("INTERACTION_CREATE", &d), Some(expected));
        assert_eq!(invoked("MESSAGE_CREATE", &d), None);
    }
}

mod buttons {
    use super::*;

    #[test]
    fn a_long_row_is_split_into_rows_of_five() {
        let row: Vec<Key> = (0..7).map(|i| key(&i.to_string(), &format!("d{i}"))).collect();

        let pages = pages(&vec![row]);

        assert_eq!(pages.len(), 1);
        let sizes: Vec<usize> = pages[0].iter().map(|row| row["components"].as_array().map_or(0, Vec::len)).collect();
        assert_eq!(sizes, vec![5, 2]);
        assert_eq!(pages[0][1]["components"][1]["custom_id"], "d6");
    }

    #[test]
    fn more_than_five_rows_go_to_the_messages_after() {
        let keyboard: Keyboard = (0..12).map(|i| vec![key("k", &format!("d{i}"))]).collect();

        let calls = request(&send(keyboard), "", None);

        let rows: Vec<usize> = calls
            .iter()
            .map(|c| c.body.as_ref().map_or(0, |b| b["components"].as_array().map_or(0, Vec::len)))
            .collect();
        assert_eq!(rows, vec![5, 5, 2]);
        assert_eq!(calls[1].body.as_ref().map(|b| b["content"].clone()), Some(json!(MORE)));
    }

    #[test]
    fn a_label_is_cut_at_eighty_characters() {
        let pages = pages(&vec![vec![key(&"x".repeat(100), "d")]]);

        assert_eq!(pages[0][0]["components"][0]["label"].as_str().map(str::len), Some(80));
    }
}

mod orders {
    use super::*;

    #[test]
    fn a_message_replies_without_mentions_and_asks_with_a_line() {
        let order = Order::Send {
            chat: 42,
            text: "Write to claude:".into(),
            keyboard: Vec::new(),
            reply_to: Some(7),
            ask: Some("message for claude".into()),
            tag: None,
        };

        let calls = request(&order, "", None);

        let body = calls[0].body.clone().expect("a body");
        assert_eq!((calls[0].method, calls[0].path.as_str()), (Method::Post, "/channels/42/messages"));
        assert_eq!(body["content"], "Write to claude:\n-# reply to this message");
        assert_eq!(body["allowed_mentions"], json!({"parse": []}));
        assert_eq!(body["message_reference"], json!({"message_id": "7", "fail_if_not_exists": false}));
    }

    #[test]
    fn a_long_question_keeps_its_reply_line_within_the_limit() {
        let order = Order::Send {
            chat: 42,
            text: "x".repeat(2100),
            keyboard: Vec::new(),
            reply_to: None,
            ask: Some("prompt".into()),
            tag: None,
        };

        let calls = request(&order, "", None);

        let content = calls[0].body.as_ref().and_then(|b| b["content"].as_str()).unwrap_or_default().to_string();
        assert_eq!(markdown::length(&content), markdown::MAX);
        assert!(content.ends_with("\n-# reply to this message"), "{content}");
    }

    #[test]
    fn edits_and_buttons_patch_the_message() {
        let edit = request(&Order::Edit { chat: 1, message: 2, text: "t".into(), keyboard: Vec::new() }, "", None);
        let buttons = request(&Order::Buttons { chat: 1, message: 2, keyboard: Vec::new() }, "", None);

        assert_eq!((edit[0].method, edit[0].path.as_str()), (Method::Patch, "/channels/1/messages/2"));
        assert_eq!(buttons[0].body, Some(json!({"components": []})));
    }

    #[test]
    fn an_answer_with_text_is_an_ephemeral_follow_up_and_without_it_nothing() {
        let calls = request(&Order::Answer { id: "77:tok".into(), text: "stale".into() }, "123", None);

        assert_eq!(calls[0].path, "/webhooks/123/tok");
        assert_eq!(calls[0].body.as_ref().map(|b| b["flags"].clone()), Some(json!(64)));
        assert_eq!(request(&Order::Answer { id: "77:tok".into(), text: String::new() }, "123", None), Vec::new());
    }

    #[test]
    fn a_reaction_puts_the_encoded_emoji() {
        let calls = request(&Order::React { chat: 1, message: 2, emoji: "👍".into() }, "", None);

        assert_eq!(
            (calls[0].method, calls[0].path.as_str()),
            (Method::Put, "/channels/1/messages/2/reactions/%F0%9F%91%8D/@me")
        );
    }
}

mod api {
    use super::*;

    #[test]
    fn the_app_id_comes_from_the_token() {
        assert_eq!(application_id(TOKEN).as_deref(), Some("123456789"));
        assert_eq!(application_id("not a token"), None);
        let link = install_link(TOKEN).expect("a link");
        assert!(link.contains("client_id=123456789&integration_type=1&scope=applications.commands"), "{link}");
    }

    #[test]
    fn the_bot_says_its_name_with_its_token_in_the_header() {
        let server = FakeHttp::start(vec![("GET /users/@me", 200, r#"{"id":"123456789","username":"cornercase"}"#)]);

        assert_eq!(bot(&server).me(), Ok("cornercase".into()));
        let request = server.request(0);
        assert!(request.contains("Bot MTIzNDU2Nzg5.Gx.SECRET-token"), "{request}");
        assert!(request.contains("DiscordBot (https://usecornercase.dev"), "{request}");
    }

    #[rstest::rstest]
    #[case::unauthorized(401, r#"{"message":"401: Unauthorized","code":0}"#, Failure::Rejected)]
    #[case::wait(429, r#"{"message":"rate limited","retry_after":1.2}"#, Failure::Wait(2))]
    #[case::closed(403, r#"{"message":"Cannot send messages to this user","code":50007}"#, Failure::Closed)]
    #[case::other(400, r#"{"message":"Invalid Form Body","code":50035}"#, Failure::Refused("Invalid Form Body (code 50035)".into()))]
    fn discord_failures_are_told_apart(#[case] status: u16, #[case] answer: &str, #[case] failure: Failure) {
        let server = FakeHttp::start(vec![("GET /users/@me", status, answer)]);

        assert_eq!(bot(&server).me(), Err(failure));
    }

    #[test]
    fn the_token_never_shows_in_an_error() {
        let answer = r#"{"message":"bad bot MTIzNDU2Nzg5.Gx.SECRET-token"}"#;
        let server = FakeHttp::start(vec![("GET /users/@me", 400, answer)]);

        let error = bot(&server).me().expect_err("a failure").to_string();

        assert_eq!(error, "Discord said: bad bot <token>");
        assert!(!format!("{:?}", bot(&server)).contains("SECRET"));
    }

    #[rstest::rstest]
    #[case::private(json!({"bot_public": false, "integration_types_config": {"0": {}, "1": {}}}), None)]
    #[case::public(json!({"bot_public": true, "integration_types_config": {"0": {}, "1": {}}}), Some(PUBLIC))]
    #[case::guild_only(json!({"bot_public": true, "integration_types_config": {"0": {}}}), Some(NO_USER_INSTALL))]
    #[case::unknown(json!({}), None)]
    fn the_app_settings_give_their_advice(#[case] app: Value, #[case] expected: Option<&str>) {
        assert_eq!(advice(&app), expected);
    }

    #[test]
    fn the_advice_is_read_from_the_app_and_a_failure_is_not_an_answer() {
        let app = r#"{"id":"1","bot_public":true,"integration_types_config":{"1":{}}}"#;
        let server = FakeHttp::start(vec![("GET /oauth2/applications/@me", 200, app)]);
        let failing = FakeHttp::start(vec![("GET /oauth2/applications/@me", 500, "{}")]);

        assert_eq!(bot(&server).read_advice(), Ok(Some(PUBLIC)));
        assert!(bot(&failing).read_advice().is_err());
    }

    #[test]
    fn pairing_writes_to_the_owner_in_a_direct_message() {
        let server = FakeHttp::start(vec![
            ("GET /oauth2/applications/@me", 200, r#"{"id":"123456789","owner":{"id":"9","username":"ada"}}"#),
            ("POST /users/@me/channels", 200, r#"{"id":"1200"}"#),
            ("POST /channels/1200/messages", 200, r#"{"id":"55"}"#),
        ]);

        let (advice, paired) = bot(&server).pair("hello");
        let chat = paired.expect("paired");

        assert_eq!((advice, chat.id, chat.user, chat.name.as_str()), (Advice::Read(None), 1200, 9, "ada"));
        assert_eq!(body(&server.request(1)), json!({"recipient_id": "9"}));
        assert_eq!(body(&server.request(2))["content"], "hello");
    }

    #[test]
    fn pairing_waits_while_discord_cannot_write_yet_and_a_team_names_its_owner() {
        let app = r#"{"id":"1","owner":{"id":"0"},"team":{"name":"acme","owner_user_id":"9",
            "members":[{"user":{"id":"9","username":"ada"}}]}}"#;
        let server = FakeHttp::start(vec![
            ("GET /oauth2/applications/@me", 200, app),
            ("POST /users/@me/channels", 200, r#"{"id":"1200"}"#),
            ("POST /channels/1200/messages", 403, r#"{"message":"Cannot send messages to this user","code":50007}"#),
        ]);

        assert_eq!(Bot::owner(&serde_json::from_str(app).expect("JSON")), Ok((9, "ada".into())));
        assert_eq!(bot(&server).pair("hello"), (Advice::Read(None), Err(Failure::Closed)));
    }

    #[test]
    fn pairing_names_a_missing_access_as_discord_does() {
        let server = FakeHttp::start(vec![
            ("GET /oauth2/applications/@me", 200, r#"{"id":"123456789","owner":{"id":"9","username":"ada"}}"#),
            ("POST /users/@me/channels", 200, r#"{"id":"1200"}"#),
            ("POST /channels/1200/messages", 403, r#"{"message":"Missing Access","code":50001}"#),
        ]);

        let (_, paired) = bot(&server).pair("hello");

        assert_eq!(paired, Err(Failure::Refused("Missing Access (code 50001)".into())));
    }

    #[test]
    fn a_long_keyboard_reports_the_first_message_and_resumes_after_a_limit() {
        let server = FakeHttp::start(vec![
            ("POST /channels/42/messages", 200, r#"{"id":"100"}"#),
            ("POST /channels/42/messages", 429, r#"{"retry_after":0}"#),
            ("POST /channels/42/messages", 200, r#"{"id":"101"}"#),
        ]);
        let keyboard: Keyboard = (0..6).map(|i| vec![key("k", &format!("d{i}"))]).collect();
        let order = send(keyboard);
        let sent = Sent::default();

        let first = bot(&server).perform(&order, &sent);
        assert_eq!(first, Err(Failure::Wait(0)));
        assert_eq!(bot(&server).perform(&order, &sent), Ok(Some(100)));

        assert_eq!(server.requests().len(), 3);
        assert_eq!(Bot::more(&sent), vec![101]);
    }

    #[test]
    fn a_button_is_acknowledged_again_after_a_short_limit() {
        let server = FakeHttp::start(vec![
            ("POST /interactions/77/tok/callback", 429, r#"{"retry_after":0.0}"#),
            ("POST /interactions/77/tok/callback", 204, ""),
        ]);

        assert_eq!(bot(&server).ack("77:tok"), Ok(()));
        assert_eq!(server.requests().len(), 2);
    }

    #[test]
    fn the_commands_are_registered_for_the_direct_message_with_the_bot() {
        let server = FakeHttp::start(vec![("PUT /applications/123456789/commands", 200, "[]")]);

        let done = bot(&server).perform(&Order::Commands, &Sent::default());

        assert_eq!(done, Ok(None));
        let registered = body(&server.request(0));
        let names: Vec<&str> = registered
            .as_array()
            .map_or(Vec::new(), |all| all.iter().filter_map(|command| command["name"].as_str()).collect());
        assert_eq!(names, vec!["status", "new", "away", "back"]);
        assert_eq!(
            registered[1],
            json!({"name": "new", "description": "start an agent", "type": 1,
                "integration_types": [1], "contexts": [1]})
        );
    }

    #[test]
    fn a_command_is_acknowledged_out_of_sight_and_its_placeholder_removed() {
        let server = FakeHttp::start(vec![
            ("POST /interactions/77/tok/callback", 204, ""),
            ("DELETE /webhooks/123456789/tok/messages/@original", 204, ""),
        ]);
        let invoked = Invoked { id: "77:tok".into(), app: None };

        assert_eq!(bot(&server).acknowledge(&invoked), Ok(()));
        assert_eq!(bot(&server).dismiss(&invoked), Ok(()));
        assert_eq!(body(&server.request(0)), json!({"type": 5, "data": {"flags": 64}}));
        assert!(server.request(1).starts_with("DELETE /webhooks/123456789/tok/messages/@original"));
    }

    #[test]
    fn an_answer_follows_up_on_the_app_of_the_token() {
        let server = FakeHttp::start(vec![("POST /webhooks/123456789/tok", 200, r#"{"id":"5"}"#)]);

        let done = bot(&server).perform(&Order::Answer { id: "77:tok".into(), text: "stale".into() }, &Sent::default());

        assert_eq!(done, Ok(None));
        assert_eq!(body(&server.request(0))["content"], "stale");
    }
}
