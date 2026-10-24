use super::*;
use crate::discord::forum::{Checklist, Progress, Server};
use crate::issues::Secret;

const OTHER_BOT: &str = r#"{"id":"123456789","username":"another"}"#;

fn made_without_the_window(routes: Vec<(&'static str, u16, &'static str)>) -> Setup {
    let mut s = Setup::new(routes);
    s.app.config.discord_forum = true;
    s
}

fn requests_to(s: &Setup, route: &str) -> usize {
    s.server.requests().iter().filter(|request| request.starts_with(route)).count()
}

fn a_forum_is_read(s: &mut Setup) {
    let home = Server { id: 7, name: "home".into() };
    let checklist = Checklist { guild_install: true, content: true, servers: vec![home] };
    let read = Ok(Progress { checklist, forum: Some(Ok(forum())) });
    let event = MessengerEvent::Forum { via: Via::Discord, token: Secret(TOKEN.into()), read };
    s.app.handle_event(AppEvent::Messenger(event), AREA).expect("the read is handled");
}

#[test]
fn with_one_server_the_forum_is_made_without_the_window() {
    let mut s = made_without_the_window(vec![
        ("GET /oauth2/applications/@me", 200, GUILD_INSTALL),
        ("GET /users/@me/guilds", 200, HOME),
        ("GET /guilds/7", 200, OURS),
        ("GET /guilds/7/channels", 200, "[]"),
        ("POST /guilds/7/channels", 201, MADE),
    ]);

    s.until("the forum is made", |app| app.messenger_settings(Via::Discord).forum_ready);

    assert!(s.app.overlay.is_none());
    assert_eq!(toast(&s.app), Some("Discord forum ready in home › #cornercase"));
    assert!(s.saved().contains(r#""channel":800"#));
}

#[test]
fn with_two_servers_the_pending_step_asks_to_pick_one_in_the_window() {
    let mut s = made_without_the_window(vec![
        ("GET /oauth2/applications/@me", 200, GUILD_INSTALL),
        ("GET /users/@me/guilds", 200, TWO),
        ("GET /guilds/7", 200, OURS),
        ("GET /guilds/8", 200, r#"{"id":"8","owner_id":"9"}"#),
    ]);

    s.until("the servers are read", |app| {
        app.messenger_settings(Via::Discord).forum == "pick the server for the forum"
    });

    assert_eq!(requests_to(&s, "POST "), 0);
    assert!(!s.app.messenger_settings(Via::Discord).forum_ready);
}

#[test]
fn outside_the_window_it_reads_every_thirty_seconds() {
    let mut s = made_without_the_window(vec![("GET /oauth2/applications/@me", 200, NO_GUILD_INSTALL)]);
    refreshing(&mut s.app, &s.rx, "the app is read", |app| {
        app.messenger_settings(Via::Discord).forum.starts_with("turn on Guild")
    });

    s.app.refresh(Instant::now() + Duration::from_secs(20));
    assert_eq!(requests_to(&s, "GET /oauth2/applications/@me"), 1);
    let later = Instant::now() + Duration::from_secs(31);
    let Setup { app, rx, server, .. } = &mut s;
    wait_until("it reads again", || {
        drain(app, rx);
        app.refresh(later);
        server.requests().len() == 2
    });
}

#[test]
fn a_read_that_changes_nothing_leaves_the_settings_revision_alone() {
    let mut s = Setup::opened(vec![("GET /oauth2/applications/@me", 200, NO_GUILD_INSTALL)]);
    s.until("the app is read", |app| app.messenger_settings(Via::Discord).forum.starts_with("turn on Guild"));
    let before = s.app.config_file.revision();

    let reads = s.server.requests().len();
    let Setup { app, rx, server, at, .. } = &mut s;
    wait_until("three more reads land", || {
        drain(app, rx);
        *at += Duration::from_secs(3);
        app.refresh(*at);
        server.requests().len() >= reads + 3
    });

    assert_eq!(s.app.config_file.revision(), before);
}

#[test]
fn a_forum_made_after_the_mode_was_turned_off_is_dropped_quietly() {
    let mut s = Setup::turned_on_in_the_settings();
    s.click(ui::ForumHit::TurnOff);

    a_forum_is_read(&mut s);

    assert_eq!(toast(&s.app), Some("forum mode is off: agents message the DM"));
    assert!(!s.app.messenger_settings(Via::Discord).forum_ready);
    assert!(!s.saved().contains("forum"));
}

#[test]
fn turning_the_mode_off_elsewhere_closes_the_dialog_and_drops_its_read() {
    let mut s = Setup::turned_on_in_the_settings();
    s.app.set_config(Config { discord_forum: false, ..s.app.config.clone() });
    assert!(s.app.overlay.is_none());

    a_forum_is_read(&mut s);

    assert_ne!(toast(&s.app), Some("Discord forum ready in home › #cornercase"));
    assert!(!s.app.messenger_settings(Via::Discord).forum_ready);
    assert!(!s.saved().contains("forum"));
}

#[test]
fn without_a_paired_chat_nothing_is_read_in_the_background() {
    let mut s = Setup::unpaired(vec![("GET /oauth2/applications/@me", 200, NO_GUILD_INSTALL)]);
    s.app.config.discord_forum = true;

    assert_eq!(s.app.next_forum_setup(Instant::now()), None);
    s.app.refresh(Instant::now());
    assert_eq!(requests_to(&s, "GET /oauth2/applications/@me"), 0);
}

#[test]
fn the_token_of_another_bot_forgets_the_forum_of_the_old_one() {
    let mut s = Setup::new(vec![("GET /users/@me", 200, OTHER_BOT)]);
    wiring::set_forum(&mut s.app, Via::Discord, Some(forum()));
    assert!(s.app.messenger_settings(Via::Discord).forum_ready);

    s.app.check_messenger(Via::Discord, Secret(TOKEN.into()));
    s.until("the other bot is set", |app| app.messenger(Via::Discord).bot() == Some("another"));

    assert!(!s.app.messenger_settings(Via::Discord).forum_ready);
    assert!(!s.saved().contains(r#""channel":800"#), "{}", s.saved());
}
