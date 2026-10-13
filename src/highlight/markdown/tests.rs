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
