use super::*;

#[rstest]
#[case::keeps_lines("a\nb", "a\nb")]
#[case::drops_escapes("\u{1b}[31mred\u{1b}[0m", "[31mred[0m")]
#[case::drops_carriage_returns("a\r\nb", "a\nb")]
#[case::drops_c1_controls("a\u{9b}2Jb", "a2Jb")]
#[case::drops_bidi_overrides("pay \u{202E}gnp.exe", "pay gnp.exe")]
#[case::drops_bidi_isolates("\u{2066}a\u{2069}\u{200F}b\u{061C}", "ab")]
#[case::turns_tabs_into_spaces("\tcode", "    code")]
fn keeps_only_what_draws_as_written(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(shown(input), expected);
}
