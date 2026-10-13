use rstest::rstest;

use super::shell_quote;

#[rstest]
#[case::plain("/home/me/wt-1.x", "/home/me/wt-1.x")]
#[case::a_space("/home/me/my wt", "'/home/me/my wt'")]
#[case::a_single_quote("/home/it's", "'/home/it'\\''s'")]
fn paths_are_quoted_for_a_shell(#[case] path: &str, #[case] expected: &str) {
    assert_eq!(shell_quote(path), expected);
}
