use std::process::Command;

use rstest::rstest;

use super::*;
use crate::test_util::TempDir;

fn read_by_git(config: &str, key: &str) -> Option<String> {
    let dir = TempDir::new();
    let file = dir.path().join("config");
    std::fs::write(&file, config).expect("write config");
    let output = Command::new("git").arg("config").arg("--file").arg(&file).args(["--get", key]).output();
    let output = output.expect("run git config");
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim_end_matches('\n').to_string())
}

fn urls(config: &str) -> Vec<(Option<String>, Option<String>)> {
    entries(config)
        .into_iter()
        .filter(|entry| entry.section == "remote" && entry.key == "url")
        .map(|entry| (entry.subsection, entry.value))
        .collect()
}

mod a_value {
    use super::*;

    #[rstest]
    #[case::plain("[remote \"origin\"]\n\turl = https://github.com/a/b.git\n", "https://github.com/a/b.git")]
    #[case::quoted(
        "[remote \"origin\"]\n\turl = \"https://ghp_x@github.com/a/b.git\"\n",
        "https://ghp_x@github.com/a/b.git"
    )]
    #[case::quoted_in_parts("[remote \"origin\"]\n\turl = https://github.com/\"a b\"/c\n", "https://github.com/a b/c")]
    #[case::a_hash_comment("[remote \"origin\"]\n\turl = https://github.com/a/b # mine\n", "https://github.com/a/b")]
    #[case::a_semicolon_comment("[remote \"origin\"]\n\turl = https://github.com/a/b;mine\n", "https://github.com/a/b")]
    #[case::a_hash_in_quotes("[remote \"origin\"]\n\turl = \"https://github.com/a/b#c\"\n", "https://github.com/a/b#c")]
    #[case::escapes("[remote \"origin\"]\n\turl = a\\\\b\\\"c\\td\\ne\n", "a\\b\"c\td\ne")]
    #[case::a_continued_line("[remote \"origin\"]\n\turl = https://github.com/\\\na/b\n", "https://github.com/a/b")]
    #[case::inner_spaces_kept("[remote \"origin\"]\n\turl = a  b   \n", "a  b")]
    #[case::no_spaces_around("[remote \"origin\"]\nurl=x\n", "x")]
    #[case::windows_line_ends("[remote \"origin\"]\r\n\turl = x\r\n", "x")]
    #[case::any_case("[REMOTE \"origin\"]\n\tURL = x\n", "x")]
    #[case::the_old_header("[remote.origin]\n\turl = x\n", "x")]
    #[case::after_a_header_on_its_line("[remote \"origin\"] url = x\n", "x")]
    #[case::the_last_line_unended("[remote \"origin\"]\n\turl = \"x\"", "x")]
    fn reads_as_git_does(#[case] config: &str, #[case] expected: &str) {
        assert_eq!(urls(config), [(Some("origin".to_string()), Some(expected.to_string()))]);
        assert_eq!(read_by_git(config, "remote.origin.url").as_deref(), Some(expected));
    }

    #[test]
    fn is_none_for_a_key_with_no_equals_sign() {
        assert_eq!(urls("[remote \"origin\"]\n\turl\n"), [(Some("origin".to_string()), None)]);
    }
}

mod a_header {
    use super::*;

    #[test]
    fn reads_escapes_in_the_subsection() {
        let config = "[remote \"a\\\"b\\\\c\"]\n\turl = x\n";

        assert_eq!(urls(config), [(Some("a\"b\\c".to_string()), Some("x".to_string()))]);
        assert_eq!(read_by_git(config, "remote.a\"b\\c.url").as_deref(), Some("x"));
    }

    #[test]
    fn keeps_the_subsection_s_case() {
        assert_eq!(urls("[remote \"Origin\"]\n\turl = x\n"), [(Some("Origin".to_string()), Some("x".to_string()))]);
    }

    #[test]
    fn has_no_subsection_when_plain() {
        let entries = entries("[core]\n\tbare = false\n");

        assert_eq!(
            entries,
            [Entry {
                section: "core".to_string(),
                subsection: None,
                key: "bare".to_string(),
                value: Some("false".to_string())
            }]
        );
    }
}

mod what_git_refuses {
    use super::*;

    #[rstest]
    #[case::an_unended_quote("\turl = \"https://a\n")]
    #[case::an_unknown_escape("\turl = https://a\\x b = c\n")]
    #[case::a_comment_after_the_key("\turl # = https://a\n")]
    fn is_left_out_and_the_next_line_still_read(#[case] bad: &str) {
        let config = format!("[remote \"origin\"]\n{bad}\turl = https://b\n");

        assert_eq!(urls(&config), [(Some("origin".to_string()), Some("https://b".to_string()))]);
    }

    #[rstest]
    #[case::an_unended_header("[remote \"origin\n\turl = https://a\n")]
    #[case::a_header_with_junk("[remote \"origin\" x]\n\turl = https://a\n")]
    #[case::a_header_with_no_name("[]\n\turl = https://a\n")]
    fn a_broken_header_owns_no_key(#[case] config: &str) {
        assert_eq!(urls(config), []);
    }

    #[test]
    fn keys_before_any_section_are_left_out() {
        assert_eq!(entries("url = https://a\n"), []);
    }
}

mod comments {
    use super::*;

    #[test]
    fn whole_lines_and_a_continuation_inside_them_are_skipped() {
        let config = "# a comment \\\n[remote \"origin\"]\n\t# url = z\n\t; url = y\n\turl = x\n";

        assert_eq!(urls(config), [(Some("origin".to_string()), Some("x".to_string()))]);
    }
}
