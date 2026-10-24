use super::posts::{Posting, THREAD, first_post, sends};
use super::*;
use crate::app::messenger::By;

const WORKING: i64 = 11;
const NEEDS_YOU: i64 = 12;
const DONE: i64 = 13;

fn retags(orders: &[Order]) -> Vec<(i64, i64)> {
    orders
        .iter()
        .filter_map(|order| match order {
            Order::Retag { thread, tag_id } => Some((*thread, *tag_id)),
            _ => None,
        })
        .collect()
}

pub(super) fn with_a_post() -> (Posting, u64) {
    let mut posting = Posting::new();
    let pane = posting.start(None);
    let post = first_post(&mut posting, pane);
    posting.posted(&post);
    posting.sent();
    (posting, pane)
}

impl Posting {
    fn retags_after(&mut self, pane: u64, state: &str) -> Vec<(i64, i64)> {
        self.report(pane, state);
        let sent = self.sent();
        retags(&sent)
    }

    fn briefly(&mut self, pane: u64, state: &str) {
        let term = self.agent.app.projects.pane_by_mut(pane).expect("the agent's pane");
        term.feed(format!("\x1b]7501;state={state}\x07").as_bytes());
        self.at = self.at.max(Instant::now()) + Duration::from_millis(600);
        drain(&mut self.agent.app, &self.agent.rx);
        self.agent.app.refresh(self.at);
    }
}

#[test]
fn the_tag_follows_the_agent_with_one_order_for_each_change() {
    let (mut posting, pane) = with_a_post();

    assert_eq!(posting.retags_after(pane, "working"), [(THREAD, WORKING)]);
    assert_eq!(posting.retags_after(pane, "working"), []);
    assert_eq!(posting.retags_after(pane, "blocked"), [(THREAD, NEEDS_YOU)]);
    assert_eq!(posting.retags_after(pane, "done"), [(THREAD, DONE)]);
    assert_eq!(posting.retags_after(pane, "idle"), []);
}

#[test]
fn an_agent_waiting_outranks_one_working_in_the_same_workspace() {
    let (mut posting, waiting) = with_a_post();
    let working = posting.start(None);

    assert_eq!(posting.retags_after(working, "working"), []);
    assert_eq!(posting.retags_after(waiting, "working"), [(THREAD, WORKING)]);
    assert_eq!(posting.retags_after(waiting, "blocked"), [(THREAD, NEEDS_YOU)]);
}

#[test]
fn a_change_undone_within_a_second_sends_nothing() {
    let (mut posting, pane) = with_a_post();

    posting.briefly(pane, "working");
    posting.briefly(pane, "blocked");
    posting.briefly(pane, "blocked");
    posting.briefly(pane, "blocked");

    assert_eq!(retags(&posting.sent()), []);
}

#[test]
fn the_tag_changes_without_away_too() {
    let (mut posting, pane) = with_a_post();
    posting.agent.app.come_back(Via::Discord, By::Command);
    posting.sent();

    assert_eq!(posting.retags_after(pane, "working"), [(THREAD, WORKING)]);
}

#[test]
fn a_post_discord_archived_is_marked_archived_and_retagged_only_after_a_message_reopens_it() {
    let (mut posting, pane) = with_a_post();
    assert_eq!(posting.retags_after(pane, "working"), [(THREAD, WORKING)]);
    assert_eq!(posting.retags_after(pane, "blocked"), [(THREAD, NEEDS_YOU)]);

    posting.link(LinkEvent::Archived { link: 1, thread: THREAD });

    assert_eq!(posting.retags_after(pane, "done"), []);
    posting.asks_again(pane);
    let reopened = posting.sent();
    assert!(sends(&reopened).iter().any(|(chat, _)| *chat == THREAD), "{reopened:?}");
    assert_eq!(posting.retags_after(pane, "working"), [(THREAD, WORKING)]);
}

#[test]
fn a_post_of_another_workspace_keeps_its_own_tag() {
    let (mut posting, pane) = with_a_post();
    let api = posting.workspace("api");
    let other = posting.start(Some(api));

    assert_eq!(posting.retags_after(other, "working"), []);
    assert_eq!(posting.retags_after(pane, "working"), [(THREAD, WORKING)]);
}
