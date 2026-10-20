use std::fmt::Write as _;
use std::time::Duration;

use super::*;
use crate::changes::Mode;
use crate::test_util::git;

fn started(repo: &TempDir) -> (App, Receiver<AppEvent>) {
    let (mut app, rx) = app_in(repo.path(), no_config());
    app.clients.change_streams.set_every(Duration::ZERO);
    (app, rx)
}

fn summary_until(app: &mut App, rx: &Receiver<AppEvent>, what: &str, wanted: impl Fn(&Value) -> bool) -> Value {
    let mut found = None;
    wait_until(what, || {
        drain(app, rx);
        app.refresh(Instant::now());
        found = lines(app).into_iter().rfind(|line| wanted(line)).or(found.take());
        found.is_some()
    });
    found.expect("a summary")
}

fn ready(line: &Value) -> bool {
    line["state"] == "ready"
}

fn paths(line: &Value) -> Vec<String> {
    let files = line["workspaces"].as_array().into_iter().flatten().flat_map(|w| w["files"].as_array().expect("files"));
    files.map(|f| f["path"].as_str().expect("a path").to_string()).collect()
}

fn call_later(
    app: &mut App,
    rx: &Receiver<AppEvent>,
    command: &str,
    args: &Value,
) -> std::result::Result<Value, Refused> {
    let text = json!({"contract": 1, "command": command, "args": args}).to_string();
    app.request(CLIENT, &text, Some(AREA), Instant::now());
    let mut reply = None;
    wait_until("the request is answered", || {
        drain(app, rx);
        app.refresh(Instant::now());
        reply = app.take_answers().into_iter().next().map(|(_, text)| text);
        reply.is_some()
    });
    reply_of(&reply.expect("an answer"))
}

fn topic_repo() -> TempDir {
    let repo = git_repo(&[("a.txt", "one\ntwo\n")]);
    git(repo.path(), &["checkout", "--quiet", "-b", "topic"]);
    std::fs::write(repo.path().join("b.txt"), "bee\n").expect("write");
    git(repo.path(), &["add", "--all"]);
    git(repo.path(), &["commit", "--quiet", "-m", "b"]);
    std::fs::write(repo.path().join("a.txt"), "one\nTWO\n").expect("edit");
    repo
}

fn numbered(lines: usize) -> String {
    (1..=lines).fold(String::new(), |mut text, n| {
        let _ = writeln!(text, "line {n}");
        text
    })
}

fn workspace_id(app: &App) -> u64 {
    app.changes_target().expect("a git workspace").workspace
}

#[test]
fn a_stream_answers_at_once_then_sends_the_changed_files() {
    let repo = repo_with_edit();
    let (mut app, rx) = started(&repo);

    let first = ok(&mut app, "changes", &json!({}));

    assert_eq!((first["stream"].clone(), first["revision"].clone()), (json!(1), json!(1)));
    assert_eq!(first["boot"], wire::server_token());
    let line = summary_until(&mut app, &rx, "the files arrive", ready);
    let file = &line["workspaces"][0]["files"][0];
    assert_eq!(
        (&file["path"], &file["status"], &file["added"], &file["removed"], &file["fold"], &file["viewed"]),
        (&json!("a.txt"), &json!("modified"), &json!(1), &json!(1), &json!("open"), &json!(false))
    );
    assert_eq!(file["digest"].as_str().map(str::len), Some(16));
    assert!(line["revision"].as_u64() > Some(1));
    assert_eq!(line["workspaces"][0]["added"], 1);
}

#[test]
fn the_mode_of_a_stream_is_its_own_and_the_window_s_stays() {
    let repo = topic_repo();
    let (mut app, rx) = started(&repo);
    ok(&mut app, "changes", &json!({}));
    let line = summary_until(&mut app, &rx, "uncommitted", ready);
    assert_eq!(paths(&line), ["a.txt"]);

    ok(&mut app, "changes-mode", &json!({"stream": 1, "mode": "commits"}));
    let line = summary_until(&mut app, &rx, "commits", |l| ready(l) && l["mode"] == "commits");
    assert_eq!(paths(&line), ["b.txt"]);
    assert_eq!(line["workspaces"][0]["base"], "main");

    ok(&mut app, "changes-mode", &json!({"stream": 1, "mode": "all"}));
    let line = summary_until(&mut app, &rx, "all", |l| ready(l) && l["mode"] == "all");
    assert_eq!(paths(&line), ["a.txt", "b.txt"]);
    assert_eq!(app.panels.changes.mode(), Mode::Uncommitted);
}

#[test]
fn a_rename_has_its_old_path_and_its_patch_pairs_it() {
    let body = numbered(12);
    let repo = git_repo(&[("a.txt", &body)]);
    git(repo.path(), &["mv", "a.txt", "c.txt"]);
    std::fs::write(repo.path().join("c.txt"), body.replace("line 3", "LINE 3")).expect("edit");
    let (mut app, rx) = started(&repo);

    let patch = call_later(&mut app, &rx, "changes-file", &json!({"path": "c.txt"})).expect("a patch");
    assert_eq!((&patch["status"], &patch["old_path"]), (&json!("renamed"), &json!("a.txt")));
    assert_eq!(patch["hunks"].as_array().map(Vec::len), Some(1));

    wait_until("the window's changes load", || {
        drain(&mut app, &rx);
        app.refresh(Instant::now());
        app.changes_target().is_some_and(|t| app.panels.changes.model(t.workspace, t.base.as_deref()).is_some())
    });
    let first = ok(&mut app, "changes", &json!({}));
    assert_eq!((&first["state"], &first["workspaces"][0]["files"][0]["old_path"]), (&json!("ready"), &json!("a.txt")));
}

#[test]
fn an_untracked_file_is_listed_and_its_patch_is_all_added() {
    let repo = git_repo(&[("a.txt", "one\n")]);
    std::fs::write(repo.path().join("new.txt"), "x\ny\n").expect("write");
    let (mut app, rx) = started(&repo);

    let patch = call_later(&mut app, &rx, "changes-file", &json!({"path": "new.txt"})).expect("a patch");

    assert_eq!(patch["status"], "untracked");
    let kinds: Vec<&Value> = patch["hunks"][0]["lines"].as_array().expect("lines").iter().map(|l| &l["kind"]).collect();
    assert_eq!(kinds, [&json!("added"), &json!("added")]);
    assert_eq!(patch["hunks"][0]["lines"][1]["new"], 2);
}

#[test]
fn a_lockfile_is_folded_but_has_lines_and_a_binary_file_has_none() {
    let repo = git_repo(&[("Cargo.lock", "a\nb\n")]);
    std::fs::write(repo.path().join("Cargo.lock"), "a\nB\n").expect("edit");
    std::fs::write(repo.path().join("pic.bin"), b"\0\0\0\0").expect("write");
    let (mut app, rx) = started(&repo);
    ok(&mut app, "changes", &json!({}));
    let line = summary_until(&mut app, &rx, "both files", |l| ready(l) && paths(l).len() == 2);
    let folds: Vec<(String, String)> = line["workspaces"][0]["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|f| (f["path"].as_str().expect("path").into(), f["fold"].as_str().expect("fold").into()))
        .collect();
    assert_eq!(folds, [("Cargo.lock".into(), "lockfile".into()), ("pic.bin".into(), "binary".into())]);

    let lock = ok(&mut app, "changes-file", &json!({"path": "Cargo.lock"}));
    assert_eq!(lock["hunks"].as_array().map(Vec::len), Some(1));
    let binary = ok(&mut app, "changes-file", &json!({"path": "pic.bin"}));
    assert_eq!(binary["hunks"], json!([]));
}

#[test]
fn a_patch_carries_emphasis_and_a_style_table() {
    let repo = git_repo(&[("a.rs", "fn main() {\n    say(1);\n}\n")]);
    std::fs::write(repo.path().join("a.rs"), "fn main() {\n    say(2);\n}\n").expect("edit");
    let (mut app, rx) = started(&repo);
    ok(&mut app, "changes", &json!({}));
    summary_until(&mut app, &rx, "the files arrive", ready);

    let patch = ok(&mut app, "changes-file", &json!({"path": "a.rs"}));

    assert_eq!(patch["styles"][0], json!({}));
    let lines = patch["hunks"][0]["lines"].as_array().expect("lines");
    let removed = lines.iter().find(|l| l["kind"] == "removed").expect("removed");
    assert_eq!(removed["emphasis"], json!([[8, 9]]));
    assert!(removed["syntax"].as_array().is_some_and(|s| !s.is_empty()));
}

#[test]
fn viewed_marks_are_shared_and_cleared_by_a_change() {
    let repo = repo_with_edit();
    let (mut app, rx) = started(&repo);
    ok(&mut app, "changes", &json!({}));
    let line = summary_until(&mut app, &rx, "the files arrive", ready);
    let digest = line["workspaces"][0]["files"][0]["digest"].as_str().expect("digest").to_string();

    let missing = refused(&mut app, "changes-viewed", &json!({"path": "a.txt"}));
    assert_eq!(missing.code, "malformed");
    ok(&mut app, "changes-viewed", &json!({"path": "a.txt", "digest": "0000000000000000"}));
    ok(&mut app, "changes-viewed", &json!({"path": "a.txt", "digest": digest}));
    let line = summary_until(&mut app, &rx, "marked", |l| l["workspaces"][0]["files"][0]["viewed"] == true);
    assert_eq!(line["workspaces"][0]["files"][0]["digest"], digest.as_str());
    let workspace = workspace_id(&app);
    let target = app.changes_target().expect("target");
    let model = app.panels.changes.model(workspace, target.base.as_deref()).expect("model");
    assert!(app.panels.changes.viewed(workspace, &model.diff().expect("diff").files[0]));

    std::fs::write(repo.path().join("a.txt"), "one\nTWO and more\n").expect("edit again");
    summary_until(&mut app, &rx, "the mark is gone", |l| {
        l["workspaces"][0]["files"][0]["viewed"] == false && l["workspaces"][0]["files"][0]["digest"] != digest.as_str()
    });

    ok(&mut app, "changes-viewed", &json!({"path": "a.txt", "clear": true}));
}

#[test]
fn the_windows_mark_shows_in_a_stream() {
    let repo = repo_with_edit();
    let (mut app, rx) = started(&repo);
    wait_until("the changes load", || {
        drain(&mut app, &rx);
        app.refresh(Instant::now());
        app.changes_target().is_some_and(|t| app.panels.changes.model(t.workspace, t.base.as_deref()).is_some())
    });
    let workspace = workspace_id(&app);
    let file = app.changes_target().and_then(|t| app.changed_diff(&t)).expect("diff").files[0].clone();
    app.panels.changes.toggle_viewed(workspace, &file);

    let first = ok(&mut app, "changes", &json!({}));

    assert_eq!(
        (paths(&first), &first["workspaces"][0]["files"][0]["viewed"]),
        (vec!["a.txt".to_string()], &json!(true))
    );
}

#[test]
fn every_project_groups_by_sidebar_order_and_leaves_out_clean_and_plain_folders() {
    let (shop, _worktree) = shop_with_topic_worktree();
    let api = repo_with_edit();
    let clean = git_repo(&[]);
    let plain = TempDir::new();
    let (mut app, rx) = started(&shop);
    for path in [api.path(), clean.path(), plain.path()] {
        app.add_project(path.to_path_buf(), AREA).expect("project");
    }

    let first = ok(&mut app, "changes", &json!({"scope": "every"}));
    assert_eq!(first["scope"], "every");
    let line = summary_until(&mut app, &rx, "three workspaces", |l| {
        ready(l) && l["workspaces"].as_array().is_some_and(|w| w.len() == 3)
    });

    let owners: Vec<u64> =
        line["workspaces"].as_array().expect("list").iter().map(|w| w["project"].as_u64().expect("project")).collect();
    assert_eq!(owners, [app.projects[0].id, app.projects[0].id, app.projects[1].id]);
    assert_eq!(app.screen.side, None);
}

#[test]
fn a_workspace_nobody_follows_costs_nothing() {
    let (one, two, three) = (repo_with_edit(), repo_with_edit(), repo_with_edit());
    let (mut app, rx) = started(&one);
    app.add_project(two.path().to_path_buf(), AREA).expect("project");
    app.add_project(three.path().to_path_buf(), AREA).expect("project");
    let (second, third) = (app.projects[1].workspaces[0].id, app.projects[2].workspaces[0].id);

    ok(&mut app, "changes", &json!({"workspace": second}));
    summary_until(&mut app, &rx, "the files arrive", ready);
    for _ in 0..3 {
        drain(&mut app, &rx);
        app.refresh(Instant::now());
    }

    assert!(app.panels.changes.model_in(second, Mode::Uncommitted, None).is_some());
    assert!(app.panels.changes.model_in(third, Mode::Uncommitted, None).is_none());
}

#[test]
fn the_stream_ends_when_its_project_closes_and_a_client_that_leaves_drops_its_model() {
    let repo = topic_repo();
    let (mut app, rx) = started(&repo);
    ok(&mut app, "changes", &json!({"mode": "commits"}));
    let workspace = workspace_id(&app);
    summary_until(&mut app, &rx, "commits", ready);
    assert!(app.panels.changes.model_in(workspace, Mode::Commits, None).is_some());

    app.forget(CLIENT);
    assert!(app.panels.changes.model_in(workspace, Mode::Commits, None).is_none());

    ok(&mut app, "changes", &json!({}));
    let project = app.projects[0].id;
    ok(&mut app, "close", &json!({"item": {"project": project}}));
    app.refresh(Instant::now());
    assert!(streamed(&mut app).contains(&Streamed::End));
}

#[test]
fn requests_refuse_what_cannot_be_followed() {
    let repo = repo_with_edit();
    let plain = TempDir::new();
    let (mut app, _rx) = started(&repo);
    let outside = app.add_project(plain.path().to_path_buf(), AREA).expect("project");
    let outside = app.projects[outside].workspaces[0].id;

    assert_eq!(refused(&mut app, "changes", &json!({"workspace": outside})).code, "conflict");
    assert_eq!(refused(&mut app, "changes", &json!({"scope": "every", "mode": "commits"})).code, "malformed");
    assert_eq!(refused(&mut app, "changes-mode", &json!({"stream": 7, "mode": "all"})).code, "not-found");
    assert_eq!(refused(&mut app, "changes-mode", &json!({"mode": "all"})).code, "malformed");
    ok(&mut app, "changes", &json!({}));
    assert_eq!(refused(&mut app, "changes", &json!({})).code, "conflict");
    assert_eq!(refused(&mut app, "changes-viewed", &json!({"path": "a.txt", "digest": "zz"})).code, "malformed");
}

#[test]
fn a_missing_file_and_a_gap_out_of_range_are_not_found() {
    let repo = repo_with_edit();
    let (mut app, rx) = started(&repo);

    let none = call_later(&mut app, &rx, "changes-file", &json!({"path": "nothing.txt"})).expect_err("refused");
    assert_eq!(none.code, "not-found");
    let gap = call_later(&mut app, &rx, "changes-gap", &json!({"path": "a.txt", "hunk": 1})).expect_err("refused");
    assert_eq!(gap.code, "not-found");
    assert!(gap.message.contains("has 1 hunks"), "{}", gap.message);
}

#[test]
fn a_gap_has_the_unchanged_lines_between_two_hunks() {
    let body = numbered(40);
    let repo = git_repo(&[("a.txt", &body)]);
    std::fs::write(repo.path().join("a.txt"), body.replace("line 2\n", "LINE 2\n").replace("line 35\n", "LINE 35\n"))
        .expect("edit");
    let (mut app, rx) = started(&repo);

    let gap = call_later(&mut app, &rx, "changes-gap", &json!({"path": "a.txt", "hunk": 1})).expect("a gap");

    let lines = gap["lines"].as_array().expect("lines");
    assert_eq!((&lines[0]["old"], &lines[0]["new"], &lines[0]["text"]), (&json!(6), &json!(6), &json!("line 6")));
    assert_eq!(lines.last().expect("last")["new"], 31);
}

#[test]
fn the_base_lists_branches_and_is_set_and_cleared() {
    let repo = topic_repo();
    let (mut app, rx) = started(&repo);
    let workspace = workspace_id(&app);

    let listed = call_later(&mut app, &rx, "changes-base", &json!({})).expect("a list");
    assert_eq!(listed["default"], "main");
    assert!(listed["branches"].as_array().expect("branches").contains(&json!("topic")));
    assert_eq!(listed["base"], Value::Null);

    let set = call_later(&mut app, &rx, "changes-base", &json!({"base": "topic"})).expect("set");
    assert_eq!(set["base"], "topic");
    let (p, w) = app.projects.workspace_index(workspace).map(|at| (at.project(), at.index())).expect("workspace");
    assert_eq!(app.projects[p].workspaces[w].base.as_deref(), Some("topic"));
    call_later(&mut app, &rx, "changes-base", &json!({"base": "main"})).expect("the default");
    assert_eq!(app.projects[p].workspaces[w].base, None);
    call_later(&mut app, &rx, "changes-base", &json!({"base": "topic"})).expect("set again");
    call_later(&mut app, &rx, "changes-base", &json!({"clear": true})).expect("clear");
    assert_eq!(app.projects[p].workspaces[w].base, None);
    let unknown = call_later(&mut app, &rx, "changes-base", &json!({"base": "nope"})).expect_err("unknown");
    assert_eq!(unknown.code, "not-found");
    assert_eq!(refused(&mut app, "changes-base", &json!({"base": "topic", "clear": true})).code, "malformed");
}

#[test]
fn status_counts_the_windows_workspace_and_the_followed_ones_only() {
    let (one, two, three) = (repo_with_edit(), repo_with_edit(), repo_with_edit());
    let (mut app, rx) = started(&one);
    app.add_project(two.path().to_path_buf(), AREA).expect("project");
    app.add_project(three.path().to_path_buf(), AREA).expect("project");
    let second = app.projects[1].workspaces[0].id;
    ok(&mut app, "changes", &json!({"workspace": second}));
    summary_until(&mut app, &rx, "the files arrive", ready);
    pump_until(&mut app, &rx, "the window's workspace loads", |a| {
        a.report(None).projects[0].workspaces[0].changes.is_some()
    });

    let report = app.report(None);

    let counts: Vec<Option<u64>> = report.projects.iter().map(|p| p.workspaces[0].changes.map(|c| c.files)).collect();
    assert_eq!(counts, [Some(1), Some(1), None]);
}

#[test]
fn a_stream_can_only_be_changed_by_the_connection_that_opened_it() {
    let repo = topic_repo();
    let (mut app, _rx) = started(&repo);
    ok(&mut app, "changes", &json!({}));
    let text = json!({"contract": 1, "command": "changes-mode", "args": {"stream": 1, "mode": "all"}}).to_string();

    app.request(CLIENT + 1, &text, Some(AREA), Instant::now());
    let (_, reply) = app.take_answers().remove(0);

    let refusal = reply_of(&reply).expect_err("refused");
    assert_eq!(refusal.code, "not-found");
    assert_eq!(app.clients.change_streams.mode_of(1), Some(Mode::Uncommitted));
    ok(&mut app, "changes-mode", &json!({"stream": 1, "mode": "all"}));
    assert_eq!(app.clients.change_streams.mode_of(1), Some(Mode::All));
}
