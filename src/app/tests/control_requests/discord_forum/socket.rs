use super::posts::{Posting, THREAD, first_post};
use super::*;
use crate::control::{AwayInfo, ForumInfo, WorkspaceInfo};

fn discord_of(app: &mut App) -> Value {
    let answer = value(app, None, Command::App(wire::AppWatch::default()));
    let away = answer["away"].as_array().expect("the platforms").clone();
    away.into_iter().find(|platform| platform["platform"] == "discord").expect("discord")
}

fn workspaces(app: &mut App) -> Vec<Value> {
    let status = ok(app, "status", &json!({}));
    status["projects"][0]["workspaces"].as_array().expect("the workspaces").clone()
}

#[test]
fn without_forum_mode_neither_the_forum_nor_a_post_is_in_the_answers() {
    let mut posting = Posting::new();
    let app = &mut posting.agent.app;
    app.config.discord_forum = false;
    wiring::set_forum(app, Via::Discord, None);

    let discord = discord_of(app);
    let telegram = app.app_state().away.into_iter().find(|platform| platform.platform == "telegram");
    let workspaces = workspaces(app);

    assert_eq!(discord.get("forum"), None, "{discord}");
    assert_eq!(telegram.and_then(|platform| platform.forum), None);
    assert!(!workspaces.is_empty() && workspaces.iter().all(|workspace| workspace.get("post").is_none()));
}

#[test]
fn a_forum_still_being_set_up_says_its_first_missing_step() {
    let mut s = Setup::opened(vec![("GET /oauth2/applications/@me", 200, NO_GUILD_INSTALL)]);
    s.app.config.discord_forum = true;
    s.until("Guild Install is off", |app| app.messenger_settings(Via::Discord).forum.starts_with("turn on Guild"));

    let discord = discord_of(&mut s.app);

    assert_eq!(
        discord["forum"],
        json!({
            "on": true,
            "ready": false,
            "guild": null,
            "channel": null,
            "pending": "turn on Guild Install: Developer Portal › Installation",
        })
    );
}

#[test]
fn a_ready_forum_names_its_server_and_channel_and_a_workspace_links_its_post() {
    let mut posting = Posting::new();
    let pane = posting.start(None);
    let post = first_post(&mut posting, pane);
    posting.posted(&post);
    let other = posting.workspace("api");

    let discord = discord_of(&mut posting.agent.app);
    let workspaces = workspaces(&mut posting.agent.app);

    assert_eq!(discord["forum"], json!({"on": true, "ready": true, "guild": "7", "channel": "800", "pending": null}));
    let url = format!("https://discord.com/channels/7/{THREAD}");
    assert_eq!(workspaces[0]["post"], json!({ "url": url, "archived": false }));
    let api = workspaces.iter().find(|workspace| workspace["id"] == other).expect("the api workspace");
    assert_eq!(api.get("post"), None, "{api}");
}

#[test]
fn a_new_post_changes_the_watched_status() {
    let mut posting = Posting::new();
    let pane = posting.start(None);
    let post = first_post(&mut posting, pane);
    let before = posting.agent.app.refreshed();

    posting.posted(&post);

    assert_ne!(posting.agent.app.refreshed(), before);
}

#[test]
fn turned_off_with_a_forum_kept_it_still_shows_as_off() {
    let mut posting = Posting::new();
    posting.agent.app.config.discord_forum = false;

    let forum = posting.agent.app.forum_info(Via::Discord).expect("the kept forum");

    assert_eq!((forum.on, forum.ready, forum.channel.as_deref()), (false, true, Some("800")));
}

mod an_older_answer {
    use super::*;

    #[test]
    fn reads_without_a_forum_or_a_post() {
        let away: AwayInfo = serde_json::from_str(r#"{"platform":"discord","ready":true,"away":false}"#).expect("away");
        let workspace: WorkspaceInfo = serde_json::from_str(r#"{"id":3,"name":"main"}"#).expect("a workspace");

        assert_eq!((away.forum, workspace.post), (None, None));
    }

    #[test]
    fn a_forum_with_fields_it_does_not_know_still_reads() {
        let text = r#"{"platform":"discord","forum":{"on":true,"ready":false,"later":1}}"#;

        let away: AwayInfo = serde_json::from_str(text).expect("away");

        assert_eq!(away.forum, Some(ForumInfo { on: true, ..ForumInfo::default() }));
    }
}
