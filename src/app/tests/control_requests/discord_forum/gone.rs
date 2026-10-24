use super::cleanup::post_of;
use super::posts::{Posting, THREAD, needs_you, posts, sends};
use super::{tags, *};

fn with_a_post() -> (Posting, u64, u64) {
    let (posting, pane) = tags::with_a_post();
    let at = posting.agent.app.projects.tab_of_pane(pane).expect("the agent's tab").workspace();
    let workspace = posting.agent.app.projects.workspace(at).id;
    (posting, pane, workspace)
}

fn notices(posting: &mut Posting, pane: u64) -> Vec<Order> {
    posting.asks_again(pane);
    posting.sent().into_iter().filter(needs_you).collect()
}

fn text_of(order: &Order) -> &str {
    match order {
        Order::Send { text, .. } | Order::Post { text, .. } => text,
        other => panic!("no text in {other:?}"),
    }
}

#[test]
fn a_retag_of_a_post_deleted_by_hand_forgets_the_post_and_sends_nothing_more() {
    let (mut posting, pane, workspace) = with_a_post();
    posting.report(pane, "working");
    let retag = posting.sent().into_iter().find(|order| matches!(order, Order::Retag { .. })).expect("a retag");

    posting.gone(&retag);

    assert_eq!(post_of(&mut posting.agent.app, workspace), Value::Null);
    assert_eq!(posting.sent(), Vec::<Order>::new());
    let next = notices(&mut posting, pane);
    assert!(matches!(&next[..], [Order::Post { .. }]), "the next notice makes a new post: {next:?}");
}

#[test]
fn a_notice_sent_to_a_post_deleted_by_hand_makes_a_new_post_with_it() {
    let (mut posting, pane, _) = with_a_post();
    let sent = notices(&mut posting, pane);
    assert_eq!(sends(&sent).first().map(|(chat, _)| *chat), Some(THREAD), "{sent:?}");

    posting.gone(&sent[0]);
    let again = posting.sent();

    let made = posts(&again);
    assert_eq!(made.len(), 1, "{again:?}");
    assert!(sends(&again).is_empty(), "{again:?}");
    assert_eq!((made[0].tag(), text_of(made[0])), (sent[0].tag(), text_of(&sent[0])));
}

#[test]
fn a_new_post_that_cannot_be_made_either_sends_the_notice_to_the_dm() {
    let (mut posting, pane, _) = with_a_post();
    let sent = notices(&mut posting, pane);
    posting.gone(&sent[0]);
    let post = posts(&posting.sent())[0].clone();

    posting.failed(&post);
    let dm = posting.sent();

    assert_eq!(sends(&dm), [(CHAT, text_of(&sent[0]))]);
}

#[test]
fn a_notice_that_finds_the_new_post_gone_too_goes_to_the_dm() {
    let (mut posting, pane, _) = with_a_post();
    let first = notices(&mut posting, pane);
    let second = notices(&mut posting, pane);
    posting.gone(&first[0]);
    posting.gone(&second[0]);
    let post = posts(&posting.sent())[0].clone();
    posting.posted(&post);
    let followed = posting.sent();
    assert_eq!(sends(&followed), [(THREAD, text_of(&second[0]))], "the second waits for the new post");

    posting.gone(&followed[0]);
    let dm = posting.sent();

    assert_eq!(sends(&dm), [(CHAT, text_of(&second[0]))]);
    assert!(posts(&dm).is_empty(), "{dm:?}");
}
