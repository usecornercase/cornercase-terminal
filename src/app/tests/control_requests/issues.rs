use serde_json::{Value, json};

use super::super::issue_list::{Setup, checkout, listing, secrets_file, setup, shortcut};
use super::*;
use crate::control::issues::{Lists, PeopleInfo, ReadIssue, SourceInfo, SourceState, Sources};
use crate::issues::secrets;
use crate::test_util::write_executable;

const STORY: &str = r#"{"id":482,"name":"Returns page crashes","app_url":"https://app.shortcut.com/acme/story/482",
    "updated_at":"2026-09-30T00:00:00Z"}"#;
const MEMBERS: &str = r#"[{"id":"u1","profile":{"mention_name":"ana","name":"Ana Ruiz"}}]"#;

fn call(s: &mut Setup, command: &str, args: &Value) -> std::result::Result<Value, Refused> {
    asked(&mut s.app, &s.rx, command, args)
}

fn good<T: serde::de::DeserializeOwned>(s: &mut Setup, command: &str, args: &Value) -> T {
    match call(s, command, args) {
        Ok(value) => serde_json::from_value(value).expect("an answer"),
        Err(e) => panic!("{command} refused: {} {}", e.code, e.message),
    }
}

fn bad(s: &mut Setup, command: &str, args: &Value) -> Refused {
    match call(s, command, args) {
        Ok(value) => panic!("{command} was taken: {value}"),
        Err(refused) => refused,
    }
}

fn sources(s: &mut Setup) -> Sources {
    good(s, "issue-sources", &json!({}))
}

fn info<'a>(sources: &'a Sources, id: &str) -> &'a SourceInfo {
    sources.sources.iter().find(|source| source.source == id).expect("a source")
}

fn keys(lists: &Lists) -> Vec<&str> {
    lists.issues.iter().map(|issue| issue.key.as_str()).collect()
}

fn gh_calls(s: &Setup) -> usize {
    std::fs::read_to_string(s.config.path().join("gh.args")).map_or(0, |text| text.lines().count())
}

fn with_token(s: &mut Setup, routes: Vec<(&'static str, u16, &'static str)>) -> crate::test_util::FakeHttp {
    let server = shortcut(s, routes);
    secrets::write(&secrets_file(s), "shortcut_token", "t0k").expect("save token");
    server
}

fn other_project(s: &mut Setup) -> TempDir {
    let dir = TempDir::new();
    s.app.add_project(dir.path().to_path_buf(), AREA).expect("a second project");
    dir
}

#[test]
fn the_sources_say_what_each_tracker_still_needs() {
    let mut s = setup(true, "");
    let got = sources(&mut s);
    assert_eq!(
        (info(&got, "github").state, info(&got, "github").from.as_deref()),
        (SourceState::Connected, Some("gh"))
    );
    assert_eq!(info(&got, "shortcut").missing, ["token"]);
    assert_eq!(info(&got, "jira").missing, ["site", "email", "token"]);
    assert_eq!(info(&got, "plane").missing, ["workspace", "token"]);
    assert_eq!(info(&got, "gitlab").missing, ["token"]);
    assert_eq!(info(&got, "jira").fields, ["assignee", "reporter"]);
    assert_eq!(got.project, s.app.projects[0].id);
}

#[test]
fn an_asana_token_for_several_workspaces_is_saved_and_says_to_choose_one() {
    let mut s = setup(true, "");
    let two = r#"{"data":{"gid":"u-me","name":"Ana","workspaces":[{"gid":"w-1","name":"Acme"},{"gid":"w-2","name":"Beta"}]}}"#;
    let server = crate::test_util::FakeHttp::start(vec![("GET /users/me", 200, two)]);
    s.app.issues.apis.asana = server.url();
    let secret = json!({"action": "secret", "entry": {"key": "asana", "value": "t0k"}});
    good::<Value>(&mut s, "config", &secret);
    let got = sources(&mut s);
    let asana = info(&got, "asana");
    assert_eq!(asana.state, SourceState::Failing);
    assert!(asana.reason.as_deref().is_some_and(|reason| reason.contains("Beta")), "{asana:?}");
}

#[test]
fn a_folder_that_is_not_a_repository_has_no_github() {
    let mut s = setup(false, "");
    let got = sources(&mut s);
    assert_eq!(
        (info(&got, "github").state, info(&got, "github").missing.clone()),
        (SourceState::Missing, vec!["repository".to_string()])
    );
}

#[test]
fn a_token_from_the_environment_says_so() {
    let mut s = setup(true, "");
    s.app.issues.env_tokens.insert(Source::Linear, "x".into());
    let got = sources(&mut s);
    let linear = info(&got, "linear");
    assert_eq!(
        (linear.state, linear.from.as_deref(), linear.env.as_deref()),
        (SourceState::Connected, Some("env"), Some("LINEAR_API_KEY"))
    );
}

#[test]
fn a_failing_list_marks_the_source_until_one_works_or_it_is_forgotten() {
    let mut s = setup(true, "");
    let broken = with_token(&mut s, Vec::new());
    drop(broken);
    s.app.issues.apis.shortcut = "http://127.0.0.1:1/api/v3".into();
    let lists: Lists = good(&mut s, "issue-list", &json!({"source": "shortcut"}));
    assert!(lists.sources[0].error.is_some(), "{lists:?}");
    let got = sources(&mut s);
    assert_eq!(info(&got, "shortcut").state, SourceState::Failing);
    assert!(info(&got, "shortcut").reason.is_some());

    s.app.forget_issues(Source::Shortcut);
    assert_eq!(info(&sources(&mut s), "shortcut").state, SourceState::Connected);

    let _server = shortcut(&mut s, Vec::new());
    let lists: Lists = good(&mut s, "issue-list", &json!({"source": "shortcut"}));
    assert_eq!(lists.sources[0].error, None);
    let got = sources(&mut s);
    assert_eq!(info(&got, "shortcut").state, SourceState::Connected);
    assert!(info(&got, "shortcut").account.is_some());
}

#[test]
fn the_places_mark_where_the_asker_is() {
    let mut s = setup(true, "");
    let _folder = other_project(&mut s);
    let got = sources(&mut s);
    assert_eq!(got.places.len(), 2);
    assert_eq!(got.places.iter().map(|place| place.here).collect::<Vec<_>>(), [true, false]);
    let pane = s.app.projects[1].workspaces[0].tabs[0].panes[0].id;
    let by = asked_by(&mut s.app, &s.rx, Some(pane), "issue-sources", &json!({})).expect("sources");
    let by: Sources = serde_json::from_value(by).expect("sources");
    assert_eq!(by.places.iter().map(|place| place.here).collect::<Vec<_>>(), [false, true]);
}

#[test]
fn a_list_answers_with_the_branch_start_would_use() {
    let mut s = listing();
    let lists: Lists = good(&mut s, "issue-list", &json!({"source": "github"}));
    assert_eq!(keys(&lists), ["#7", "#3"]);
    assert_eq!(lists.issues[0].branch, "issue-7-fix-the-login");
    assert_eq!(lists.sources[0].error, None);
}

#[test]
fn the_filters_reach_the_tracker() {
    let mut s = listing();
    let people = json!([{"person": {"handle": "bo"}}, "me"]);
    let _: Lists = good(&mut s, "issue-list", &json!({"source": "github", "closed": true, "people": people}));
    let args = std::fs::read_to_string(s.config.path().join("gh.args")).expect("gh arguments");
    assert!(args.contains("--state all") && args.contains("--assignee bo") && args.contains("--author @me"), "{args}");
}

#[test]
fn a_cached_list_asks_nobody() {
    let mut s = listing();
    let first: Lists = good(&mut s, "issue-list", &json!({"source": "github"}));
    let calls = gh_calls(&s);
    let again: Lists = good(&mut s, "issue-list", &json!({"source": "github", "cached": true}));
    assert_eq!((keys(&again), again.sources[0].cached, gh_calls(&s)), (keys(&first), true, calls));
    let other: Lists = good(&mut s, "issue-list", &json!({"source": "github", "cached": true, "closed": true}));
    assert_eq!((other.issues.len(), other.sources[0].cached), (0, false));
}

#[test]
fn a_failing_tracker_keeps_its_cached_list_and_says_why() {
    let mut s = listing();
    let _: Lists = good(&mut s, "issue-list", &json!({"source": "github"}));
    write_executable(&s.config.path().join("gh"), "#!/bin/sh\necho boom >&2\nexit 1\n");
    let lists: Lists = good(&mut s, "issue-list", &json!({"source": "github"}));
    assert_eq!((keys(&lists), lists.sources[0].cached), (vec!["#7", "#3"], true));
    assert!(lists.sources[0].error.as_deref().is_some_and(|e| e.contains("boom")), "{lists:?}");
}

#[test]
fn several_trackers_are_merged_newest_first() {
    let mut s = listing();
    let _server = with_token(&mut s, Vec::new());
    let lists: Lists = good(&mut s, "issue-list", &json!({}));
    let sources: Vec<&str> = lists.issues.iter().map(|issue| issue.source.as_str()).collect();
    assert_eq!(sources, ["shortcut", "github", "github"], "{lists:?}");
    assert_eq!(lists.sources.len(), 2);
}

#[test]
fn a_list_is_refused_for_what_it_cannot_mean() {
    let mut s = listing();
    let person = json!([{"person": {"handle": "bo"}}, "anyone"]);
    assert_eq!(bad(&mut s, "issue-list", &json!({"people": person})).code, "malformed");
    assert_eq!(bad(&mut s, "issue-list", &json!({"source": "bugzilla"})).code, "malformed");
    assert_eq!(bad(&mut s, "issue-list", &json!({"project": 9999})).code, "not-found");
}

#[test]
fn a_tracker_that_changes_while_it_is_listed_is_not_cached() {
    let mut s = listing();
    let _server = with_token(&mut s, Vec::new());
    let text = json!({"contract": 1, "command": "issue-list", "args": {"source": "shortcut"}}).to_string();
    s.app.request(CLIENT, &text, Some(AREA), Instant::now());
    s.app.forget_issues(Source::Shortcut);
    let mut got = Vec::new();
    wait_until("the answer", || {
        drain(&mut s.app, &s.rx);
        got.extend(s.app.take_answers());
        !got.is_empty()
    });
    let lists: Lists = serde_json::from_value(reply_of(&got[0].1).expect("a list")).expect("lists");
    assert!(lists.sources[0].error.as_deref().is_some_and(|e| e.contains("settings changed")), "{lists:?}");
    let cached: Lists = good(&mut s, "issue-list", &json!({"source": "shortcut", "cached": true}));
    assert_eq!(cached.issues.len(), 0);
}

#[test]
fn an_issue_is_read_with_its_body_comments_and_prompt() {
    let mut s = listing();
    let read: ReadIssue = good(&mut s, "issue-read", &json!({"issue": "#7"}));
    assert_eq!((read.issue.key.as_str(), read.body.as_str()), ("#7", "It **breaks**."));
    assert_eq!(read.comments[0].author, "bo");
    assert!(read.info.contains(&"open".to_string()) && !read.prompt.is_empty(), "{read:?}");
}

#[test]
fn an_issue_of_another_repository_is_refused() {
    let mut s = listing();
    let refusal = bad(&mut s, "issue-read", &json!({"issue": "https://github.com/other/repo/issues/7"}));
    assert_eq!(refusal.code, "failed");
    assert!(refusal.message.contains("not an issue of this project"), "{}", refusal.message);
}

#[test]
fn an_issue_of_a_tracker_that_is_not_connected_is_a_conflict() {
    let mut s = listing();
    let refusal = bad(&mut s, "issue-read", &json!({"issue": "ENG-1", "source": "linear"}));
    assert_eq!(refusal.code, "conflict");
    assert!(refusal.message.contains("Linear is not connected"), "{}", refusal.message);
}

#[test]
fn the_people_a_filter_can_name_are_listed_and_cached() {
    let mut s = listing();
    let server = with_token(&mut s, vec![("GET /api/v3/members", 200, MEMBERS)]);
    let people: PeopleInfo = good(&mut s, "issue-people", &json!({"source": "shortcut"}));
    assert_eq!((people.people[0].handle.as_str(), people.fields[0].as_str()), ("ana", "owner"));
    let calls = server.requests().len();
    let again: PeopleInfo = good(&mut s, "issue-people", &json!({"source": "shortcut", "cached": true}));
    assert_eq!((again.people, again.cached, server.requests().len()), (people.people, true, calls));
    assert_eq!(bad(&mut s, "issue-people", &json!({"source": "linear"})).code, "conflict");
}

#[test]
fn start_with_an_issue_works_in_a_worktree_named_after_it_and_sends_the_prompt() {
    let mut s = listing();
    let done: Done = good(&mut s, "start", &json!({"issue": "#7"}));
    assert!(done.ids.pane.is_some());
    assert_eq!(s.app.projects[0].workspaces.len(), 2);
    let workspace = &s.app.projects[0].workspaces[1];
    assert_eq!(workspace.name.as_deref(), Some("#7 Fix the login"));
    assert_eq!(workspace.tabs.len(), 1);
    wait_until("the agent got the prompt", || {
        drain(&mut s.app, &s.rx);
        s.app.refresh(Instant::now());
        std::fs::read_to_string(checkout(&s).join("got")).is_ok_and(|got| got.contains('u'))
    });
    let again: Done = good(&mut s, "start", &json!({"issue": "#7"}));
    assert!(again.ids.pane.is_some() && again.ids.pane != done.ids.pane);
    assert_eq!((s.app.projects[0].workspaces.len(), s.app.projects[0].workspaces[1].tabs.len()), (2, 2));
}

#[test]
fn an_issue_that_is_not_tied_to_a_repository_asks_where() {
    let mut s = listing();
    let _folder = other_project(&mut s);
    let _server = with_token(&mut s, vec![("GET /api/v3/stories/482", 200, STORY)]);
    let refusal = bad(&mut s, "start", &json!({"issue": "sc-482"}));
    assert_eq!(refusal.code, "conflict");
    assert!(
        refusal.message.contains("(where you are)") && refusal.message.contains("--project ID"),
        "{}",
        refusal.message
    );
    assert_eq!(s.app.projects[0].workspaces.len(), 1);

    let project = s.app.projects[0].id;
    let done: Done = good(&mut s, "start", &json!({"issue": "sc-482", "project": project}));
    assert!(done.ids.pane.is_some());
    let name = s.app.projects[0].workspaces[1].name.clone();
    assert_eq!(name.as_deref(), Some("sc-482 Returns page crashes"));
    let checkout = std::fs::read_dir(s.worktrees.path().join(s.app.projects[0].path.file_name().expect("name")))
        .expect("worktrees")
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(checkout, ["sc-482-returns-page-crashes"]);
}

#[test]
fn an_issue_in_a_workspace_opens_a_tab_named_after_it() {
    let mut s = listing();
    let _folder = other_project(&mut s);
    let _server = with_token(&mut s, vec![("GET /api/v3/stories/482", 200, STORY)]);
    let workspace = s.app.projects[1].workspaces[0].id;
    let done: Done = good(&mut s, "start", &json!({"issue": "sc-482", "workspace": workspace}));
    assert!(done.ids.pane.is_some());
    let tabs = &s.app.projects[1].workspaces[0].tabs;
    assert_eq!(tabs.len(), 2);
    assert_eq!(tabs[1].label(&s.app.config), "sc-482 Returns page crashes");
}

#[test]
fn an_issue_and_a_prompt_are_not_both_taken() {
    let mut s = listing();
    assert_eq!(bad(&mut s, "start", &json!({"issue": "#7", "prompt": "do it"})).code, "malformed");
    assert_eq!(bad(&mut s, "start", &json!({"source": "github"})).code, "malformed");
    assert_eq!(bad(&mut s, "flow-run", &json!({"name": "Quick", "issue": "#7", "prompt": "x"})).code, "malformed");
    assert_eq!(s.app.projects[0].workspaces.len(), 1);
}

#[test]
fn a_lookup_that_hangs_times_out_naming_what_it_waits_for() {
    let mut s = setup(true, "sleep 30");
    let refusal = bad(&mut s, "start", &json!({"issue": "#7", "timeout": 1}));
    assert!(refusal.message.contains("is still looking the issue up"), "{}", refusal.message);
}

#[test]
fn a_client_that_leaves_during_the_lookup_opens_nothing() {
    let mut s = setup(
        true,
        "sleep 1\ncat <<'EOF'\n{\"number\":7,\"title\":\"Fix\",\"state\":\"OPEN\",\"labels\":[],\"author\":{\"login\":\"a\"},\"updatedAt\":\"\",\"url\":\"u\"}\nEOF",
    );
    let text = json!({"contract": 1, "command": "start", "args": {"issue": "#7"}}).to_string();
    s.app.request(CLIENT, &text, Some(AREA), Instant::now());
    s.app.forget(CLIENT);
    let event = s.rx.recv_timeout(Duration::from_secs(20)).expect("the lookup answers");
    s.app.handle_event(event, AREA).expect("handle event");
    drain(&mut s.app, &s.rx);
    assert_eq!((s.app.projects[0].workspaces.len(), s.app.take_answers().len()), (1, 0));
}

#[test]
fn the_contract_lists_the_issues_feature() {
    let mut s = listing();
    let described = good::<Value>(&mut s, "contract", &json!({}));
    assert!(described["features"].as_array().expect("features").contains(&json!("issues")));
}
