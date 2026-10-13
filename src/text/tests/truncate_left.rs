use super::*;

#[rstest]
#[case::fits("short", 10, "short")]
#[case::keeps_the_end("/a/b/c/project", 8, "…project")]
#[case::counts_chars_not_bytes("ñandú/añil", 5, "…añil")]
#[case::keeps_a_single_ellipsis("x › …/b/project", 12, "…/b/project")]
fn shortens_from_the_left(#[case] input: &str, #[case] max: usize, #[case] expected: &str) {
    assert_eq!(truncate_left(input, max), expected);
}
