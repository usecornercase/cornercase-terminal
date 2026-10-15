use super::*;

mod format {
    use super::*;

    #[test]
    fn escape_keeps_names_from_turning_into_markdown() {
        assert_eq!(Markdown.escape("my_project *v2* `x` > #1"), r"my\_project \*v2\* \`x\` \> \#1");
    }

    #[test]
    fn bold_is_two_stars() {
        assert_eq!(Markdown.bold("claude"), "**claude**");
    }

    #[test]
    fn a_screen_is_a_code_block_that_cannot_be_closed_from_inside() {
        let screen = Markdown.pre("one\n```\ntwo", 100);

        assert_eq!(screen, "```\none\n`\u{200b}``\ntwo\n```");
    }

    #[test]
    fn a_screen_keeps_its_last_lines_within_its_room() {
        let text: Vec<String> = (0..500).map(|i| format!("line {i}")).collect();

        let screen = Markdown.pre(&text.join("\n"), 300);

        assert!(length(&screen) <= 300, "{}", length(&screen));
        assert!(screen.ends_with("line 499\n```"));
    }

    #[test]
    fn the_room_is_what_the_header_leaves() {
        assert_eq!(Markdown.room("abc"), 1995);
        assert!(Markdown.fits("abc", &"x".repeat(1995)));
        assert!(!Markdown.fits("abc", &"x".repeat(1996)));
    }

    #[test]
    fn lines_are_packed_into_messages() {
        let rows: Vec<String> = (0..300).map(|i| format!("agent number {i}")).collect();

        let messages = Markdown.lines(&rows);

        assert!(messages.len() > 1 && messages.iter().all(|m| length(m) <= LIMIT));
        assert_eq!(messages.join("\n"), rows.join("\n"));
    }
}

mod splitting {
    use super::*;

    #[test]
    fn a_short_message_passes_as_it_is() {
        let source = "**Done.** I fixed `src/a.rs`.\n\n- one\n- two\n\n```rust\nfn a() {}\n\nfn b() {}\n```";

        assert_eq!(markdown(source), vec![source.to_string()]);
    }

    #[test]
    fn a_long_code_block_is_closed_and_opened_again() {
        let code: Vec<String> = (0..400).map(|i| format!("let x{i} = {i};")).collect();
        let source = format!("Here:\n```rust\n{}\n```", code.join("\n"));

        let messages = markdown(&source);

        assert!(messages.len() > 1);
        for message in &messages {
            assert!(length(message) <= LIMIT, "{}", length(message));
            assert_eq!(message.matches("```").count() % 2, 0, "{message}");
        }
        assert!(messages[1].starts_with("```rust\n"));
    }

    #[test]
    fn a_huge_message_stops_after_four_parts() {
        let source: Vec<String> = (0..2000).map(|i| format!("paragraph {i} with some words in it")).collect();

        let messages = markdown(&source.join("\n\n"));

        assert_eq!(messages.len(), 4);
        assert!(messages[3].ends_with(CUT));
        assert!(messages.iter().all(|m| length(m) <= MAX));
    }

    #[test]
    fn a_line_longer_than_a_message_is_cut() {
        let messages = markdown(&"y".repeat(5000));

        assert_eq!(messages.len(), 3);
        assert!(messages.iter().all(|m| length(m) <= LIMIT));
    }

    #[test]
    fn a_tilde_fence_becomes_backticks_that_its_code_cannot_close() {
        let source = "Run:\n~~~sh\necho ```\nls\n~~~\nDone.";

        assert_eq!(markdown(source), vec!["Run:\n```sh\necho `\u{200b}``\nls\n```\nDone.".to_string()]);
    }

    #[test]
    fn a_long_tilde_block_is_opened_again_with_backticks() {
        let code: Vec<String> = (0..400).map(|i| format!("let x{i} = {i};")).collect();
        let source = format!("~~~~rust\n{}\n~~~~", code.join("\n"));

        let messages = markdown(&source);

        assert!(messages.len() > 1);
        assert!(messages.iter().all(|m| m.starts_with("````rust\n") && m.ends_with("\n````") && !m.contains('~')));
    }

    #[test]
    fn an_unclosed_fence_is_closed() {
        assert_eq!(markdown("```\nlet a;"), vec!["```\nlet a;\n```".to_string()]);
    }
}
