use rstest::rstest;

use super::*;

#[rstest]
#[case::one(1, Some(40), "1 message waiting from pane 40")]
#[case::one_from_a_script(1, None, "1 message waiting")]
#[case::several(3, Some(40), "3 messages waiting, the first from pane 40")]
#[case::several_from_a_script(2, None, "2 messages waiting")]
fn the_band_counts_the_messages_and_names_the_first_sender(
    #[case] count: usize,
    #[case] from: Option<u64>,
    #[case] expected: &str,
) {
    assert_eq!(label(count, from), expected);
}

#[test]
fn a_timeout_says_what_held_the_message() {
    assert!(waiting(4, Some(Why::Draft)).starts_with("the person in pane 4 has a draft"));
    assert!(waiting(4, Some(Why::Typing)).contains("typed in the last 2s"));
    assert!(waiting(4, None).starts_with("an earlier message to pane 4"));
}
