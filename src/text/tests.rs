use rstest::rstest;

use super::*;

mod display_path;
mod folder_name;
mod truncate_cells;
mod truncate_left;
mod truncate_right;

#[test]
fn one_line_joins_lines_and_spaces() {
    assert_eq!(one_line(" a\n  b\tc "), "a b c");
}

#[rstest]
#[case::a_bidi_override("run\u{202e}gpj.sh", "run\\u{202e}gpj.sh")]
#[case::a_zero_width_space("a\u{200b}b", "a\\u{200b}b")]
#[case::a_line_separator("a\u{2028}b", "a\\u{2028}b")]
#[case::a_byte_order_mark("\u{feff}x", "\\u{feff}x")]
#[case::a_tag("a\u{e0041}", "a\\u{e0041}")]
#[case::a_newline("a\nb", "a\\u{a}b")]
#[case::plain_text("Notes · ñ 日本", "Notes · ñ 日本")]
fn escape_hidden_shows_invisible_characters(#[case] text: &str, #[case] shown: &str) {
    assert_eq!(escape_hidden(text), shown);
}
