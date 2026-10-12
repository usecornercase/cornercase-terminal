use rstest::rstest;

use super::*;

fn style_of_word(code: &str, info: &str, word: &str) -> Style {
    highlight(code, info)
        .expect("highlighted")
        .iter()
        .flatten()
        .find(|(text, _)| text.contains(word))
        .map(|(_, style)| *style)
        .expect("the word is there")
}

#[rstest]
#[case::keyword("fn main() {}", "rust", "fn", Color::Magenta)]
#[case::function_name("fn main() {}", "rust", "main", Color::Cyan)]
#[case::string("let s = \"hi\";", "rust", "hi", Color::Green)]
#[case::number("x = 42", "python", "42", Color::Yellow)]
#[case::comment("# note\nx = 1", "py", "note", Color::DarkGray)]
#[case::typescript_as_javascript("const a = 'x'", "ts", "const", Color::Magenta)]
#[case::diff_addition("+added line\n-removed line", "diff", "added", Color::Green)]
#[case::diff_deletion("+added line\n-removed line", "diff", "removed", Color::Red)]
fn colours_tokens_with_the_terminal_palette(
    #[case] code: &str,
    #[case] info: &str,
    #[case] word: &str,
    #[case] colour: Color,
) {
    assert_eq!(style_of_word(code, info, word).fg, Some(colour));
}

#[test]
fn operators_stay_plain() {
    assert_eq!(style_of_word("let x = 1 + 2;", "rust", "+").fg, None);
}

#[test]
fn keeps_one_line_per_source_line_without_newlines() {
    let lines = highlight("a = 1\nb = 2\n", "python").expect("highlighted");
    let text: Vec<String> = lines.iter().map(|l| l.iter().map(|(t, _)| t.as_str()).collect()).collect();
    assert_eq!(text, ["a = 1", "b = 2"]);
}

#[rstest]
#[case::extra_words("rust ignore", Some("rust"))]
#[case::attributes("js {1,3}", Some("js"))]
#[case::case("Python", Some("python"))]
#[case::alias("tsx", Some("js"))]
#[case::empty("  ", None)]
fn reads_the_language_from_the_fence(#[case] info: &str, #[case] expected: Option<&str>) {
    assert_eq!(language_token(info).as_deref(), expected);
}

#[test]
fn an_unknown_language_is_not_highlighted() {
    assert!(highlight("x", "no-such-language").is_none());
}

#[test]
fn the_same_block_comes_from_the_cache() {
    let first = highlight("fn cached() {}", "rust").expect("highlighted");
    let second = highlight("fn cached() {}", "rust").expect("highlighted");
    assert!(Arc::ptr_eq(&first, &second));
}
