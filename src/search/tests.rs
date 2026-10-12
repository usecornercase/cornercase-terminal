use rstest::rstest;

use super::*;

fn candidate(kind: Kind, name: &str, keys: &[&str]) -> Candidate {
    Candidate {
        kind,
        goto: Goto::Group(1),
        name: name.into(),
        context: String::new(),
        keys: keys.iter().map(|k| (*k).to_string()).collect(),
    }
}

fn names(results: &[Candidate]) -> Vec<&str> {
    results.iter().map(|c| c.name.as_str()).collect()
}

mod rank {
    use super::*;

    #[test]
    fn an_empty_query_finds_nothing() {
        assert_eq!(rank(vec![candidate(Kind::Project, "api", &["api"])], "  "), Vec::<Candidate>::new());
    }

    #[test]
    fn leaves_out_what_does_not_match() {
        let found =
            rank(vec![candidate(Kind::Project, "api", &["api"]), candidate(Kind::Project, "web", &["web"])], "we");
        assert_eq!(names(&found), ["web"]);
    }

    #[test]
    fn exact_then_prefix_then_substring() {
        let found = rank(
            vec![
                candidate(Kind::Project, "my-api", &["my-api"]),
                candidate(Kind::Project, "api-docs", &["api-docs"]),
                candidate(Kind::Project, "api", &["api"]),
            ],
            "api",
        );
        assert_eq!(names(&found), ["api", "api-docs", "my-api"]);
    }

    #[test]
    fn projects_then_workspaces_then_tabs_on_a_tie() {
        let found = rank(
            vec![
                candidate(Kind::Tab, "login-tab", &["login-tab"]),
                candidate(Kind::Workspace, "login-ws", &["login-ws"]),
                candidate(Kind::Project, "login-app", &["login-app"]),
            ],
            "login",
        );
        assert_eq!(names(&found), ["login-app", "login-ws", "login-tab"]);
    }

    #[test]
    fn keeps_the_given_order_otherwise() {
        let found = rank(
            vec![
                candidate(Kind::Tab, "zsh", &["zsh", "feat/b"]),
                candidate(Kind::Tab, "nvim", &["nvim", "feat/a"]),
            ],
            "feat",
        );
        assert_eq!(names(&found), ["zsh", "nvim"]);
    }

    #[test]
    fn any_key_matches_and_the_best_one_counts() {
        let found = rank(
            vec![
                candidate(Kind::Workspace, "renamed", &["renamed", "feat/x"]),
                candidate(Kind::Workspace, "other", &["other", "x"]),
            ],
            "x",
        );
        assert_eq!(names(&found), ["other", "renamed"]);
    }

    #[test]
    fn ignores_case() {
        assert_eq!(names(&rank(vec![candidate(Kind::Project, "API", &["API"])], "api")), ["API"]);
    }
}

mod find {
    use super::*;

    #[rstest]
    #[case::start("feat/search", "feat", Some((0, 4)))]
    #[case::middle("my-api", "API", Some((3, 6)))]
    #[case::counts_chars_not_bytes("ñandú-api", "api", Some((6, 9)))]
    #[case::missing("zsh", "vim", None)]
    #[case::empty("zsh", "", None)]
    fn returns_the_char_range_of_the_first_match(
        #[case] haystack: &str,
        #[case] needle: &str,
        #[case] expected: Option<(usize, usize)>,
    ) {
        assert_eq!(find(haystack, needle), expected);
    }
}

mod search {
    use super::*;

    #[test]
    fn typing_selects_the_first_result() {
        let mut s = Search::default();
        s.move_selection(2, 5, 10);
        s.push('a');
        assert_eq!((s.query(), s.selected()), ("a", 0));
    }

    #[test]
    fn the_selection_stays_inside_the_results() {
        let mut s = Search::default();
        s.move_selection(10, 3, 10);
        assert_eq!(s.selected(), 2);
    }

    #[test]
    fn moving_down_scrolls_to_keep_the_selection_visible() {
        let mut s = Search::default();
        s.move_selection(4, 10, 3);
        assert_eq!(s.scroll(), 2);
    }

    #[test]
    fn scrolling_stops_at_the_last_page() {
        let mut s = Search::default();
        s.scroll_by(100, 10, 3);
        assert_eq!(s.scroll(), 7);
    }
}
