use super::*;
use crate::test_util::{TempDir, fake_gh};

const LIST: &str = r#"[{"author":{"login":"ana","is_bot":false},"labels":[{"name":"bug","color":"d73a4a"}],
        "assignees":[{"login":"luis"}],"number":482,"title":"Returns page crashes","state":"OPEN",
        "updatedAt":"2026-09-19T12:50:24Z","url":"https://github.com/acme/shop/issues/482"}]"#;

fn gh_printing(tmp: &TempDir, json: &str) -> std::path::PathBuf {
    fake_gh(tmp.path(), &format!("echo \"$@\" > args; cat <<'EOF'\n{json}\nEOF"))
}

fn args(tmp: &TempDir) -> String {
    std::fs::read_to_string(tmp.path().join("args")).expect("args").trim().to_string()
}

mod list {
    use super::*;

    #[test]
    fn reads_what_gh_prints() {
        let tmp = TempDir::new();
        let gh = gh_printing(&tmp, LIST);

        let issues = list(&gh, tmp.path(), &Query::default()).expect("list");

        let expected = Issue {
            labels: vec!["bug".into()],
            assignees: vec!["luis".into()],
            updated_at: "2026-09-19T12:50:24Z".into(),
            url: "https://github.com/acme/shop/issues/482".into(),
            ..crate::issues::issue(Source::Github, 482, "Returns page crashes")
        };
        assert_eq!(issues, [expected]);
    }

    #[test]
    fn asks_for_open_issues_by_default() {
        let tmp = TempDir::new();
        list(&gh_printing(&tmp, "[]"), tmp.path(), &Query::default()).expect("list");
        assert_eq!(args(&tmp), format!("issue list --state open --limit 100 --json {LIST_FIELDS}"));
    }

    #[test]
    fn closed_and_people_change_the_query() {
        let tmp = TempDir::new();
        let query = Query { closed: true, people: [Who::Me, Who::Person("bo".into())] };
        list(&gh_printing(&tmp, "[]"), tmp.path(), &query).expect("list");
        assert_eq!(
            args(&tmp),
            format!("issue list --state all --limit 100 --json {LIST_FIELDS} --assignee @me --author bo")
        );
    }

    #[test]
    fn people_are_the_assignable_users_of_the_repo() {
        let tmp = TempDir::new();
        let gh = fake_gh(tmp.path(), "echo \"$@\" > args; printf 'zoe\\nana\\n'");

        let found = people(&gh, tmp.path()).expect("people");

        assert_eq!(found.iter().map(|p| p.handle.as_str()).collect::<Vec<_>>(), ["ana", "zoe"]);
        assert_eq!(args(&tmp), "api repos/{owner}/{repo}/assignees --paginate --jq .[].login");
    }

    #[test]
    fn a_closed_issue_says_so() {
        let tmp = TempDir::new();
        let json = r#"[{"number":1,"title":"x","state":"CLOSED","author":null,"url":"u"}]"#;
        let issue = list(&gh_printing(&tmp, json), tmp.path(), &Query::default()).expect("list").remove(0);
        assert_eq!((issue.closed, issue.state.as_str(), issue.author.as_str()), (true, "closed", ""));
    }

    #[test]
    fn reports_the_first_line_gh_prints() {
        let tmp = TempDir::new();
        let gh = fake_gh(tmp.path(), "echo >&2; echo 'no git remotes found' >&2; echo more >&2; exit 1");

        let err = list(&gh, tmp.path(), &Query::default()).expect_err("gh fails");

        assert_eq!(err.to_string(), "no git remotes found");
    }

    #[test]
    fn says_when_gh_is_missing() {
        let tmp = TempDir::new();
        let err = list(&tmp.path().join("gh"), tmp.path(), &Query::default()).expect_err("no gh");
        assert!(matches!(err, Error::RunGh(_)), "{err:?}");
    }

    #[test]
    fn rejects_other_output() {
        let tmp = TempDir::new();
        let err = list(&gh_printing(&tmp, "not json"), tmp.path(), &Query::default()).expect_err("bad output");
        assert!(matches!(err, Error::GhOutput(_)), "{err:?}");
    }
}

mod view {
    use super::*;

    const VIEW: &str = r#"{"number":482,"title":"Returns page crashes","state":"OPEN","labels":[{"name":"bug"}],
            "assignees":[{"login":"luis"}],"author":{"login":"ana"},"updatedAt":"","url":"u",
            "body":"It **crashes**.","comments":[{"author":{"login":"bo"},"body":"Same here","createdAt":"2026-09-20T10:00:00Z"},
            {"author":{"login":"x"},"body":"  ","createdAt":""}]}"#;

    #[test]
    fn reads_the_body_and_the_comments() {
        let tmp = TempDir::new();

        let detail = view(&gh_printing(&tmp, VIEW), tmp.path(), 482).expect("view");

        assert_eq!(
            detail,
            Detail {
                info: vec!["open".into(), "bug".into(), "assigned to @luis".into(), "opened by @ana".into()],
                body: "It **crashes**.".into(),
                comments: vec![Comment {
                    author: "bo".into(),
                    created_at: "2026-09-20T10:00:00Z".into(),
                    body: "Same here".into()
                }],
            }
        );
    }

    #[test]
    fn asks_gh_for_that_issue() {
        let tmp = TempDir::new();
        view(&gh_printing(&tmp, VIEW), tmp.path(), 482).expect("view");
        assert_eq!(args(&tmp), format!("issue view 482 --json {VIEW_FIELDS}"));
    }
}
