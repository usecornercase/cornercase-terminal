use serde_json::json;

use super::*;
use crate::discord::tests::{TOKEN, body, bot};
use crate::discord::{Sent, install_link};
use crate::messenger::link::Transport as _;
use crate::messenger::{Key, Keyboard};
use crate::test_util::FakeHttp;

const OWNER: &str = r#"{"id":"123456789","owner":{"id":"9","username":"ada"},
    "integration_types_config":{"0":{},"1":{}},"flags":262144}"#;
const TAGGED: &str = r#"{"id":"800","type":15,"name":"cornercase","available_tags":[
    {"id":"11","name":"working"},{"id":"12","name":"needs you"},{"id":"13","name":"done"}]}"#;

fn home() -> Server {
    Server { id: 7, name: "home".into() }
}

fn forum(channel: i64) -> Forum {
    Forum { guild: 7, guild_name: "home".into(), channel, tags: Tags { working: 11, waiting: 12, done: 13 } }
}

fn post(keyboard: Keyboard) -> Order {
    Order::Post {
        forum: 800,
        title: "app › main".into(),
        tag_id: 12,
        text: "! claude needs you".into(),
        keyboard,
        tag: Some(1),
    }
}

mod links {
    use super::*;

    #[test]
    fn the_server_install_asks_for_the_bot_with_the_forum_permissions() {
        assert_eq!(
            guild_install_link(TOKEN).as_deref(),
            Some(
                "https://discord.com/oauth2/authorize?client_id=123456789&integration_type=0\
                 &scope=bot+applications.commands&permissions=292057844816"
            )
        );
        assert_eq!(guild_install_link("not a token"), None);
    }

    #[test]
    fn the_user_install_is_unchanged() {
        assert_eq!(
            install_link(TOKEN).as_deref(),
            Some(
                "https://discord.com/oauth2/authorize?client_id=123456789&integration_type=1&scope=applications.commands"
            )
        );
    }
}

mod checklist {
    use super::*;

    #[test]
    fn without_guild_install_no_server_is_read() {
        let app = r#"{"owner":{"id":"9"},"integration_types_config":{"1":{}},"flags":0}"#;
        let server = FakeHttp::start(vec![("GET /oauth2/applications/@me", 200, app)]);

        assert_eq!(bot(&server).checklist(), Ok(Checklist::default()));
        assert_eq!(server.requests().len(), 1);
    }

    #[test]
    fn only_the_servers_of_the_owner_count() {
        let server = FakeHttp::start(vec![
            ("GET /oauth2/applications/@me", 200, OWNER),
            ("GET /users/@me/guilds", 200, r#"[{"id":"7","name":"home"},{"id":"8","name":"work"}]"#),
            ("GET /guilds/7", 200, r#"{"id":"7","owner_id":"9"}"#),
            ("GET /guilds/8", 200, r#"{"id":"8","owner_id":"5"}"#),
        ]);

        let checklist = Checklist { guild_install: true, content: true, servers: vec![home()] };
        assert_eq!(bot(&server).checklist(), Ok(checklist));
    }

    #[test]
    fn a_team_app_counts_the_servers_of_the_team_owner() {
        let app = r#"{"team":{"owner_user_id":"9","members":[]},"integration_types_config":{"0":{}},
            "flags":524288}"#;
        let server = FakeHttp::start(vec![
            ("GET /oauth2/applications/@me", 200, app),
            ("GET /users/@me/guilds", 200, r#"[{"id":"7","name":"home"}]"#),
            ("GET /guilds/7", 200, r#"{"id":"7","owner_id":"9"}"#),
        ]);

        let checklist = Checklist { guild_install: true, content: true, servers: vec![home()] };
        assert_eq!(bot(&server).checklist(), Ok(checklist));
    }

    #[test]
    fn without_the_content_intent_the_server_is_listed_but_no_forum_is_made() {
        let app = r#"{"owner":{"id":"9"},"integration_types_config":{"0":{}},"flags":8192}"#;
        let server = FakeHttp::start(vec![
            ("GET /oauth2/applications/@me", 200, app),
            ("GET /users/@me/guilds", 200, r#"[{"id":"7","name":"home"}]"#),
            ("GET /guilds/7", 200, r#"{"id":"7","owner_id":"9"}"#),
        ]);

        let checklist = Checklist { guild_install: true, content: false, servers: vec![home()] };
        assert_eq!(bot(&server).set_up(None, None), Ok(Progress { checklist, forum: None }));
        assert_eq!(server.requests().len(), 3);
    }
}

mod channel {
    use super::*;

    #[test]
    fn the_forum_is_made_private_with_its_three_tags() {
        let server = FakeHttp::start(vec![("POST /guilds/7/channels", 201, TAGGED)]);

        assert_eq!(bot(&server).create_forum(&home()), Ok(forum(800)));
        assert_eq!(
            body(&server.request(0)),
            json!({
                "name": "cornercase",
                "type": 15,
                "default_auto_archive_duration": 10080,
                "available_tags": [{"name": "working"}, {"name": "needs you"}, {"name": "done"}],
                "permission_overwrites": [
                    {"id": "7", "type": 0, "deny": "1024"},
                    {"id": "123456789", "type": 1, "allow": "292057844800"},
                ],
            })
        );
    }

    #[rstest::rstest]
    #[case::a_missing_permission(403, 50013, Failure::Missing("Manage Channels"))]
    #[case::an_unknown_server(404, 10004, Failure::Removed)]
    #[case::no_access(403, 50001, Failure::Removed)]
    fn a_refusal_is_named(#[case] status: u16, #[case] code: i64, #[case] failure: Failure) {
        let answer = format!(r#"{{"message":"no","code":{code}}}"#);
        let server = FakeHttp::start(vec![("POST /guilds/7/channels", status, answer)]);

        assert_eq!(bot(&server).create_forum(&home()), Err(failure));
    }

    #[test]
    fn a_missing_permission_says_which() {
        assert_eq!(Failure::Missing("Manage Channels").to_string(), "missing permission: Manage Channels");
    }

    #[test]
    fn missing_tags_are_added_and_the_others_kept() {
        let partial = r#"{"id":"800","available_tags":[{"id":"11","name":"working"},{"id":"30","name":"mine"}]}"#;
        let server = FakeHttp::start(vec![("GET /channels/800", 200, partial), ("PATCH /channels/800", 200, TAGGED)]);

        assert_eq!(bot(&server).ensure_tags(&forum(800)), Ok(forum(800)));
        assert_eq!(
            body(&server.request(1)),
            json!({"available_tags": [{"id": "11", "name": "working"}, {"id": "30", "name": "mine"},
                {"name": "needs you"}, {"name": "done"}]})
        );
    }

    #[test]
    fn a_forum_with_its_tags_is_left_alone() {
        let server = FakeHttp::start(vec![("GET /channels/800", 200, TAGGED)]);

        assert_eq!(bot(&server).ensure_tags(&forum(800)), Ok(forum(800)));
        assert_eq!(server.requests().len(), 1);
    }

    #[test]
    fn a_lost_forum_is_found_again_by_its_name() {
        let server = FakeHttp::start(vec![
            ("GET /channels/500", 404, r#"{"message":"Unknown Channel","code":10003}"#),
            (
                "GET /guilds/7/channels",
                200,
                r#"[{"id":"1","type":0,"name":"cornercase"},{"id":"800","type":15,"name":"cornercase"}]"#,
            ),
            ("GET /channels/800", 200, TAGGED),
        ]);

        assert_eq!(bot(&server).forum(&home(), Some(&forum(500))), Ok(forum(800)));
    }

    #[test]
    fn without_a_forum_one_is_made() {
        let server = FakeHttp::start(vec![
            ("GET /guilds/7/channels", 200, r#"[{"id":"1","type":0,"name":"general"}]"#),
            ("POST /guilds/7/channels", 201, TAGGED),
        ]);

        assert_eq!(bot(&server).forum(&home(), None), Ok(forum(800)));
    }
}

mod posts {
    use super::*;

    #[test]
    fn a_post_opens_a_thread_with_its_tag_and_reports_it() {
        let server =
            FakeHttp::start(vec![("POST /channels/800/threads", 201, r#"{"id":"700","message":{"id":"701"}}"#)]);
        let sent = Sent::default();

        assert_eq!(bot(&server).perform(&post(Vec::new()), &sent), Ok(Some(701)));
        assert_eq!(Bot::posted(&sent), Some(700));
        assert_eq!(
            body(&server.request(0)),
            json!({
                "name": "app › main",
                "applied_tags": ["12"],
                "message": {"content": "! claude needs you", "components": [], "allowed_mentions": {"parse": []}},
            })
        );
    }

    #[test]
    fn buttons_past_the_fifth_row_follow_in_the_thread() {
        let server = FakeHttp::start(vec![
            ("POST /channels/800/threads", 201, r#"{"id":"700"}"#),
            ("POST /channels/700/messages", 200, r#"{"id":"702"}"#),
        ]);
        let keyboard: Keyboard = (0..6).map(|i| vec![Key { text: "k".into(), data: format!("d{i}") }]).collect();
        let sent = Sent::default();

        assert_eq!(bot(&server).perform(&post(keyboard), &sent), Ok(Some(700)));
        assert_eq!(Bot::more(&sent), vec![702]);
        assert_eq!(body(&server.request(1))["content"], "…");
    }

    #[test]
    fn a_refused_post_names_the_permission() {
        let server = FakeHttp::start(vec![("POST /channels/800/threads", 403, r#"{"message":"no","code":50013}"#)]);

        assert_eq!(bot(&server).perform(&post(Vec::new()), &Sent::default()), Err(Failure::Missing("Send Messages")));
        assert_eq!(Bot::posted(&Sent::default()), None);
    }

    #[test]
    fn a_retag_patches_the_thread() {
        let server = FakeHttp::start(vec![("PATCH /channels/700", 200, r#"{"id":"700"}"#)]);

        assert_eq!(bot(&server).perform(&Order::Retag { thread: 700, tag_id: 13 }, &Sent::default()), Ok(None));
        assert_eq!(body(&server.request(0)), json!({"applied_tags": ["13"]}));
    }

    #[test]
    fn a_retag_or_a_close_of_an_archived_thread_names_the_thread() {
        let server =
            FakeHttp::start(vec![("PATCH /channels/700", 400, r#"{"message":"Thread is archived","code":50083}"#)]);
        let order = Order::Retag { thread: 700, tag_id: 13 };

        let failure = bot(&server).perform(&order, &Sent::default()).expect_err("a failure");

        assert_eq!(failure, Failure::Archived);
        assert_eq!(Bot::archived(&order, &failure), Some(700));
        assert_eq!(Bot::archived(&Order::Close { thread: 700, lock: false }, &failure), Some(700));
        assert_eq!(Bot::archived(&Order::Reopen { thread: 700 }, &failure), None);
    }

    fn sending(chat: i64) -> Order {
        Order::Send { chat, text: "hi".into(), keyboard: Vec::new(), reply_to: None, ask: None, tag: Some(1) }
    }

    fn refusing(path: &'static str, code: i64) -> FakeHttp {
        let answer = format!(r#"{{"message":"no","code":{code}}}"#);
        FakeHttp::start(vec![(path, 404, answer)])
    }

    #[test]
    fn a_thread_deleted_by_hand_is_gone() {
        let server = refusing("POST /channels/700/messages", 10003);
        let order = sending(700);

        let failure = bot(&server).posting(Some(42)).perform(&order, &Sent::default()).expect_err("a failure");

        assert_eq!(Bot::gone(&order, &failure), Some(700));
        assert_eq!(Bot::gone(&order, &Failure::Missing("Send Messages")), None);
    }

    #[rstest::rstest]
    #[case::an_unknown_channel(10003)]
    #[case::no_access(50001)]
    #[case::an_unknown_server(10004)]
    fn a_refusal_in_the_dm_is_not_the_forums(#[case] code: i64, #[values(None, Some(42))] dm: Option<i64>) {
        let server = refusing("POST /channels/42/messages", code);
        let order = sending(42);

        let failure = bot(&server).posting(dm).perform(&order, &Sent::default()).expect_err("a failure");

        assert_eq!(failure, Failure::Refused(format!("no (code {code})")));
        assert_eq!(Bot::gone(&order, &failure), None);
    }

    #[rstest::rstest]
    #[case::an_unknown_channel(10003, Failure::Lost)]
    #[case::no_access(50001, Failure::Removed)]
    fn a_refusal_of_a_forum_order_is_the_forums(#[case] code: i64, #[case] failure: Failure) {
        let server = refusing("PATCH /channels/700", code);

        let got = bot(&server).perform(&Order::Retag { thread: 700, tag_id: 11 }, &Sent::default());

        assert_eq!(got, Err(failure));
    }

    #[rstest::rstest]
    #[case::a_retag(Order::Retag { thread: 700, tag_id: 11 })]
    #[case::a_close(Order::Close { thread: 700, lock: true })]
    #[case::a_reopen(Order::Reopen { thread: 700 })]
    #[case::a_removal(Order::Remove { thread: 700 })]
    fn an_order_on_a_thread_deleted_by_hand_names_the_thread(#[case] order: Order) {
        assert_eq!(Bot::gone(&order, &Failure::Lost), Some(700));
    }

    #[test]
    fn a_post_in_a_forum_deleted_by_hand_is_a_failed_post() {
        let order = Order::Post {
            forum: 800,
            title: "web › main".into(),
            tag_id: 11,
            text: "hi".into(),
            keyboard: Vec::new(),
            tag: Some(1),
        };

        assert_eq!(Bot::gone(&order, &Failure::Lost), None);
    }
}

mod closing {
    use super::*;

    #[rstest::rstest]
    #[case::archived_and_locked(Order::Close { thread: 700, lock: true }, json!({"archived": true, "locked": true}))]
    #[case::archived_only(Order::Close { thread: 700, lock: false }, json!({"archived": true, "locked": false}))]
    #[case::reopened(Order::Reopen { thread: 700 }, json!({"archived": false, "locked": false}))]
    fn the_thread_is_patched(#[case] order: Order, #[case] expected: Value) {
        let server = FakeHttp::start(vec![("PATCH /channels/700", 200, r#"{"id":"700"}"#)]);

        assert_eq!(bot(&server).perform(&order, &Sent::default()), Ok(None));
        assert_eq!(body(&server.request(0)), expected);
    }

    #[test]
    fn a_removed_thread_is_deleted() {
        let server = FakeHttp::start(vec![("DELETE /channels/700", 200, r#"{"id":"700"}"#)]);

        assert_eq!(bot(&server).perform(&Order::Remove { thread: 700 }, &Sent::default()), Ok(None));
        let request = server.request(0);
        assert!(request.starts_with("DELETE /channels/700 "));
        assert_eq!(request.split_once("\r\n\r\n").map(|(_, body)| body), Some(""));
    }

    #[rstest::rstest]
    #[case::close(Order::Close { thread: 700, lock: true }, "PATCH /channels/700")]
    #[case::reopen(Order::Reopen { thread: 700 }, "PATCH /channels/700")]
    #[case::remove(Order::Remove { thread: 700 }, "DELETE /channels/700")]
    fn a_refusal_names_manage_threads(#[case] order: Order, #[case] route: &'static str) {
        let server = FakeHttp::start(vec![(route, 403, r#"{"message":"no","code":50013}"#)]);

        assert_eq!(bot(&server).perform(&order, &Sent::default()), Err(Failure::Missing("Manage Threads")));
    }
}
