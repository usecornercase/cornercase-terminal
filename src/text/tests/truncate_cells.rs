use super::*;

#[rstest]
#[case::fits("coder", 5, "coder")]
#[case::ascii_like_truncate_right("reviewer", 5, "revi…")]
#[case::wide_fits("编码", 4, "编码")]
#[case::wide_counts_two("编码实现", 5, "编码…")]
#[case::never_splits_a_wide_char("编码实现", 6, "编码…")]
#[case::mixed("a编码", 4, "a编…")]
fn truncate_cells_keeps_within_the_cells(#[case] input: &str, #[case] max: usize, #[case] expected: &str) {
    assert_eq!(truncate_cells(input, max), expected);
    assert!(cells(&truncate_cells(input, max)) <= max);
}

#[test]
fn cells_counts_wide_chars_twice() {
    assert_eq!((cells("coder"), cells("编码"), cells("a编")), (5, 4, 3));
}
