use super::*;
use crate::messenger::{Forum, Tags};
use crate::settings::Row;
use crate::test_util::FakeHttp;

const TOKEN: &str = "MTIzNDU2Nzg5.Gx.SECRET-token";
const NO_GUILD_INSTALL: &str = r#"{"id":"123456789","owner":{"id":"9"},"integration_types_config":{"1":{}}}"#;
const NO_CONTENT: &str = r#"{"id":"123456789","owner":{"id":"9"},"integration_types_config":{"0":{},"1":{}}}"#;
const GUILD_INSTALL: &str =
    r#"{"id":"123456789","owner":{"id":"9"},"integration_types_config":{"0":{},"1":{}},"flags":262144}"#;
const HOME: &str = r#"[{"id":"7","name":"home"}]"#;
const TWO: &str = r#"[{"id":"7","name":"home"},{"id":"8","name":"work"}]"#;
const OURS: &str = r#"{"id":"7","owner_id":"9"}"#;
const MADE: &str = r#"{"id":"800","type":15,"name":"cornercase","available_tags":[
    {"id":"11","name":"working"},{"id":"12","name":"needs you"},{"id":"13","name":"done"}]}"#;
const NO_PERMISSION: &str = r#"{"message":"Missing Permissions","code":50013}"#;

struct Setup {
    dir: TempDir,
    app: App,
    rx: Receiver<AppEvent>,
    server: FakeHttp,
    at: Instant,
    _orders: Receiver<Order>,
}

impl Setup {
    fn new(routes: Vec<(&'static str, u16, &'static str)>) -> Self {
        Self::with_chat(routes, Some(chat()))
    }

    fn unpaired(routes: Vec<(&'static str, u16, &'static str)>) -> Self {
        Self::with_chat(routes, None)
    }

    fn with_chat(routes: Vec<(&'static str, u16, &'static str)>, chat: Option<Chat>) -> Self {
        let dir = TempDir::new();
        let (mut app, rx) = new_app(dir.path().join("config.json"));
        let mut routes = routes;
        routes.push(("GET /gateway/bot", 429, r#"{"retry_after":60}"#));
        let server = FakeHttp::start(routes);
        wiring::set_up(&mut app, Via::Discord, TOKEN, "cornercase", chat);
        wiring::point(&mut app, Via::Discord, &server.url());
        let orders = wiring::capture(&mut app, Via::Discord);
        Self { dir, app, rx, server, at: Instant::now(), _orders: orders }
    }

    fn turned_on_in_the_settings() -> Self {
        let mut s = Self::new(Vec::new());
        s.app.open_settings_on(&Row::Forum(Via::Discord));
        send_key(&mut s.app, KeyCode::Enter, KeyModifiers::NONE);
        s
    }

    fn opened(routes: Vec<(&'static str, u16, &'static str)>) -> Self {
        let mut s = Self::new(routes);
        s.app.open_forum(Via::Discord);
        s
    }

    fn until(&mut self, what: &str, cond: impl Fn(&App) -> bool) {
        let Self { app, rx, at, .. } = self;
        wait_until(what, || {
            drain(app, rx);
            *at = (*at + Duration::from_secs(3)).max(Instant::now());
            app.refresh(*at);
            cond(app)
        });
    }

    fn click(&mut self, hit: ui::ForumHit) {
        let view = self.app.forum_view(Via::Discord);
        let pos = (0..AREA.height)
            .flat_map(|y| (0..AREA.width).map(move |x| Position::new(x, y)))
            .find(|pos| ui::forum_hit(AREA, &view, *pos) == Some(hit))
            .unwrap_or_else(|| panic!("no {hit:?} in the dialog"));
        click(&mut self.app, pos);
    }

    fn saved(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("discord.json")).unwrap_or_default()
    }
}

fn forum() -> Forum {
    Forum { guild: 7, guild_name: "home".into(), channel: 800, tags: Tags { working: 11, waiting: 12, done: 13 } }
}

fn steps_of(app: &App) -> Vec<bool> {
    app.forum_view(Via::Discord).steps.iter().map(|(done, _)| *done).collect()
}

mod setup {
    use super::*;

    #[test]
    fn turning_forum_mode_on_in_the_settings_opens_the_dialog() {
        let s = Setup::turned_on_in_the_settings();

        assert!(matches!(s.app.overlay, Some(Overlay::Forum(Via::Discord))));
        assert!(s.app.config.discord_forum);
    }

    #[test]
    fn the_steps_tick_one_by_one_and_the_forum_is_made_in_the_owners_server() {
        let mut s = Setup::opened(vec![
            ("GET /oauth2/applications/@me", 200, NO_GUILD_INSTALL),
            ("GET /oauth2/applications/@me", 200, NO_CONTENT),
            ("GET /oauth2/applications/@me", 200, GUILD_INSTALL),
            ("GET /users/@me/guilds", 200, "[]"),
            ("GET /users/@me/guilds", 200, "[]"),
            ("GET /users/@me/guilds", 200, TWO),
            ("GET /guilds/7", 200, OURS),
            ("GET /guilds/8", 200, r#"{"id":"8","owner_id":"5"}"#),
            ("GET /guilds/7/channels", 200, "[]"),
            ("POST /guilds/7/channels", 201, MADE),
        ]);
        s.until("Guild Install is off", |app| app.messenger_settings(Via::Discord).forum.starts_with("turn on Guild"));
        assert_eq!(steps_of(&s.app), [false, false, false, false]);
        assert_eq!(s.app.forum_view(Via::Discord).url, None);

        s.until("the intent is off", |app| steps_of(app) == [true, false, false, false]);
        assert_eq!(
            s.app.messenger_settings(Via::Discord).forum,
            "turn on Message Content Intent: Developer Portal › Bot"
        );

        s.until("no server has the app", |app| steps_of(app) == [true, true, false, false]);
        let view = s.app.forum_view(Via::Discord);
        let url = view.url.unwrap_or_default();
        assert!(url.contains("integration_type=0"), "{url}");
        assert!(view.notes.iter().any(|note| note.starts_with("no server yet?")), "{:?}", view.notes);
        assert_eq!(s.app.messenger_settings(Via::Discord).forum, "add the app to your server");

        s.until("the forum is made", |app| app.overlay.is_none());
        assert_eq!(toast(&s.app), Some("Discord forum ready in home › #cornercase"));
        let saved = s.saved();
        assert!(saved.contains(r#""channel":800"#) && saved.contains(r#""waiting":12"#), "{saved}");
        let settings = s.app.messenger_settings(Via::Discord);
        assert_eq!((settings.forum.as_str(), settings.forum_ready), ("home › #cornercase", true));
    }

    #[test]
    fn a_missing_permission_keeps_the_dialog_open_with_the_link() {
        let mut s = Setup::opened(vec![
            ("GET /oauth2/applications/@me", 200, GUILD_INSTALL),
            ("GET /users/@me/guilds", 200, HOME),
            ("GET /guilds/7", 200, OURS),
            ("GET /guilds/7/channels", 200, "[]"),
            ("POST /guilds/7/channels", 403, NO_PERMISSION),
        ]);
        s.until("the forum cannot be made", |app| app.forum_view(Via::Discord).problem.is_some());

        let view = s.app.forum_view(Via::Discord);
        assert_eq!(view.problem.as_deref(), Some("missing permission: Manage Channels"));
        assert!(view.url.is_some_and(|url| url.contains("permissions=")));
        assert_eq!(steps_of(&s.app), [true, true, true, false]);
        assert!(matches!(s.app.overlay, Some(Overlay::Forum(Via::Discord))));
        assert!(!s.saved().contains("forum"));
    }

    #[test]
    fn with_two_servers_a_click_picks_the_one_for_the_forum() {
        let mut s = Setup::opened(vec![
            ("GET /oauth2/applications/@me", 200, GUILD_INSTALL),
            ("GET /users/@me/guilds", 200, TWO),
            ("GET /guilds/7", 200, OURS),
            ("GET /guilds/8", 200, r#"{"id":"8","owner_id":"9"}"#),
            ("GET /guilds/8/channels", 200, "[]"),
            ("POST /guilds/8/channels", 201, MADE),
        ]);
        s.until("both servers are listed", |app| app.forum_view(Via::Discord).servers.len() == 2);
        assert_eq!(s.app.forum_view(Via::Discord).servers, ["home", "work"]);
        assert_eq!(s.app.messenger_settings(Via::Discord).forum, "pick the server for the forum");

        s.click(ui::ForumHit::Server(1));
        s.until("the forum is made", |app| app.overlay.is_none());

        assert_eq!(toast(&s.app), Some("Discord forum ready in work › #cornercase"));
        assert!(!s.server.requests().iter().any(|r| r.starts_with("GET /guilds/7/channels")));
    }

    #[test]
    fn done_closes_the_dialog_with_the_mode_still_on_and_asks_only_in_the_background() {
        let mut s = Setup::opened(vec![("GET /oauth2/applications/@me", 200, NO_GUILD_INSTALL)]);
        s.app.config.discord_forum = true;
        refreshing(&mut s.app, &s.rx, "the app is read", |app| {
            app.forum_view(Via::Discord).steps[0].1.starts_with("turn on")
        });
        s.click(ui::ForumHit::Done);
        s.app.refresh(Instant::now() + Duration::from_secs(20));

        assert!(s.app.overlay.is_none() && s.app.config.discord_forum);
        assert_eq!(s.app.tick(Instant::now()).filter(Duration::is_zero), None);
        assert_eq!(s.server.requests().len(), 1);
    }

    #[test]
    fn turn_off_closes_the_dialog_and_turns_the_mode_off() {
        let mut s = Setup::turned_on_in_the_settings();
        s.click(ui::ForumHit::TurnOff);

        assert!(s.app.overlay.is_none());
        assert!(!s.app.config.discord_forum);
        assert_eq!(toast(&s.app), Some("forum mode is off: agents message the DM"));
    }

    #[test]
    fn escape_closes_the_dialog() {
        let mut s = Setup::opened(Vec::new());
        send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);

        assert!(s.app.overlay.is_none());
    }
}

mod background;
mod buttons;
mod cleanup;
mod gone;
mod posts;
mod replies;
mod socket;
mod started;
mod tags;
