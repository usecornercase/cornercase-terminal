use rstest::rstest;

use super::*;

fn text(source: &str, width: usize) -> Vec<String> {
    render(source, width).iter().map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect()).collect()
}

fn style_of(source: &str, word: &str) -> Style {
    render(source, 80)
        .iter()
        .flat_map(|line| line.spans.clone())
        .find(|s| s.content.contains(word))
        .map(|s| s.style)
        .expect("the word is rendered")
}

mod blocks {
    use super::*;

    #[test]
    fn paragraphs_wrap_at_the_width_with_a_blank_row_between() {
        assert_eq!(text("one two three four\n\nfive", 10), ["one two", "three four", "", "five"]);
    }

    #[test]
    fn a_line_break_in_the_source_is_kept() {
        assert_eq!(text("first line\nsecond line", 80), ["first line", "second line"]);
    }

    #[test]
    fn a_word_longer_than_the_line_is_cut() {
        assert_eq!(text("abcdefghijklmnop", 10), ["abcdefghij", "klmnop"]);
    }

    #[test]
    fn headings_are_bold_with_a_blank_row_before() {
        assert_eq!(text("intro\n# Title", 80), ["intro", "", "Title"]);
        assert!(style_of("# Title", "Title").add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn bullet_lists_indent_their_wrapped_lines() {
        assert_eq!(text("- one two three\n- four", 12), ["• one two", "  three", "• four"]);
    }

    #[test]
    fn numbered_lists_keep_their_numbers() {
        assert_eq!(text("3. a\n4. b", 80), ["3. a", "4. b"]);
    }

    #[test]
    fn nested_lists_go_under_their_item() {
        assert_eq!(text("- a\n  - b\n- c", 80), ["• a", "  • b", "• c"]);
    }

    #[test]
    fn task_lists_show_their_boxes() {
        assert_eq!(text("- [x] done\n- [ ] todo", 80), ["• [x] done", "• [ ] todo"]);
    }

    #[test]
    fn quotes_have_a_bar() {
        assert_eq!(text("> quoted words here", 12), ["│ quoted", "│ words here"]);
    }

    #[test]
    fn code_blocks_keep_their_lines_with_a_gutter() {
        assert_eq!(text("```rust\nfn main() {\n    x\n}\n```", 80), ["rust", "│ fn main() {", "│     x", "│ }"]);
    }

    #[test]
    fn code_in_a_known_language_is_highlighted() {
        assert_eq!(style_of("```rust\nfn main() {}\n```", "fn").fg, Some(Color::Magenta));
    }

    #[test]
    fn code_in_an_unknown_language_stays_yellow() {
        assert_eq!(style_of("```nope\nsome code\n```", "some code").fg, Some(Color::Yellow));
    }

    #[test]
    fn long_code_lines_are_cut_not_wrapped() {
        assert_eq!(text("```\nabcdefghijklmnop\n```", 10), ["│ abcdefg…"]);
    }

    #[test]
    fn rules_span_the_width() {
        assert_eq!(text("a\n\n---\n\nb", 10), ["a", "", "──────────", "", "b"]);
    }

    #[test]
    fn tables_line_up_their_columns() {
        assert_eq!(text("| a | bb |\n|---|---|\n| ccc | d |", 80), ["a   │ bb", "────┼───", "ccc │ d "]);
    }

    #[test]
    fn html_comments_and_tags_are_dropped() {
        assert_eq!(
            text("<!-- template\nhint -->\n\n<details><summary>Logs</summary>\n\nshown\n</details>", 80),
            ["Logs", "", "shown"]
        );
    }
}

mod inline {
    use super::*;

    #[test]
    fn links_show_their_url_after_the_text() {
        assert_eq!(text("see [the docs](https://x.dev)", 80), ["see the docs (https://x.dev)"]);
    }

    #[test]
    fn a_link_whose_text_is_its_url_shows_it_once() {
        assert_eq!(text("<https://x.dev>", 80), ["https://x.dev"]);
    }

    #[test]
    fn images_show_their_alt_text() {
        assert_eq!(text("![screenshot](a.png)", 80), ["[image: screenshot]"]);
    }

    #[test]
    fn inline_code_is_coloured() {
        assert_eq!(style_of("run `cargo` now", "cargo").fg, Some(Color::Yellow));
    }

    #[test]
    fn emphasis_is_italic() {
        assert!(style_of("an *important* word", "important").add_modifier.contains(Modifier::ITALIC));
    }

    #[rstest]
    #[case::issue("fixed in #482", "#482", REFERENCE)]
    #[case::story("see sc-12 too", "sc-12", REFERENCE)]
    #[case::linear("blocked by ENG-123", "ENG-123", REFERENCE)]
    #[case::mention("thanks @ana", "@ana", MENTION)]
    fn references_and_mentions_stand_out(#[case] source: &str, #[case] word: &str, #[case] style: Style) {
        assert_eq!(style_of(source, word), style);
    }

    #[test]
    fn an_email_is_not_a_mention() {
        assert_eq!(style_of("write to ana@x.dev", "ana@x.dev"), Style::default());
    }
}

mod plain {
    use super::*;

    fn lines(source: &str) -> Vec<String> {
        plain(source).lines().map(crate::text::one_line).filter(|line| !line.is_empty()).collect()
    }

    #[rstest]
    #[case::strong_and_code("**Two** are blocked: `#12` and `#14`.", &["Two are blocked: #12 and #14."])]
    #[case::emphasis_and_strikethrough("an *important* ~~old~~ word", &["an important old word"])]
    #[case::heading("## Summary\n\nTwo are blocked.", &["Summary", "Two are blocked."])]
    #[case::bullets("- **#12** waits for review\n- #14 fails CI", &["#12 waits for review", "#14 fails CI"])]
    #[case::numbers("1. First\n2. Second", &["First", "Second"])]
    #[case::tasks("- [x] done\n- [ ] not yet", &["done", "not yet"])]
    #[case::nested_list("- a\n  - b\n- c", &["a", "b", "c"])]
    #[case::quote("> quoted *text*\n> more", &["quoted text", "more"])]
    #[case::fence("```sh\ngh pr list\n```\nTwo are blocked.", &["gh pr list", "Two are blocked."])]
    #[case::links("see [the PR](https://x.dev/12) and <https://x.dev>", &["see the PR and https://x.dev"])]
    #[case::soft_break("first line\nsecond line", &["first line", "second line"])]
    #[case::inline_html("Use <b>bold</b> html", &["Use bold html"])]
    #[case::html_block("<div>\nTwo are blocked.\n</div>", &["Two are blocked."])]
    #[case::html_block_with_attributes("<p class=\"a\">Two <b>are</b> blocked.</p>", &["Two are blocked."])]
    #[case::comment_over_lines("<!--\nnote\n-->\n\nHello", &["Hello"])]
    #[case::comment_with_a_greater_than("<!-- a > b -->\nHello", &["Hello"])]
    #[case::template_then_details(
        "<!-- template\nhint -->\n\n<details><summary>Logs</summary>\n\nshown\n</details>",
        &["Logs", "shown"]
    )]
    #[case::break_in_a_paragraph("<p>a<br>b</p>", &["a", "b"])]
    #[case::break_in_a_block("<div>\nTwo<br>are<br/>blocked\n</div>", &["Two", "are", "blocked"])]
    #[case::html_comment("<!-- note -->\nHello", &["Hello"])]
    #[case::rule("---\n\nAfter", &["After"])]
    #[case::table("| a | b |\n|---|---|\n| 1 | 2 |", &["a b", "1 2"])]
    #[case::escapes_and_entities("5 \\* 3 &amp; more", &["5 * 3 & more"])]
    #[case::image("![shot](a.png) here", &["shot here"])]
    fn markdown_reads_as_its_words(#[case] source: &str, #[case] expected: &[&str]) {
        assert_eq!(lines(source), expected);
    }
}
