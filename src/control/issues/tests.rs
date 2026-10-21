use serde_json::json;

use super::*;
use crate::control::contract::unknown_fields;
use crate::control::{Command, Request};
use crate::test_util::fixture;

fn issue(source: &str, key: &str, title: &str) -> IssueInfo {
    IssueInfo {
        source: source.into(),
        key: key.into(),
        number: 7,
        title: title.into(),
        url: format!("https://example.com/{key}"),
        state: "OPEN".into(),
        closed: false,
        labels: vec!["bug".into()],
        assignees: vec!["ana".into()],
        author: "bo".into(),
        updated_at: "2026-09-19T12:00:00Z".into(),
        branch: "issue-7-fix-the-login".into(),
    }
}

fn connected_sources() -> Vec<SourceInfo> {
    let blank = SourceInfo::default;
    vec![
        SourceInfo {
            source: "github".into(),
            name: "GitHub".into(),
            tab: true,
            repository: true,
            state: SourceState::Connected,
            from: Some("gh".into()),
            fields: ["assignee".into(), "author".into()],
            ..blank()
        },
        SourceInfo {
            source: "linear".into(),
            name: "Linear".into(),
            tab: true,
            state: SourceState::Connected,
            account: Some("@ana in acme".into()),
            from: Some("saved".into()),
            fields: ["assignee".into(), "creator".into()],
            people: [Who::Me, Who::Anyone],
            ..blank()
        },
    ]
}

fn unready_sources() -> Vec<SourceInfo> {
    let blank = SourceInfo::default;
    vec![
        SourceInfo {
            source: "jira".into(),
            name: "Jira".into(),
            state: SourceState::Missing,
            missing: vec!["site".into(), "email".into(), "token".into()],
            fields: ["assignee".into(), "reporter".into()],
            ..blank()
        },
        SourceInfo {
            source: "shortcut".into(),
            name: "Shortcut".into(),
            tab: true,
            state: SourceState::Failing,
            reason: Some("the token was rejected".into()),
            from: Some("env".into()),
            env: Some("SHORTCUT_API_TOKEN".into()),
            fields: ["owner".into(), "requester".into()],
            ..blank()
        },
    ]
}

fn sources() -> Sources {
    Sources {
        project: 1,
        closed: false,
        sources: [connected_sources(), unready_sources()].concat(),
        places: vec![
            PlaceInfo { project: 1, workspace: None, label: "web-shop".into(), worktree: true, here: true },
            PlaceInfo {
                project: 2, workspace: Some(7), label: "notes › default".into(), worktree: false, here: false
            },
        ],
    }
}

fn lists() -> Lists {
    Lists {
        project: 1,
        issues: vec![issue("github", "#7", "Fix the login")],
        sources: vec![
            ListedInfo { source: "github".into(), count: 1, cached: false, account: None, error: None },
            ListedInfo {
                source: "linear".into(),
                count: 0,
                cached: true,
                account: Some("@ana in acme".into()),
                error: Some("Linear answered 429".into()),
            },
        ],
    }
}

fn read() -> ReadIssue {
    ReadIssue {
        issue: issue("github", "#7", "Fix the login"),
        info: vec!["OPEN".into(), "bug".into()],
        body: "It **breaks**.".into(),
        comments: vec![CommentInfo {
            author: "bo".into(),
            created_at: "2026-09-19T12:00:00Z".into(),
            body: "Same here".into(),
        }],
        prompt: "https://example.com/#7".into(),
    }
}

fn people() -> PeopleInfo {
    PeopleInfo {
        source: "jira".into(),
        fields: ["assignee".into(), "reporter".into()],
        cached: false,
        people: vec![PersonInfo { handle: "ana".into(), name: "Ana Ruiz".into(), id: Some("acc-1".into()) }],
    }
}

fn written(value: &impl Serialize) -> String {
    serde_json::to_string_pretty(value).expect("json")
}

#[test]
fn the_answers_are_written_as_their_fixtures() {
    assert_eq!(written(&sources()), fixture!("control/issues/sources.json").trim_end());
    assert_eq!(written(&lists()), fixture!("control/issues/list.json").trim_end());
    assert_eq!(written(&read()), fixture!("control/issues/read.json").trim_end());
    assert_eq!(written(&people()), fixture!("control/issues/people.json").trim_end());
}

#[test]
fn the_fixtures_read_back() {
    assert_eq!(serde_json::from_str::<Sources>(fixture!("control/issues/sources.json")).expect("sources"), sources());
    assert_eq!(serde_json::from_str::<Lists>(fixture!("control/issues/list.json")).expect("list"), lists());
    assert_eq!(serde_json::from_str::<ReadIssue>(fixture!("control/issues/read.json")).expect("read"), read());
    assert_eq!(serde_json::from_str::<PeopleInfo>(fixture!("control/issues/people.json")).expect("people"), people());
}

#[test]
fn a_newer_server_may_add_keys_and_states() {
    let read: Sources = serde_json::from_value(json!({
        "project": 3,
        "sources": [{"source": "x", "state": "paused", "newer": 1}],
        "extra": true
    }))
    .expect("sources");
    assert_eq!((read.project, read.sources[0].state), (3, SourceState::Other));
}

#[test]
fn who_travels_as_a_word_or_an_object() {
    assert_eq!(serde_json::to_value(Who::Me).expect("json"), json!("me"));
    assert_eq!(serde_json::to_value(Who::Anyone).expect("json"), json!("anyone"));
    let person = Who::Person { handle: "bo".into(), id: None };
    assert_eq!(serde_json::to_value(&person).expect("json"), json!({"person": {"handle": "bo", "id": null}}));
    assert_eq!(serde_json::from_value::<Who>(json!({"person": {"handle": "bo"}})).expect("who"), person);
}

#[test]
fn who_converts_to_the_trackers_and_back() {
    let person = |handle: &str, id: Option<&str>| Who::Person { handle: handle.into(), id: id.map(Into::into) };
    assert_eq!(person("@bo", None).to_issues(), Ok(issues::Who::Person("bo".into())));
    assert_eq!(
        person("ana", Some("acc-1")).to_issues(),
        Ok(issues::Who::User { id: "acc-1".into(), name: "ana".into() })
    );
    assert_eq!(Who::Me.to_issues(), Ok(issues::Who::Me));
    assert!(person(" @ ", None).to_issues().is_err());
    let user = issues::Who::User { id: "acc-1".into(), name: "ana".into() };
    assert_eq!(Who::from(&user), person("ana", Some("acc-1")));
    assert_eq!(Who::from(&issues::Who::Person("bo".into())), person("bo", None));
}

#[test]
fn the_requests_read_from_their_documented_json() {
    let list: Command = serde_json::from_value(json!({
        "command": "issue-list",
        "args": {"source": "shortcut", "closed": true, "people": ["me", {"person": {"handle": "bo"}}]}
    }))
    .expect("list");
    let Command::IssueList(list) = list else { panic!("not a list") };
    assert_eq!(list.people, [Who::Me, Who::Person { handle: "bo".into(), id: None }]);
    assert!(list.closed && !list.cached);
    let Command::IssueRead(read) =
        serde_json::from_value(json!({"command": "issue-read", "args": {"issue": "#7"}})).expect("read")
    else {
        panic!("not a read")
    };
    assert_eq!((read.issue.as_str(), read.source), ("#7", None));
}

#[test]
fn one_person_is_not_a_pair() {
    assert!(serde_json::from_value::<IssueList>(json!({"people": ["me"]})).is_err());
}

#[test]
fn a_request_without_an_id_has_no_stray_field() {
    let raw = json!({"command": "issue-people", "args": {"source": "jira"}});
    let parsed: Request = serde_json::from_value(raw.clone()).expect("request");
    assert_eq!(unknown_fields(&raw, &parsed), None);
    let raw = json!({"command": "issue-people", "args": {"source": "jira", "bogus": 1}});
    let parsed: Request = serde_json::from_value(raw.clone()).expect("request");
    assert_eq!(unknown_fields(&raw, &parsed).as_deref(), Some("args.bogus"));
}

#[test]
fn the_logged_fields_name_kinds_and_sizes_never_texts() {
    let list = IssueList {
        source: Some("jira".into()),
        people: [Who::Person { handle: "secret-handle".into(), id: None }, Who::Me],
        ..IssueList::default()
    };
    let mut fields = Fields::default();
    list.fields(&mut fields);
    let logged = format!("{:?}", fields.0);
    assert!(logged.contains("person,me") && !logged.contains("secret-handle"), "{logged}");
    let read = IssueRead { issue: "ENG-4242".into(), ..IssueRead::default() };
    let mut fields = Fields::default();
    read.fields(&mut fields);
    assert!(fields.0.contains(&("bytes", "8".to_string())));
    assert!(!format!("{:?}", fields.0).contains("ENG-4242"));
}
