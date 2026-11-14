use rstest::rstest;

use super::*;

mod display_path;
mod first_words;
mod folder_name;
mod truncate_cells;
mod truncate_left;
mod truncate_right;

#[rstest]
#[case::nothing(0, "0 KB")]
#[case::kilobytes(300 * 1024, "300 KB")]
#[case::just_under_a_megabyte(MB / 2 - 1, "512 KB")]
#[case::megabytes(300 * MB, "300 MB")]
#[case::nearly_a_gigabyte(GB - MB / 10, "1.0 GB")]
#[case::gigabytes(GB * 25 / 2, "12.5 GB")]
fn size_reads_in_kilobytes_megabytes_then_gigabytes(#[case] bytes: u64, #[case] expected: &str) {
    assert_eq!(size(bytes), expected);
}

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

#[rstest]
#[case::fits("npm ci", 10, &["npm ci"])]
#[case::splits_at_the_width("abcdefg", 3, &["abc", "def", "g"])]
#[case::never_splits_a_wide_char("a编码", 2, &["a", "编", "码"])]
#[case::empty("", 4, &[""])]
#[case::no_width("abc", 0, &["abc"])]
fn wrap_cells_keeps_every_character_within_the_width(#[case] text: &str, #[case] max: usize, #[case] rows: &[&str]) {
    assert_eq!(wrap_cells(text, max), rows);
}

#[rstest]
#[case::plain("/C:/Users/me", Some("/C:/Users/me"))]
#[case::escapes("/a%20b/%C3%B1", Some("/a b/ñ"))]
#[case::a_percent_without_digits("/50%/x%2", Some("/50%/x%2"))]
#[case::bytes_that_are_not_utf8("/%FF", None)]
fn percent_decoded_turns_escapes_into_utf8(#[case] text: &str, #[case] expected: Option<&str>) {
    assert_eq!(percent_decoded(text).as_deref(), expected);
}

#[rstest]
#[case::unreserved("Az09-._~", "Az09-._~")]
#[case::space_and_reserved("a b&c#d%e", "a%20b%26c%23d%25e")]
#[case::accent("é", "%C3%A9")]
#[case::emoji("🙂", "%F0%9F%99%82")]
fn percent_encoded_escapes_every_byte_but_the_unreserved(#[case] text: &str, #[case] expected: &str) {
    assert_eq!(percent_encoded(text), expected);
    assert_eq!(percent_decoded(expected).as_deref(), Some(text));
}

#[test]
fn plural_names_one_in_the_singular() {
    assert_eq!(
        (plural(1, "repository", "repositories"), plural(0, "repository", "repositories"), plural(2, "a", "b")),
        ("1 repository".to_string(), "0 repositories".to_string(), "2 b".to_string())
    );
}

mod within_bytes {
    use super::*;

    #[rstest]
    #[case::ascii_at_the_limit("abcd", 4, "abcd")]
    #[case::ascii_over_the_limit("abcdef", 4, "abcd")]
    #[case::a_three_byte_char_across_the_limit("ab日", 3, "ab")]
    #[case::empty("", 4, "")]
    fn keeps_whole_chars(#[case] text: &str, #[case] max: usize, #[case] expected: &str) {
        assert_eq!(within_bytes(text, max), expected);
    }
}
