use rstest::rstest;

use super::*;

fn style_of(source: &str, language: &str, word: &str) -> Style {
    highlight(source, language)
        .expect("highlighted")
        .iter()
        .flatten()
        .find(|(text, _)| text == word || text.trim() == word)
        .map_or_else(|| panic!("{word:?} is a segment of its own"), |(_, style)| *style)
}

#[rstest]
#[case::rust_keyword("fn main() {}\n", "rust", "fn", Color::Magenta)]
#[case::rust_function("fn main() {}\n", "rust", "main", Color::Blue)]
#[case::typescript_type("let a: Order = load();\n", "typescript", "Order", Color::Cyan)]
#[case::typescript_string("const a = 'x';\n", "typescript", "'x'", Color::Green)]
#[case::python_number("x = 42\n", "python", "42", Color::Yellow)]
#[case::jsx_tag("const a = <div className=\"x\" />;\n", "tsx", "div", Color::Red)]
#[case::jsx_attribute("const a = <div className=\"x\" />;\n", "tsx", "className", Color::Yellow)]
fn colours_tokens_with_the_terminal_palette(
    #[case] source: &str,
    #[case] language: &str,
    #[case] word: &str,
    #[case] colour: Color,
) {
    assert_eq!(style_of(source, language, word).fg, Some(colour));
}

#[test]
fn comments_are_grey_and_italic() {
    let style = style_of("# a note\nx = 1\n", "python", "# a note");
    assert_eq!((style.fg, style.add_modifier.contains(Modifier::ITALIC)), (Some(Color::DarkGray), true));
}

#[test]
fn keeps_one_entry_per_line_with_tabs_expanded() {
    let lines = highlight("fn a() {\n\tb();\n}\n", "rust").expect("highlighted");
    let text: Vec<String> = lines.iter().map(|l| l.iter().map(|(t, _)| t.as_str()).collect()).collect();
    assert_eq!(text, ["fn a() {", "    b();", "}"]);
}

#[test]
fn a_multiline_comment_colours_every_line() {
    let lines = highlight("/* one\ntwo */\nfn a() {}\n", "rust").expect("highlighted");
    assert_eq!(lines[1].first().map(|(t, s)| (t.as_str(), s.fg)), Some(("two */", Some(Color::DarkGray))));
}

#[test]
fn a_huge_source_is_left_plain() {
    assert_eq!(highlight(&"x".repeat(MAX_BYTES + 1), "rust"), None);
}

#[rstest]
#[case::by_extension("src/main.rs", "", Some("rust"))]
#[case::javascript_reads_jsx("App.jsx", "", Some("tsx"))]
#[case::plain_javascript_too("index.js", "", Some("tsx"))]
#[case::header_as_c("lib.h", "", Some("c"))]
#[case::by_name("Dockerfile", "", Some("dockerfile"))]
#[case::dotfile("home/.zshrc", "", Some("zsh"))]
#[case::shebang("bin/deploy", "#!/usr/bin/env bash", Some("bash"))]
#[case::versioned_shebang("bin/tool", "#!/usr/bin/python3", Some("python"))]
#[case::family_with_slashes("App.kt", "", Some("javascript"))]
#[case::family_with_hashes("app/models/user.rb", "", Some("python"))]
#[case::family_with_dashes("db/schema.sql", "", Some("lua"))]
#[case::text_stays_plain("notes.txt", "", None)]
#[case::unknown("data.xyz", "", None)]
fn picks_a_grammar_for_every_file(#[case] path: &str, #[case] first: &str, #[case] expected: Option<&str>) {
    assert_eq!(language(Path::new(path), first), expected);
}
