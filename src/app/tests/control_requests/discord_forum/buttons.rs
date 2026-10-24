use super::posts::{Posting, THREAD, first_post, posts, sends};
use super::*;
use crate::messenger::Tap;

const STRANGER: i64 = 66;
const UNKNOWN: i64 = 901;

fn in_its_post() -> (Posting, u64) {
    let mut posting = Posting::new();
    let pane = posting.start(None);
    let post = first_post(&mut posting, pane);
    posting.posted(&post);
    posting.sent();
    (posting, pane)
}

fn press(posting: &mut Posting, (chat, user): (i64, i64), tap: &Tap) {
    let data = tap.data(wiring::run(&posting.agent.app, Via::Discord));
    let press = Press { id: format!("q{chat}"), chat, user, message: 5000, data };
    update(&mut posting.agent.app, Via::Discord, Update::Press(press));
}

fn answered(posting: &mut Posting, what: &str) -> Vec<Order> {
    let mut got = Vec::new();
    wait_until(what, || {
        got.extend(posting.sent());
        got.iter().any(|order| matches!(order, Order::Answer { text, .. } if !text.is_empty()))
    });
    got
}

#[test]
fn the_owners_key_in_the_post_reaches_the_pane_and_its_buttons_go_away_there() {
    let (mut posting, pane) = in_its_post();

    press(&mut posting, (THREAD, USER), &Tap::Key(pane, "1".into()));
    let got = answered(&mut posting, "the key is sent");

    assert!(
        got.iter().any(|order| matches!(order, Order::Answer { text, .. } if text == "sent 1 to claude")),
        "{got:?}"
    );
    let cleared = got.iter().find_map(|order| match order {
        Order::Buttons { chat, message, .. } => Some((*chat, *message)),
        _ => None,
    });
    assert_eq!(cleared, Some((THREAD, 5000)), "{got:?}");
}

#[test]
fn the_screen_button_in_the_post_answers_in_the_post() {
    let (mut posting, pane) = in_its_post();

    press(&mut posting, (THREAD, USER), &Tap::Screen(pane));
    let got = posting.sent();

    let chats: Vec<i64> = sends(&got).iter().map(|(chat, _)| *chat).collect();
    assert_eq!(chats, [THREAD], "{got:?}");
    assert!(sends(&got)[0].1.contains("claude"), "{got:?}");
}

#[test]
fn the_write_button_in_the_post_asks_in_the_post() {
    let (mut posting, pane) = in_its_post();

    press(&mut posting, (THREAD, USER), &Tap::Write(pane));
    let got = posting.sent();

    let asked = got.iter().find_map(|order| match order {
        Order::Send { chat, ask: Some(_), .. } => Some(*chat),
        _ => None,
    });
    assert_eq!(asked, Some(THREAD), "{got:?}");
}

#[test]
fn the_screen_button_of_status_in_the_dm_answers_in_the_dm() {
    let mut posting = Posting::new();
    let pane = posting.start(None);

    press(&mut posting, (CHAT, USER), &Tap::Screen(pane));
    let got = posting.sent();

    assert!(posts(&got).is_empty(), "{got:?}");
    let chats: Vec<i64> = sends(&got).iter().map(|(chat, _)| *chat).collect();
    assert_eq!(chats, [CHAT], "{got:?}");
}

#[test]
fn a_press_by_someone_else_in_the_post_is_ignored() {
    let (mut posting, pane) = in_its_post();

    press(&mut posting, (THREAD, STRANGER), &Tap::Screen(pane));
    press(&mut posting, (THREAD, STRANGER), &Tap::Key(pane, "esc".into()));

    assert_eq!(posting.sent(), Vec::new());
}

#[test]
fn a_press_by_the_owner_in_a_thread_cornercase_did_not_make_is_ignored() {
    let (mut posting, pane) = in_its_post();

    press(&mut posting, (UNKNOWN, USER), &Tap::Screen(pane));
    press(&mut posting, (UNKNOWN, USER), &Tap::Key(pane, "esc".into()));

    assert_eq!(posting.sent(), Vec::new());
}
