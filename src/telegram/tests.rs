use serde_json::json;

use super::*;
use crate::messenger::Key;
use crate::test_util::FakeHttp;

const TOKEN: &str = "123456:SECRET-token";

fn bot(server: &FakeHttp) -> Bot {
    Bot::new(&server.url(), TOKEN)
}

fn body(request: &str) -> Value {
    let (_, body) = request.split_once("\r\n\r\n").expect("a body");
    serde_json::from_str(body).expect("a JSON body")
}

mod updates {
    use super::*;

    #[test]
    fn messages_and_button_presses_are_read_and_the_next_offset_follows_the_last() {
        let values = vec![
            json!({"update_id": 7, "message": {"message_id": 3, "date": 1_760_000_000, "text": "/status",
                "chat": {"id": 42, "type": "private"}, "from": {"id": 9, "username": "ada"},
                "reply_to_message": {"message_id": 2}}}),
            json!({"update_id": 8, "callback_query": {"id": "q1", "data": "ab12:s:5", "from": {"id": 9},
                "message": {"message_id": 4, "chat": {"id": 42}}}}),
            json!({"update_id": 9, "edited_message": {}}),
        ];

        let (updates, next) = parse(&values);

        let message = Message {
            id: 3,
            chat: 42,
            user: 9,
            private: true,
            guild: None,
            date: 1_760_000_000,
            name: "@ada".into(),
            text: "/status".into(),
            reply_to: Some(2),
        };
        let press = Press { id: "q1".into(), chat: 42, user: 9, message: 4, data: "ab12:s:5".into() };
        assert_eq!(updates, vec![Update::Message(message), Update::Press(press)]);
        assert_eq!(next, Some(10));
    }

    #[test]
    fn a_photo_is_a_message_without_text_and_a_group_is_not_private() {
        let values = vec![json!({"update_id": 1, "message": {"message_id": 1, "photo": [],
            "chat": {"id": -5, "type": "group"}, "from": {"id": 9, "first_name": "Ada"}}})];

        let (updates, _) = parse(&values);

        let Some(Update::Message(message)) = updates.first() else { panic!("a message: {updates:?}") };
        assert_eq!((message.text.as_str(), message.private, message.name.as_str()), ("", false, "Ada"));
    }
}

mod orders {
    use super::*;

    fn key(text: &str, data: &str) -> Key {
        Key { text: text.into(), data: data.into() }
    }

    #[test]
    fn a_message_carries_its_buttons_its_reply_and_html() {
        let order = Order::Send {
            chat: 42,
            text: "<b>hi</b> &amp; bye".into(),
            keyboard: vec![vec![key("screen", "ab12:s:5")]],
            reply_to: Some(7),
            ask: None,
            tag: Some(1),
        };

        let (method, body) = request(&order, true);

        assert_eq!(method, "sendMessage");
        assert_eq!(body["parse_mode"], "HTML");
        assert_eq!(body["reply_parameters"]["message_id"], 7);
        assert_eq!(body["reply_markup"]["inline_keyboard"][0][0]["callback_data"], "ab12:s:5");
        let (_, plain) = request(&order, false);
        assert_eq!((plain["text"].as_str(), plain.get("parse_mode")), (Some("hi & bye"), None));
    }

    #[test]
    fn a_question_asks_for_a_reply() {
        let order = Order::Send {
            chat: 42,
            text: "Write to claude:".into(),
            keyboard: Vec::new(),
            reply_to: None,
            ask: Some("message for claude".into()),
            tag: Some(2),
        };

        let (_, body) = request(&order, true);

        assert_eq!(body["reply_markup"], json!({"force_reply": true, "input_field_placeholder": "message for claude"}));
    }

    #[test]
    fn emptying_the_buttons_sends_an_empty_keyboard() {
        let (method, body) = request(&Order::Buttons { chat: 1, message: 2, keyboard: Vec::new() }, true);

        assert_eq!((method, &body["reply_markup"]), ("editMessageReplyMarkup", &json!({"inline_keyboard": []})));
    }
}

mod forum_orders {
    use super::*;

    #[rstest::rstest]
    #[case::post(Order::Post {
        forum: 1,
        title: "web › main".into(),
        tag_id: 2,
        text: "hi".into(),
        keyboard: Vec::new(),
        tag: Some(3),
    })]
    #[case::retag(Order::Retag { thread: 4, tag_id: 2 })]
    #[case::close(Order::Close { thread: 4, lock: true })]
    #[case::reopen(Order::Reopen { thread: 4 })]
    #[case::remove(Order::Remove { thread: 4 })]
    fn telegram_ignores_them_without_calling(#[case] order: Order) {
        let server = FakeHttp::start(Vec::<(&str, u16, &str)>::new());

        assert_eq!(bot(&server).perform(&order, &Plain::default()), Ok(None));
        assert_eq!(server.requests(), Vec::<String>::new());
    }
}

mod api {
    use super::*;

    #[test]
    fn the_bot_says_its_name() {
        let server = FakeHttp::start(vec![(
            "POST /bot123456:SECRET-token/getMe",
            200,
            r#"{"ok":true,"result":{"id":1,"is_bot":true,"username":"cornercase_bot"}}"#,
        )]);

        assert_eq!(bot(&server).me(), Ok("cornercase_bot".into()));
    }

    #[rstest::rstest]
    #[case::unauthorized(401, r#"{"ok":false,"error_code":401,"description":"Unauthorized"}"#, Failure::Rejected)]
    #[case::conflict(409, r#"{"ok":false,"error_code":409,"description":"Conflict"}"#, Failure::Conflict)]
    #[case::wait(429, r#"{"ok":false,"error_code":429,"parameters":{"retry_after":7}}"#, Failure::Wait(7))]
    fn telegram_failures_are_told_apart(#[case] status: u16, #[case] answer: &str, #[case] failure: Failure) {
        let server = FakeHttp::start(vec![("POST /bot123456:SECRET-token/getMe", status, answer)]);

        assert_eq!(bot(&server).me(), Err(failure));
    }

    #[test]
    fn the_token_never_shows_in_an_error() {
        let answer = r#"{"ok":false,"error_code":400,"description":"bad bot 123456:SECRET-token"}"#;
        let server = FakeHttp::start(vec![("POST /bot123456:SECRET-token/getMe", 400, answer)]);

        let error = bot(&server).me().expect_err("a failure").to_string();

        assert_eq!(error, "Telegram said: bad bot <token>");
    }

    #[test]
    fn updates_are_asked_for_from_an_offset_with_a_wait() {
        let server = FakeHttp::start(vec![(
            "POST /bot123456:SECRET-token/getUpdates",
            200,
            r#"{"ok":true,"result":[{"update_id":5}]}"#,
        )]);

        let updates = bot(&server).updates(5, Duration::from_secs(3)).expect("updates");

        let asked = body(&server.request(0));
        assert_eq!(updates, vec![json!({"update_id": 5})]);
        assert_eq!((asked["offset"].as_i64(), asked["timeout"].as_u64()), (Some(5), Some(3)));
    }
}
