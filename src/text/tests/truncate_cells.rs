use super::*;

#[rstest]
#[case::fits("coder", 5, "coder")]
#[case::ascii_like_truncate_right("reviewer", 5, "revi…")]
#[case::wide_fits("编码", 4, "编码")]
#[case::wide_counts_two("编码实现", 5, "编码…")]
#[case::never_splits_a_wide_char("编码实现", 6, "编码…")]
#[case::mixed("a编码", 4, "a编…")]
#[case::emoji_presentation_fits("❤️❤️", 4, "❤️❤️")]
#[case::emoji_presentation_counts_two("❤️❤️", 3, "❤️…")]
#[case::never_splits_a_joined_emoji("👨‍👩‍👧ab", 3, "👨‍👩‍👧…")]
fn truncate_cells_keeps_within_the_cells(#[case] input: &str, #[case] max: usize, #[case] expected: &str) {
    assert_eq!(truncate_cells(input, max), expected);
    assert!(cells(&truncate_cells(input, max)) <= max);
}

#[rstest]
#[case::fits("~/code", 6, "~/code")]
#[case::ascii_like_truncate_left("~/code/app", 6, "…e/app")]
#[case::wide_counts_two("~/项目/客户端", 7, "…客户端")]
#[case::never_splits_a_wide_char("~/项目/客户端", 6, "…户端")]
#[case::emoji_presentation_counts_two("a❤️❤️", 3, "…❤️")]
fn truncate_cells_left_keeps_the_end_within_the_cells(#[case] input: &str, #[case] max: usize, #[case] expected: &str) {
    assert_eq!(truncate_cells_left(input, max), expected);
    assert!(cells(&truncate_cells_left(input, max)) <= max);
}

#[rstest]
#[case::wide("编码", 3, "编")]
#[case::emoji_presentation("❤️❤️", 3, "❤️")]
fn clip_cells_keeps_only_what_fits(#[case] input: &str, #[case] max: usize, #[case] expected: &str) {
    assert_eq!(clip_cells(input, max), expected);
}

#[test]
fn pad_cells_fills_to_the_cells_a_wide_text_takes() {
    assert_eq!(
        (pad_cells("编码", 6), pad_cells("ab", 4), pad_cells("编码实现", 4)),
        ("编码  ".into(), "ab  ".into(), "编码实现".into())
    );
}

#[test]
fn cells_counts_wide_chars_twice() {
    assert_eq!((cells("coder"), cells("编码"), cells("a编")), (5, 4, 3));
}
