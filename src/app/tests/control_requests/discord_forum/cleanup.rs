use super::posts::{Posting, THREAD, first_post, sends};
use super::*;
use crate::control::AgentsReopen;

const DONE: i64 = 13;

fn cleaning(orders: &[Order]) -> Vec<Order> {
    orders
        .iter()
        .filter(|order| {
            matches!(order, Order::Retag { .. } | Order::Close { .. } | Order::Reopen { .. } | Order::Remove { .. })
        })
        .cloned()
        .collect()
}

fn locked() -> [Order; 3] {
    [
        Order::Reopen { thread: THREAD },
        Order::Retag { thread: THREAD, tag_id: DONE },
        Order::Close { thread: THREAD, lock: true },
    ]
}

fn closed(order: &Order) -> bool {
    matches!(order, Order::Close { .. } | Order::Remove { .. })
}

fn with_a_post_in(name: &str) -> (Posting, u64, u64) {
    let mut posting = Posting::new();
    let workspace = posting.workspace(name);
    let pane = posting.start(Some(workspace));
    let post = first_post(&mut posting, pane);
    posting.posted(&post);
    posting.sent();
    (posting, workspace, pane)
}

fn close_agents(posting: &mut Posting, workspace: u64) {
    ok(&mut posting.agent.app, "agents-close", &json!({"workspace": workspace, "force": true}));
}

fn with_its_agents_closed() -> (Posting, u64) {
    let (mut posting, workspace, _) = with_a_post_in("api");
    close_agents(&mut posting, workspace);
    posting.sent();
    (posting, workspace)
}

fn close(posting: &mut Posting, item: Item) {
    let close = wire::Close { item, remove_worktree: false, force: true, unlock: false };
    done(now(&mut posting.agent.app, None, Command::Close(close)));
}

pub(super) fn post_of(app: &mut App, workspace: u64) -> Value {
    let status = ok(app, "status", &json!({}));
    let projects = status["projects"].as_array().expect("the projects").clone();
    let found = projects.iter().flat_map(|project| project["workspaces"].as_array().cloned().unwrap_or_default());
    found.into_iter().find(|w| w["id"] == workspace).map(|w| w["post"].clone()).expect("the workspace")
}

mod closing_the_workspace {
    use super::*;

    #[test]
    fn reopens_its_post_in_case_discord_archived_it_then_tags_it_done_and_locks_it() {
        let (mut posting, workspace, _) = with_a_post_in("api");

        close(&mut posting, Item::Workspace(workspace));
        let got = posting.waits("the post is closed", closed);

        assert_eq!(cleaning(&got), locked());
        assert_eq!(cleaning(&posting.waits_a_while()), Vec::<Order>::new(), "the mapping is forgotten");
    }

    #[test]
    fn deletes_its_post_when_the_setting_says_delete() {
        let (mut posting, workspace, _) = with_a_post_in("api");
        posting.agent.app.config.discord_forum_close = "delete".into();

        close(&mut posting, Item::Workspace(workspace));
        let got = posting.waits("the post is removed", closed);

        assert_eq!(cleaning(&got), [Order::Remove { thread: THREAD }]);
    }

    #[test]
    fn by_closing_its_project_cleans_the_post_too() {
        let (mut posting, _, _) = with_a_post_in("api");
        let project = posting.agent.app.projects[0].id;

        close(&mut posting, Item::Project(project));
        let got = posting.waits("the post is closed", closed);

        assert_eq!(cleaning(&got).last(), Some(&Order::Close { thread: THREAD, lock: true }));
    }

    #[test]
    fn with_forum_mode_off_leaves_the_post_alone() {
        let (mut posting, workspace, _) = with_a_post_in("api");
        posting.agent.app.config.discord_forum = false;

        close(&mut posting, Item::Workspace(workspace));

        assert_eq!(cleaning(&posting.waits_a_while()), Vec::<Order>::new());
    }

    #[test]
    fn whose_post_was_deleted_by_hand_sends_nothing_more() {
        let (mut posting, workspace, _) = with_a_post_in("api");
        close(&mut posting, Item::Workspace(workspace));
        let got = posting.waits("the post is closed", closed);

        for order in cleaning(&got) {
            posting.gone(&order);
        }

        assert_eq!(posting.waits_a_while(), Vec::<Order>::new());
    }

    #[test]
    fn after_its_agents_were_closed_unarchives_the_post_to_lock_it() {
        let (mut posting, workspace) = with_its_agents_closed();

        close(&mut posting, Item::Workspace(workspace));
        let got = posting.waits("the post is closed", closed);

        assert_eq!(cleaning(&got), locked());
    }
}

mod closing_its_agents {
    use super::*;

    #[test]
    fn archives_the_post_without_locking_it() {
        let (mut posting, workspace, _) = with_a_post_in("api");

        close_agents(&mut posting, workspace);
        let got = posting.sent();

        assert_eq!(
            cleaning(&got),
            [Order::Retag { thread: THREAD, tag_id: DONE }, Order::Close { thread: THREAD, lock: false }]
        );
        assert_eq!(post_of(&mut posting.agent.app, workspace)["archived"], true);
        assert_eq!(cleaning(&posting.waits_a_while()), Vec::<Order>::new(), "no retag of an archived post");
    }

    #[test]
    fn and_reopening_them_unarchives_it_and_the_next_notice_goes_to_the_same_thread() {
        let (mut posting, workspace) = with_its_agents_closed();

        ask(&mut posting.agent.app, None, Command::AgentsReopen(AgentsReopen { workspace: Some(workspace) }));
        posting.agent.answered("the agents are reopened");
        let reopened = posting.sent();
        let pane = posting.start(Some(workspace));
        posting.report(pane, "blocked");
        let next = posting.waits("the next notice", |order| matches!(order, Order::Send { .. }));

        assert_eq!(cleaning(&reopened), [Order::Reopen { thread: THREAD }]);
        assert_eq!(post_of(&mut posting.agent.app, workspace)["archived"], false);
        assert!(sends(&next).iter().all(|(chat, _)| *chat == THREAD) && !sends(&next).is_empty(), "{next:?}");
    }

    #[test]
    fn with_forum_mode_off_sends_nothing() {
        let (mut posting, workspace, _) = with_a_post_in("api");
        posting.agent.app.config.discord_forum = false;

        close_agents(&mut posting, workspace);

        assert_eq!(cleaning(&posting.sent()), Vec::<Order>::new());
        assert_eq!(post_of(&mut posting.agent.app, workspace)["archived"], false);
    }
}

impl Posting {
    fn waits_a_while(&mut self) -> Vec<Order> {
        let mut got = Vec::new();
        for _ in 0..3 {
            self.at = (self.at + Duration::from_secs(5)).max(Instant::now());
            drain(&mut self.agent.app, &self.agent.rx);
            self.agent.app.refresh(self.at);
            got.extend(self.sent());
        }
        got
    }
}
