use super::super::following_the_app::{follow, next};
use super::socket::discord_of;
use super::*;
use crate::control::{AppState, ForumInfo, Snapshot};

type Routes = Vec<(&'static str, u16, &'static str)>;

const WORK: &str = r#"{"id":"8","owner_id":"9"}"#;
const THREE: &str = r#"[{"id":"7","name":"home"},{"id":"8","name":"work"},{"id":"6","name":"play"}]"#;
const PLAY: &str = r#"{"id":"6","owner_id":"9"}"#;
const BIG: &str = r#"[{"id":"1041234567890123457","name":"home"},{"id":"1041234567890123459","name":"work"}]"#;
const BIG_WORK: &str = r#"{"id":"1041234567890123459","owner_id":"9"}"#;
const LOST: &str = r#"{"message":"Unknown Guild","code":10004}"#;
const PICK: &str = "pick the server for the forum";
const MAKE: &str = "a private forum channel, #cornercase, is made for you";

fn two_servers_then(then: Routes) -> Routes {
    let mut routes = vec![
        ("GET /oauth2/applications/@me", 200, GUILD_INSTALL),
        ("GET /users/@me/guilds", 200, TWO),
        ("GET /guilds/7", 200, OURS),
        ("GET /guilds/8", 200, WORK),
    ];
    routes.extend(then);
    routes
}

fn with_the_mode_on(routes: Routes) -> Setup {
    let mut s = Setup::new(routes);
    s.app.config.discord_forum = true;
    s
}

fn asked_to_pick(then: Routes) -> Setup {
    let mut s = with_the_mode_on(two_servers_then(then));
    s.until("the servers are read", |app| !app.forum_view(Via::Discord).servers.is_empty());
    s
}

fn one_server_refusing_the_channel() -> Setup {
    let mut s = with_the_mode_on(vec![
        ("GET /oauth2/applications/@me", 200, GUILD_INSTALL),
        ("GET /users/@me/guilds", 200, HOME),
        ("GET /guilds/7", 200, OURS),
        ("GET /guilds/7/channels", 200, "[]"),
        ("POST /guilds/7/channels", 403, NO_PERMISSION),
    ]);
    s.until("the forum cannot be made", |app| app.forum_view(Via::Discord).problem.is_some());
    s
}

fn making_in_work() -> Routes {
    vec![("GET /guilds/8/channels", 200, "[]"), ("POST /guilds/8/channels", 201, MADE)]
}

fn pick(s: &mut Setup, id: &str) -> Value {
    ok(&mut s.app, "forum-server", &json!({ "id": id }))
}

fn refusal(s: &mut Setup, id: &str) -> (String, String) {
    let refused = refused(&mut s.app, "forum-server", &json!({ "id": id }));
    (refused.code, refused.message)
}

fn pending_of(s: &Setup) -> Option<String> {
    s.app.forum_info(Via::Discord).and_then(|forum| forum.pending)
}

fn forum_of(fed: &Snapshot<AppState>) -> ForumInfo {
    fed.report.away[1].forum.clone().expect("the forum")
}

fn listed_ids(s: &Setup) -> Vec<String> {
    let forum = s.app.forum_info(Via::Discord).unwrap_or_default();
    forum.servers.into_iter().map(|server| server.id).collect()
}

mod the_list {
    use super::*;

    #[test]
    fn while_the_pick_is_pending_shows_what_the_dialog_draws() {
        let mut s = asked_to_pick(Vec::new());

        let discord = discord_of(&mut s.app);

        assert_eq!(
            discord["forum"],
            json!({
                "on": true,
                "ready": false,
                "guild": null,
                "channel": null,
                "pending": PICK,
                "servers": [{"id": "7", "name": "home"}, {"id": "8", "name": "work"}],
            })
        );
        let names: Vec<&str> = discord["forum"]["servers"]
            .as_array()
            .expect("the servers")
            .iter()
            .filter_map(|server| server["name"].as_str())
            .collect();
        assert_eq!(s.app.forum_view(Via::Discord).servers, names);
    }

    #[test]
    fn keeps_ids_past_what_a_javascript_number_holds_as_they_are() {
        let mut s = with_the_mode_on(vec![
            ("GET /oauth2/applications/@me", 200, GUILD_INSTALL),
            ("GET /users/@me/guilds", 200, BIG),
            ("GET /guilds/1041234567890123457", 200, OURS),
            ("GET /guilds/1041234567890123459", 200, BIG_WORK),
            ("GET /guilds/1041234567890123459/channels", 200, "[]"),
            ("POST /guilds/1041234567890123459/channels", 201, MADE),
        ]);
        s.until("the servers are read", |app| !app.forum_view(Via::Discord).servers.is_empty());
        assert_eq!(listed_ids(&s), ["1041234567890123457", "1041234567890123459"]);

        let answer = pick(&mut s, "1041234567890123459");
        s.until("the forum is made", |app| app.messenger_settings(Via::Discord).forum_ready);

        assert_eq!(answer, json!({"id": "1041234567890123459", "name": "work"}));
        assert_eq!(discord_of(&mut s.app)["forum"]["guild"], "1041234567890123459");
    }

    #[test]
    fn is_left_out_with_one_server_since_that_one_is_picked_by_itself() {
        let mut s = one_server_refusing_the_channel();

        assert_eq!(discord_of(&mut s.app)["forum"].get("servers"), None);
        assert_eq!(pending_of(&s).as_deref(), Some(MAKE));
    }

    #[test]
    fn is_left_out_while_an_earlier_step_is_pending_though_the_dialog_draws_it() {
        let mut s = with_the_mode_on(vec![
            ("GET /oauth2/applications/@me", 200, NO_CONTENT),
            ("GET /users/@me/guilds", 200, TWO),
            ("GET /guilds/7", 200, OURS),
            ("GET /guilds/8", 200, WORK),
        ]);
        s.until("the servers are read", |app| !app.forum_view(Via::Discord).servers.is_empty());

        assert_eq!(discord_of(&mut s.app)["forum"].get("servers"), None);
        assert_eq!(pending_of(&s).as_deref(), Some("turn on Message Content Intent: Developer Portal › Bot"));
        assert_eq!(refusal(&mut s, "8").0, "conflict");
    }

    #[test]
    fn goes_with_the_paired_chat_as_nothing_is_read_without_one() {
        let mut s = asked_to_pick(Vec::new());

        s.app.forget_chat(Via::Discord);

        let forum = s.app.forum_info(Via::Discord).expect("the mode is on");
        assert_eq!((forum.pending, forum.servers), (None, Vec::new()));
    }

    #[test]
    fn goes_with_the_bot() {
        let mut s = asked_to_pick(Vec::new());

        s.app.remove_messenger(Via::Discord).expect("the bot is removed");

        let forum = s.app.forum_info(Via::Discord).expect("the mode is on");
        assert_eq!((forum.pending, forum.servers), (None, Vec::new()));
    }

    #[test]
    fn goes_with_the_mode() {
        let mut s = asked_to_pick(Vec::new());

        s.app.set_config(Config { discord_forum: false, ..s.app.config.clone() });

        assert_eq!(s.app.forum_info(Via::Discord), None);
    }

    #[test]
    fn moves_the_feed_when_it_comes_when_it_changes_and_when_it_goes() {
        let mut s = with_the_mode_on(two_servers_then(vec![
            ("GET /users/@me/guilds", 200, THREE),
            ("GET /guilds/6", 200, PLAY),
            ("GET /guilds/8/channels", 200, "[]"),
            ("POST /guilds/8/channels", 201, MADE),
        ]));
        let revision = follow(&mut s.app).revision;

        s.until("two servers are read", |app| !app.forum_view(Via::Discord).servers.is_empty());
        let fed = next(&mut s.app, revision);
        assert_eq!(forum_of(&fed).servers.len(), 2);

        s.until("the third is read", |app| app.forum_view(Via::Discord).servers.len() == 3);
        let fed = next(&mut s.app, fed.revision);
        assert_eq!(forum_of(&fed).servers.len(), 3);

        pick(&mut s, "8");
        let fed = next(&mut s.app, fed.revision);
        let picked = forum_of(&fed);
        assert_eq!((picked.servers, picked.pending.as_deref()), (Vec::new(), Some(MAKE)));

        s.until("the forum is made", |app| app.messenger_settings(Via::Discord).forum_ready);
        let fed = next(&mut s.app, fed.revision);
        let made = forum_of(&fed);
        assert_eq!((made.ready, made.guild.as_deref(), made.pending), (true, Some("8"), None));
    }
}

mod the_request {
    use super::*;

    #[test]
    fn makes_the_forum_in_that_server_with_the_toast_of_the_dialog() {
        let mut s = asked_to_pick(making_in_work());

        let answer = pick(&mut s, "8");
        s.until("the forum is made", |app| app.messenger_settings(Via::Discord).forum_ready);

        assert_eq!(answer, json!({"id": "8", "name": "work"}));
        assert_eq!(toast(&s.app), Some("Discord forum ready in work › #cornercase"));
        assert_eq!(
            discord_of(&mut s.app)["forum"],
            json!({"on": true, "ready": true, "guild": "8", "channel": "800", "pending": null})
        );
        assert!(s.saved().contains(r#""guild":8"#) && s.saved().contains(r#""channel":800"#), "{}", s.saved());
        assert!(!s.server.requests().iter().any(|r| r.starts_with("GET /guilds/7/channels")));
    }

    #[test]
    fn is_answered_once_the_pick_is_taken_and_sends_the_next_read_out_at_once() {
        let mut s = asked_to_pick(Vec::new());
        let Setup { app, rx, at, .. } = &mut s;
        wait_until("no read is out", || {
            drain(app, rx);
            app.next_forum_setup(*at).is_some()
        });
        assert!(s.app.next_forum_setup(s.at).is_some_and(|wait| !wait.is_zero()));

        pick(&mut s, "8");

        assert_eq!(s.app.next_forum_setup(s.at), Some(Duration::ZERO));
        assert_eq!(pending_of(&s).as_deref(), Some(MAKE));
        assert_eq!(listed_ids(&s), Vec::<String>::new());
    }

    #[test]
    fn moves_the_note_and_the_revision_of_the_forum_mode_row() {
        let mut s = asked_to_pick(Vec::new());
        let before = s.app.config_file.revision();
        assert_eq!(s.app.messenger_settings(Via::Discord).forum, PICK);

        pick(&mut s, "8");

        assert_eq!(s.app.messenger_settings(Via::Discord).forum, MAKE);
        assert_ne!(s.app.config_file.revision(), before);
    }

    #[test]
    fn closes_the_dialog_when_it_shows_as_a_click_does() {
        let mut s = asked_to_pick(making_in_work());
        s.app.open_forum(Via::Discord);
        s.until("the servers show", |app| !app.forum_view(Via::Discord).servers.is_empty());

        pick(&mut s, "8");
        s.until("the forum is made", |app| app.overlay.is_none());

        assert_eq!(toast(&s.app), Some("Discord forum ready in work › #cornercase"));
    }

    #[test]
    fn leaves_what_stops_the_forum_in_the_note_of_the_setting() {
        let mut s =
            asked_to_pick(vec![("GET /guilds/8/channels", 200, "[]"), ("POST /guilds/8/channels", 403, NO_PERMISSION)]);
        pick(&mut s, "8");
        s.until("the forum cannot be made", |app| app.forum_view(Via::Discord).problem.is_some());

        let described = ok(&mut s.app, "config", &json!({"action": "get"}));
        let settings = described["settings"].as_array().expect("the settings");
        let row = settings.iter().find(|setting| setting["key"] == "discord_forum").expect("the row");

        assert_eq!(row["note"], "missing permission: Manage Channels");
        assert_eq!(pending_of(&s).as_deref(), Some(MAKE));
        assert_eq!(listed_ids(&s), Vec::<String>::new());
    }

    #[test]
    fn asks_again_when_that_server_turns_out_gone() {
        let mut s = asked_to_pick(vec![
            ("GET /guilds/8/channels", 404, LOST),
            ("GET /guilds/7/channels", 200, "[]"),
            ("POST /guilds/7/channels", 201, MADE),
        ]);
        pick(&mut s, "8");

        s.until("the server is lost", |app| app.forum_view(Via::Discord).problem.is_some());
        assert_eq!(pending_of(&s).as_deref(), Some(PICK));
        assert_eq!(listed_ids(&s), ["7", "8"]);

        assert_eq!(pick(&mut s, "7")["name"], "home");
        s.until("the forum is made", |app| app.messenger_settings(Via::Discord).forum_ready);
        assert_eq!(toast(&s.app), Some("Discord forum ready in home › #cornercase"));
    }
}

mod a_refusal {
    use super::*;

    #[test]
    fn of_an_id_not_in_the_list_names_the_ones_that_are_and_changes_nothing() {
        let mut s = asked_to_pick(Vec::new());

        let (code, message) = refusal(&mut s, "9");

        assert_eq!(code, "not-found");
        assert_eq!(message, "`9` is not a server the forum can go in; they are: 7 (home), 8 (work)");
        assert_eq!(pending_of(&s).as_deref(), Some(PICK));
        assert_eq!(listed_ids(&s), ["7", "8"]);
    }

    #[test]
    fn says_there_is_no_bot() {
        let (mut app, _rx) = empty_app();

        let refused = refused(&mut app, "forum-server", &json!({"id": "8"}));

        assert_eq!(refused.code, "conflict");
        assert!(refused.message.contains("no Discord bot"), "{}", refused.message);
        assert!(refused.message.contains("config secret discord"), "{}", refused.message);
    }

    #[test]
    fn says_the_bot_is_not_paired() {
        let mut s = Setup::unpaired(Vec::new());
        s.app.config.discord_forum = true;

        let (code, message) = refusal(&mut s, "8");

        assert_eq!(code, "conflict");
        assert!(message.contains("not paired"), "{message}");
    }

    #[test]
    fn says_the_mode_is_off() {
        let mut s = asked_to_pick(Vec::new());
        s.app.set_config(Config { discord_forum: false, ..s.app.config.clone() });

        let (code, message) = refusal(&mut s, "8");

        assert_eq!(code, "conflict");
        assert!(message.contains("forum mode is off"), "{message}");
        assert!(message.contains("config set discord_forum true"), "{message}");
    }

    #[test]
    fn says_discord_was_not_read_yet() {
        let mut s = with_the_mode_on(Vec::new());

        let (code, message) = refusal(&mut s, "8");

        assert_eq!(code, "conflict");
        assert!(message.contains("was not read yet"), "{message}");
    }

    #[test]
    fn names_the_step_that_is_pending_instead() {
        let mut s = with_the_mode_on(vec![("GET /oauth2/applications/@me", 200, NO_GUILD_INSTALL)]);
        s.until("Guild Install is off", |app| app.messenger_settings(Via::Discord).forum.starts_with("turn on Guild"));

        let (code, message) = refusal(&mut s, "8");

        assert_eq!(code, "conflict");
        assert_eq!(
            message,
            "no server is waiting to be picked: turn on Guild Install: Developer Portal › Installation"
        );
    }

    #[test]
    fn says_what_failed_when_the_forum_cannot_be_made() {
        let mut s = one_server_refusing_the_channel();

        let (code, message) = refusal(&mut s, "7");

        assert_eq!(code, "conflict");
        assert_eq!(message, "no server is waiting to be picked: missing permission: Manage Channels");
    }

    #[test]
    fn says_the_pick_was_taken_when_it_comes_twice() {
        let mut s = asked_to_pick(Vec::new());
        pick(&mut s, "8");

        let (code, message) = refusal(&mut s, "8");

        assert_eq!(code, "conflict");
        assert_eq!(message, format!("no server is waiting to be picked: {MAKE}"));
    }

    #[test]
    fn names_the_server_of_a_forum_that_is_ready() {
        let mut s = with_the_mode_on(Vec::new());
        wiring::set_forum(&mut s.app, Via::Discord, Some(forum()));

        let (code, message) = refusal(&mut s, "8");

        assert_eq!(code, "conflict");
        assert_eq!(message, "no server is waiting to be picked: home › #cornercase");
    }

    #[test]
    fn of_an_id_that_is_a_number_says_the_request_is_malformed() {
        let mut s = asked_to_pick(Vec::new());

        let refused = refused(&mut s.app, "forum-server", &json!({"id": 8}));

        assert_eq!(refused.code, "malformed");
        assert_eq!(pending_of(&s).as_deref(), Some(PICK));
    }

    #[test]
    fn of_a_field_it_does_not_know_names_the_field() {
        let mut s = asked_to_pick(Vec::new());

        let refused = refused(&mut s.app, "forum-server", &json!({"id": "8", "name": "work"}));

        assert_eq!(refused.code, "unknown-field");
        assert!(refused.message.contains("args.name"), "{}", refused.message);
        assert_eq!(pending_of(&s).as_deref(), Some(PICK));
    }
}
