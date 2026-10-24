use super::*;
use crate::state::{DiscordPost, State};

fn saved_with_a_post() -> (Posting, State) {
    let mut posting = Posting::new();
    let pane = posting.start(None);
    let post = first_post(&mut posting, pane);
    posting.posted(&post);
    let saved = posting.agent.app.state();
    (posting, saved)
}

fn restored(saved: &State, channel: i64) -> (Posting, u64) {
    let mut again = Posting::new();
    again.agent.app.config.resume_agents = false;
    let forum =
        Forum { guild: 7, guild_name: "home".into(), channel, tags: Tags { working: 11, waiting: WAITING, done: 13 } };
    wiring::set_forum(&mut again.agent.app, Via::Discord, Some(forum));
    assert!(again.agent.app.restore(saved, AREA));
    let workspace = again.agent.app.projects.last().expect("the restored project").workspaces[0].id;
    (again, workspace)
}

#[test]
fn a_workspace_saves_the_thread_of_its_post_with_the_forum() {
    let (_posting, saved) = saved_with_a_post();

    let posts: Vec<DiscordPost> =
        saved.projects.iter().flat_map(|p| &p.workspaces).filter_map(|w| w.discord_post).collect();
    assert_eq!(posts, [DiscordPost { channel: FORUM, thread: THREAD, archived: false }]);
}

#[test]
fn after_a_restart_the_next_notice_goes_to_the_same_thread() {
    let (_posting, saved) = saved_with_a_post();
    let (mut again, workspace) = restored(&saved, FORUM);
    let pane = again.start(Some(workspace));

    again.report(pane, "blocked");
    let got = again.waits("the notice after the restart", needs_you);

    assert!(posts(&got).is_empty(), "{got:?}");
    assert!(matches!(sends(&got)[..], [(THREAD, text)] if text.starts_with("**! claude needs you**")), "{got:?}");
}

#[test]
fn a_post_saved_in_another_forum_is_dropped_and_a_new_one_is_made() {
    let (_posting, saved) = saved_with_a_post();
    let (mut again, workspace) = restored(&saved, FORUM + 1);
    let pane = again.start(Some(workspace));

    let post = first_post(&mut again, pane);

    assert!(matches!(post, Order::Post { forum, .. } if forum == FORUM + 1), "{post:?}");
    assert_eq!(again.agent.app.state().projects.iter().flat_map(|p| &p.workspaces).find_map(|w| w.discord_post), None);
}
