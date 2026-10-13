use rstest::rstest;

use super::*;

fn gh(number: u64, title: &str) -> Issue {
    issue(Source::Github, number, title)
}

mod filter {
    use super::*;

    fn found(query: &str) -> Vec<u64> {
        let issues = [
            Issue { labels: vec!["bug".into()], ..gh(482, "Returns page crashes") },
            Issue { author: "luis".into(), state: "In Review".into(), ..gh(48, "Dark mode") },
        ];
        filter(&issues, query).iter().map(|i| i.number).collect()
    }

    #[rstest]
    #[case::empty("", vec![482, 48])]
    #[case::title("CRASH", vec![482])]
    #[case::key("#48", vec![482, 48])]
    #[case::whole_key("#482", vec![482])]
    #[case::label("bug", vec![482])]
    #[case::author("luis", vec![48])]
    #[case::state("review", vec![48])]
    #[case::nothing("zzz", vec![])]
    fn keeps_the_matching_issues_in_order(#[case] query: &str, #[case] expected: Vec<u64>) {
        assert_eq!(found(query), expected);
    }
}

#[test]
fn sorting_puts_the_last_updated_first() {
    let old = Issue { updated_at: "2026-01-01T00:00:00Z".into(), ..gh(1, "old") };
    let new = Issue { updated_at: "2026-09-01T00:00:00.123Z".into(), ..issue(Source::Linear, 2, "new") };
    let mut list = vec![&old, &new];
    sort_by_updated(&mut list);
    assert_eq!(list.iter().map(|i| i.number).collect::<Vec<_>>(), [2, 1]);
}

mod branch {
    use super::*;

    #[rstest]
    #[case::plain("Returns page crashes", "issue-482-returns-page-crashes")]
    #[case::symbols("Fix: `cd` breaks (again)!", "issue-482-fix-cd-breaks-again")]
    #[case::accents("Añadir búsqueda rápida", "issue-482-anadir-busqueda-rapida")]
    #[case::nothing_left("???", "issue-482")]
    #[case::cut_at_a_word(
        "Returns page crashes when the shipping address is empty on mobile",
        "issue-482-returns-page-crashes-when-the-shipping"
    )]
    fn is_named_after_a_github_issue(#[case] title: &str, #[case] expected: &str) {
        assert_eq!(branch(&gh(482, title)), expected);
    }

    #[rstest]
    #[case::story(Source::Shortcut, "sc-12-dark-mode")]
    #[case::linear(Source::Linear, "ENG-12-dark-mode")]
    #[case::jira(Source::Jira, "PROJ-12-dark-mode")]
    #[case::plane(Source::Plane, "WEB-12-dark-mode")]
    fn starts_with_the_key_of_the_tracker(#[case] source: Source, #[case] expected: &str) {
        assert_eq!(branch(&issue(source, 12, "Dark mode")), expected);
    }
}

#[test]
fn the_workspace_is_named_after_the_issue() {
    let name = workspace_name(&gh(482, "Returns page crashes when the shipping address is empty"));
    assert_eq!(name, "#482 Returns page crashes when the ship…");
}

mod prompt {
    use super::*;

    #[rstest]
    #[case::url("{url}", "https://github.com/acme/shop/issues/7")]
    #[case::text_as_typed("Fix {key}: {title}", "Fix #7: it's broken")]
    #[case::number_and_branch("{number} on {branch}", "7 on issue-7")]
    #[case::ref_and_source("{ref} from {source}", "#7 from github")]
    #[case::unknown_placeholders_stay("x {nope} {url", "x {nope} {url")]
    #[case::newlines_are_kept("a\nb", "a\nb")]
    fn fills_in_the_issue(#[case] template: &str, #[case] expected: &str) {
        assert_eq!(prompt(template, &gh(7, "it's broken"), "issue-7"), expected);
    }
}

mod meta {
    use super::*;

    const UPDATED: &str = "2026-09-19T12:00:00Z";

    fn updated() -> Issue {
        Issue { updated_at: UPDATED.into(), labels: vec!["bug".into(), "p1".into()], ..gh(1, "x") }
    }

    #[rstest]
    #[case::minutes(5 * 60, "5m")]
    #[case::hours(3 * 3_600, "3h")]
    #[case::days(4 * 86_400, "4d")]
    #[case::months(65 * 86_400, "2mo")]
    #[case::years(800 * 86_400, "2y")]
    fn shows_labels_author_and_age(#[case] ago: i64, #[case] age: &str) {
        let now = parse_time(UPDATED).expect("timestamp") + ago;
        assert_eq!(meta(&updated(), now), format!("bug, p1 · ana · {age}"));
    }

    #[test]
    fn shows_the_state_of_stories_and_closed_issues() {
        let story = Issue { state: "In Review".into(), ..issue(Source::Shortcut, 1, "x") };
        let closed = Issue { state: "closed".into(), closed: true, ..gh(2, "y") };
        assert_eq!((meta(&story, 0), meta(&closed, 0)), ("In Review · ana".into(), "closed · ana".into()));
    }

    #[test]
    fn leaves_out_what_is_missing() {
        assert_eq!(meta(&Issue { author: String::new(), ..gh(1, "x") }, 0), "");
    }
}

#[test]
fn a_checklist_is_a_bold_title_and_task_items() {
    let items = [(true, "Reproduce".to_string()), (false, "Fix\nit".to_string())];
    assert_eq!(checklist("Tasks", &items), "**Tasks**\n\n- [x] Reproduce\n- [ ] Fix it");
}

#[test]
fn people_describe_the_fields_that_filter() {
    let people = People { shortcut: [Who::Me, Who::Person("bo".into())], ..People::default() };
    assert_eq!(
        (people.describe(Source::Shortcut), people.describe(Source::Linear)),
        ("owner me · requester @bo".into(), String::new())
    );
}

#[test]
fn an_account_describes_who_and_where() {
    let account = Account { handle: "ana".into(), workspace: "acme".into() };
    assert_eq!(account.describe(), "@ana in acme");
}
