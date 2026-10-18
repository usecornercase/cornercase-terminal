use super::*;
use crate::control::issues::{CommentInfo, IssueInfo, ListedInfo, PlaceInfo, SourceInfo};

fn source(id: &str, state: SourceState) -> SourceInfo {
    SourceInfo { source: id.into(), name: id.into(), tab: true, state, ..SourceInfo::default() }
}

#[test]
fn a_person_is_a_handle_without_its_at_sign() {
    assert_eq!(who(Some("@bo")), Who::Person { handle: "bo".into(), id: None });
    assert_eq!(who(Some("acc-1")), Who::Person { handle: "acc-1".into(), id: None });
    assert_eq!((who(Some("me")), who(Some("anyone")), who(None)), (Who::Me, Who::Anyone, Who::Anyone));
}

#[test]
fn the_sources_read_as_one_line_each_then_the_places() {
    let sources = Sources {
        project: 1,
        sources: vec![
            SourceInfo { from: Some("gh".into()), ..source("github", SourceState::Connected) },
            SourceInfo { account: Some("@ana".into()), ..source("linear", SourceState::Connected) },
            SourceInfo { missing: vec!["site".into(), "token".into()], ..source("jira", SourceState::Missing) },
            SourceInfo { reason: Some("rejected".into()), tab: false, ..source("shortcut", SourceState::Failing) },
        ],
        places: vec![
            PlaceInfo { project: 1, label: "web-shop".into(), worktree: true, here: true, ..PlaceInfo::default() },
            PlaceInfo { project: 2, workspace: Some(7), label: "notes › default".into(), ..PlaceInfo::default() },
        ],
        ..Sources::default()
    };
    let text = sources_text(&sources);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0].trim_end(), "github    connected  through gh");
    assert_eq!(lines[1].trim_end(), "linear    connected  @ana");
    assert_eq!(lines[2].trim_end(), "jira      missing    missing site, token");
    assert_eq!(lines[3].trim_end(), "shortcut  failing    rejected  (not a tab)");
    assert_eq!(lines[5], "places for Shortcut, Linear, Jira, Plane and Asana issues:");
    assert_eq!(lines[6], "  project 1  web-shop  a new worktree  (where you are)");
    assert_eq!(lines[7], "  workspace 7  notes › default  a new tab");
}

#[test]
fn a_list_is_tab_separated_under_a_header_with_dashes_for_nothing() {
    let issue = IssueInfo {
        source: "github".into(),
        key: "#7".into(),
        title: "Fix\tthe\nlogin".into(),
        ..IssueInfo::default()
    };
    let text = tsv(&LIST_COLUMNS, &[issue]).expect("a table");
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("source\tkey\tstate\tauthor\tupdated_at\ttitle\turl\tbranch"));
    assert_eq!(lines.next(), Some("github\t#7\t-\t-\t-\tFix the login\t-\t-"));
}

#[test]
fn the_people_are_a_table_too() {
    let person =
        crate::control::issues::PersonInfo { handle: "ana".into(), name: String::new(), id: Some("acc-1".into()) };
    assert_eq!(tsv(&PEOPLE_COLUMNS, &[person]).expect("a table"), "handle\tname\tid\nana\t-\tacc-1\n");
}

#[test]
fn a_failing_tracker_is_named_with_its_reason() {
    let lists = Lists {
        sources: vec![
            ListedInfo { source: "github".into(), ..ListedInfo::default() },
            ListedInfo { source: "linear".into(), error: Some("Linear answered 429".into()), ..ListedInfo::default() },
        ],
        ..Lists::default()
    };
    assert_eq!(failure_lines(&lists), ["Linear: Linear answered 429"]);
}

#[test]
fn an_issue_reads_as_its_title_details_body_and_comments() {
    let read = ReadIssue {
        issue: IssueInfo {
            key: "#7".into(),
            title: "Fix the login".into(),
            url: "https://github.com/acme/shop/issues/7".into(),
            branch: "issue-7-fix-the-login".into(),
            ..IssueInfo::default()
        },
        info: vec!["open".into(), "bug".into()],
        body: "It **breaks**.\n".into(),
        comments: vec![CommentInfo { author: "bo".into(), created_at: "2026-09-19".into(), body: "Same here".into() }],
        ..ReadIssue::default()
    };
    assert_eq!(
        read_text(&read),
        "#7  Fix the login\nopen · bug\nhttps://github.com/acme/shop/issues/7\nbranch issue-7-fix-the-login\n\nIt **breaks**.\n\n@bo · 2026-09-19\nSame here\n"
    );
}
