use rstest::rstest;

use super::*;

#[rstest]
#[case::empty("  ")]
#[case::yes("Yes.")]
#[case::go_on("  go on! ")]
#[case::spanish("sí")]
#[case::number("2")]
#[case::slash("/compact now")]
#[case::bang("!git status")]
fn noise_is_recognised(#[case] text: &str) {
    assert!(is_noise(text));
}

#[rstest]
#[case::sentence("add a rate limit to the public API")]
#[case::starts_with_go("go through the failing tests")]
fn real_prompts_are_kept(#[case] text: &str) {
    assert!(!is_noise(text));
}

#[test]
fn a_long_paste_becomes_its_first_line() {
    let text = format!("error: boom\n{}", "line\n".repeat(30));
    assert_eq!(fold_paste(&text), "error: boom (pasted 31 lines)");
    assert_eq!(fold_paste("short one"), "short one");
}

#[test]
fn a_text_is_flattened_and_cut_with_an_ellipsis() {
    assert_eq!(trim_to("a  b\nc", 10), "a b c");
    assert_eq!(trim_to("abcdefghij", 4), "abcd…");
}
