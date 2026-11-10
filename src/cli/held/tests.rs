use rstest::rstest;

use super::*;
use crate::cli::args;
use crate::cli::tests::parse;

fn parsed(args: &[&str]) -> HeldArgs {
    match parse(args).expect("parse").command {
        Some(args::Command::Control(args::Control::Held(held))) => held,
        other => panic!("not held: {other:?}"),
    }
}

#[test]
fn each_button_is_its_own_flag() {
    let now = parsed(&["held", "--pane", "9", "--send-now"]);
    let wait = parsed(&["held", "--tab", "4", "--wait-turn", "--json"]);

    assert_eq!(
        (now.target.pane, now.button.send_now, now.button.wait_turn, now.print.json),
        (Some(9), true, false, false)
    );
    assert_eq!(
        (wait.target.tab, wait.button.send_now, wait.button.wait_turn, wait.print.json),
        (Some(4), false, true, true)
    );
}

#[rstest]
#[case::neither(&["held", "--pane", "9"])]
#[case::both(&["held", "--pane", "9", "--send-now", "--wait-turn"])]
#[case::a_pane_and_a_tab(&["held", "--pane", "9", "--tab", "4", "--send-now"])]
fn exactly_one_button_and_one_target_are_taken(#[case] args: &[&str]) {
    assert!(parse(args).is_err(), "{args:?}");
}

#[test]
fn it_acts_in_a_view() {
    let cli = parse(&["held", "--view", "41", "--wait-turn"]).expect("parse");
    let Some(args::Command::Control(control)) = cli.command else { panic!("not a control command") };

    assert_eq!(control.view(), Some(41));
}

#[rstest]
#[case::nothing_left(0, false, false, "nothing is held in pane 9 now\n")]
#[case::one(1, false, false, "1 message held in pane 9\n")]
#[case::several(3, false, false, "3 messages held in pane 9\n")]
#[case::queued(2, true, false, "2 messages held in pane 9, queued to go right after the person's\n")]
#[case::pasted_after_the_draft(
    0,
    false,
    true,
    "the message is in pane 9 after the person's draft, not sent: their Enter sends both\nnothing is held in pane 9 now\n"
)]
fn the_text_says_what_is_held_now(
    #[case] held_sends: u32,
    #[case] held_queued: bool,
    #[case] after_draft: bool,
    #[case] expected: &str,
) {
    let held = HeldInfo { pane: 9, held_sends, held_from: Some(40), held_queued };
    let held = HeldAnswer { held, after_draft };

    assert_eq!(text(&held), expected);
}
