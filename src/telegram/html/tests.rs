use super::*;

mod markdown {
    use super::*;

    #[test]
    fn an_agent_reply_becomes_telegram_html() {
        let source = "## Done\n\nFixed **the login** in `src/auth.rs` & added *a test*.\n\n\
            - first\n- second\n  1. nested\n\n```rust\nfn main() {}\n```\n\nSee [the PR](https://github.com/a/b/pull/1) \
            and [a file](src/x.rs).";

        let messages = markdown(source);

        assert_eq!(
            messages,
            vec![
                "<b>Done</b>\n\n\
                 Fixed <b>the login</b> in <code>src/auth.rs</code> &amp; added <i>a test</i>.\n\n\
                 • first\n• second\n  1. nested\n\n\
                 <pre><code class=\"language-rust\">fn main() {}</code></pre>\n\n\
                 See <a href=\"https://github.com/a/b/pull/1\">the PR</a> and a file."
                    .to_string()
            ]
        );
    }

    #[test]
    fn html_written_by_the_agent_stays_text() {
        assert_eq!(
            markdown("a <b>tag</b> & <script>"),
            vec!["a &lt;b&gt;tag&lt;/b&gt; &amp; &lt;script&gt;".to_string()]
        );
    }

    #[test]
    fn a_long_reply_is_split_into_messages_under_the_limit() {
        let paragraph = "word ".repeat(300);
        let source = std::iter::repeat_n(paragraph.trim(), 6).collect::<Vec<_>>().join("\n\n");

        let messages = markdown(&source);

        assert_eq!(messages.len(), 3);
        assert!(messages.iter().all(|m| length(&plain(m)) <= LIMIT), "{:?}", messages.iter().map(String::len));
    }

    #[test]
    fn a_huge_code_block_is_split_keeping_it_code() {
        let code = (0..400).map(|i| format!("line {i} of the output")).collect::<Vec<_>>().join("\n");

        let messages = markdown(&format!("```\n{code}\n```"));

        assert!(messages.len() > 1);
        assert!(messages.iter().all(|m| m.starts_with("<pre>") && length(&plain(m)) <= LIMIT));
    }

    #[test]
    fn past_four_messages_the_rest_is_left_in_cornercase() {
        let source = std::iter::repeat_n("x".repeat(3_000), 8).collect::<Vec<_>>().join("\n\n");

        let messages = markdown(&source);

        assert_eq!(messages.len(), 4);
        assert!(messages[3].ends_with(&format!("<i>{CUT}</i>")), "{}", &messages[3][messages[3].len() - 60..]);
    }
}

mod screens {
    use super::*;

    #[test]
    fn a_screen_keeps_its_last_lines_as_code() {
        let screen = (0..2_000).map(|i| format!("row {i}")).collect::<Vec<_>>().join("\n");

        let pre = pre(&screen);

        assert!(pre.starts_with("<pre>") && pre.ends_with("row 1999</pre>"));
        assert!(length(&plain(&pre)) <= LIMIT);
    }

    #[test]
    fn plain_text_undoes_the_html() {
        assert_eq!(plain("<b>a</b> &lt;b&gt; &amp;amp; &quot;"), "a <b> &amp; \"");
    }
}
