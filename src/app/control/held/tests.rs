use rstest::rstest;

use super::*;

#[rstest]
#[case::one(1, Some(40), "1 message waiting from pane 40 · goes after yours")]
#[case::one_from_a_script(1, None, "1 message waiting · goes after yours")]
#[case::several(3, Some(40), "3 messages waiting, the first from pane 40 · goes after yours")]
#[case::several_from_a_script(2, None, "2 messages waiting · goes after yours")]
fn the_band_counts_the_messages_and_names_the_first_sender(
    #[case] count: usize,
    #[case] from: Option<u64>,
    #[case] expected: &str,
) {
    assert_eq!(label(count, from, false), expected);
}

#[test]
fn an_acknowledged_message_says_it_goes_right_after_the_persons_prompt() {
    assert_eq!(label(1, Some(40), true), "queued: goes right after yours");
    assert_eq!(label(2, Some(40), true), "2 queued: the first goes right after yours");
}

#[test]
fn a_timeout_says_what_held_the_message() {
    assert!(waiting(4, Some(Why::Draft)).starts_with("the person in pane 4 has a draft"));
    assert!(waiting(4, Some(Why::Typing)).contains("typed in the last 2s"));
    assert!(waiting(4, None).starts_with("an earlier message to pane 4"));
}
