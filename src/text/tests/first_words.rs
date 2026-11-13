use super::*;

#[rstest]
#[case::fits("Fix the login", 40, "Fix the login")]
#[case::exactly_the_limit("Review the open pull requests and fix it", 40, "Review the open pull requests and fix it")]
#[case::cuts_after_the_last_whole_word(
    "Review the open pull requests and fix the failing checks",
    40,
    "Review the open pull requests and fix…"
)]
#[case::a_space_right_at_the_cut("aaaa bbbb cccc", 10, "aaaa bbbb…")]
#[case::one_long_word("supercalifragilisticexpialidocious", 10, "supercali…")]
#[case::a_short_word_before_a_long_one(
    "Fix https://github.com/acme/shop/issues/482 now",
    40,
    "Fix https://github.com/acme/shop/issues…"
)]
#[case::counts_chars_not_bytes("ñandú ñandú ñandú", 10, "ñandú…")]
#[case::keeps_a_single_ellipsis("Fix the login… then the signup form", 15, "Fix the login…")]
#[case::no_spaces_at_all("修复登录页面的错误并添加测试用例以及更新文档说明", 10, "修复登录页面的错误…")]
fn cuts_after_a_whole_word(#[case] input: &str, #[case] max: usize, #[case] expected: &str) {
    assert_eq!(first_words(input, max), expected);
}
