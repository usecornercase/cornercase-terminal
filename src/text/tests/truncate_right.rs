use super::*;

#[rstest]
#[case::fits("short", 10, "short")]
#[case::keeps_the_start("feature/login-page", 8, "feature…")]
#[case::counts_chars_not_bytes("ñandú/añil", 5, "ñand…")]
#[case::keeps_a_single_ellipsis("shop › #482 empty addr… › bash", 24, "shop › #482 empty addr…")]
fn shortens_from_the_right(#[case] input: &str, #[case] max: usize, #[case] expected: &str) {
    assert_eq!(truncate_right(input, max), expected);
}
