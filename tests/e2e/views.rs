use serde_json::{Value, json};

use super::commands::{Followed, new_tab};
use super::*;

pub(super) fn size_of(session: &Session, pane: &str, when: &str) -> String {
    let marker = format!("size-{when}");
    let command = format!("echo {marker}-$(stty size | tr ' ' x)");
    session.says(&["send", "--pane", pane, "--enter", &command]);
    let seen =
        session.says(&["wait", "--pane", pane, "--text", &format!("^{marker}-[0-9]+x[0-9]+$"), "--timeout", "10"]);
    seen.rsplit('-').next().unwrap_or_default().to_string()
}

#[test]
fn a_view_sizes_its_tab_and_moves_without_the_window_and_gives_both_back_when_it_ends() {
    let app = Harness::start();
    let (pane, tab) = new_tab(&app);
    let window = app.session.report().shown;
    let before = size_of(&app.session, &pane, "before");

    let mut opened = app.session.spawn(&["view", "--cols", "100", "--rows", "30", "--tab", &tab]);
    let mut followed = Followed::of(&mut opened);
    followed.until("the view's first line", |line| line["view"]["controls"] == true && line["revision"] == 1);
    let id = followed.seen[0]["view"]["id"].as_u64().expect("an id").to_string();
    let sized = size_of(&app.session, &pane, "during");
    let moved = app.session.says(&["new-tab", "--view", &id, "--focus"]);
    followed.until("the view moves", |line| line["view"]["projection"] == 2);
    let inside = app.session.says(&["status", "--view", &id]);
    let unchanged = app.session.report().shown == window;
    let mut streaming = app.session.spawn(&["surface", "--view", &id]);
    let mut frames = Followed::of(&mut streaming);
    frames.until("a frame of the view's tab", |frame| frame["view"].as_u64() == id.parse().ok());

    let _ = opened.kill();
    let _ = opened.wait();
    app.session
        .wait_for_report("the view closes", |_| app.session.cli(&["status", "--view", &id]).status.code() != Some(0));
    let after = size_of(&app.session, &pane, "after");

    assert_eq!(sized, "30x100");
    assert!(unchanged, "the window kept showing what it showed");
    assert!(inside.contains("(view)"), "{inside}");
    assert_ne!(moved, "");
    assert_ne!(before, "30x100");
    assert_eq!(after, before);
    assert_eq!(finish_within(streaming, TIMEOUT).status.code(), Some(0));
}

pub(super) fn opened(session: &Session, tab: &str) -> (std::process::Child, Followed, String) {
    let mut opened = session.spawn(&["view", "--cols", "80", "--rows", "24", "--tab", tab]);
    let mut followed = Followed::of(&mut opened);
    followed.until("the view's first line", |line| line["revision"] == 1);
    let id = followed.seen[0]["view"]["id"].as_u64().expect("an id").to_string();
    (opened, followed, id)
}

#[test]
fn commands_with_a_view_pass_the_feature_checks_they_need() {
    let app = Harness::start();
    let (pane, tab) = new_tab(&app);
    let (mut opened, _followed, id) = opened(&app.session, &tab);

    let hidden = app.session.says(&["view-set", "--view", &id, "--hidden"]);
    let visible = app.session.says(&["view-set", "--view", &id, "--visible"]);
    let mut streaming = app.session.spawn(&["surface", "--view", &id, "--patches"]);
    let mut frames = Followed::of(&mut streaming);
    frames.until("the view's first frame", |frame| frame["revision"] == 1);
    app.session.says(&["send", "--pane", &pane, "--enter", "echo patch-\"\"ok"]);
    frames.until("a patch", |frame| frame.get("base").is_some());
    let profiled = app.session.says(&["new-tab", "--view", &id, "--profile", ""]);
    let revealed: Value = serde_json::from_str(
        &app.session.says(&["focus", "--view", &id, "--pane", &pane, "--reveal", "patch-ok", "--json"]),
    )
    .expect("json");
    let _ = opened.kill();
    let _ = opened.wait();

    assert!(hidden.ends_with("  hidden"), "{hidden}");
    assert!(!visible.contains("hidden"), "{visible}");
    assert_eq!(frames.seen[0]["view"].as_u64(), id.parse().ok());
    assert!(frames.seen[0].get("base").is_none(), "the first frame is complete");
    assert_ne!(profiled, "");
    assert_eq!(revealed["revealed"], true);
    assert!(revealed.get("projection").is_some(), "{revealed}");
    assert_eq!(finish_within(streaming, TIMEOUT).status.code(), Some(0));
}

#[test]
fn go_with_a_token_nobody_gave_fails_and_says_so() {
    let app = Harness::start();

    let out = app.session.cli(&["go", "cc-nope-1"]);

    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("there is no notice"), "{out:?}");
}

#[test]
fn a_flow_run_focused_in_a_view_moves_the_view_and_not_the_window() {
    let session = Session::new();
    let config = json!({
        "worktrees_dir": session.dir.join("worktrees"),
        "agent_commands": {"claude": fake_trusting_agent(&session.dir)},
    });
    std::fs::write(session.dir.join("config.json"), config.to_string()).expect("write the config");
    let started = session.cold(&["start-server"]);
    assert!(started.status.success(), "start-server failed: {started:?}");
    let repo = repository(&session.dir, "shop");
    let project = session.says(&["open", &repo.display().to_string()]);
    let mut opened = session.spawn(&["view", "--cols", "120", "--rows", "40"]);
    let mut followed = Followed::of(&mut opened);
    followed.until("the view's first line", |line| line["revision"] == 1);
    let id = followed.seen[0]["view"]["id"].as_u64().expect("an id").to_string();
    let window = session.report().shown;

    let args = ["flow", "run", "Quick", "--prompt", "add dark mode", "--project", &project, "--focus"];
    let run = session.says(&[&args[..], &["--view", &id, "--json"]].concat());
    let run: Value = serde_json::from_str(&run).expect("the run");
    let inside = session.says(&["status", "--view", &id, "--json"]);
    let inside: Value = serde_json::from_str(&inside).expect("the status");
    let _ = opened.kill();
    let _ = opened.wait();

    assert_eq!(inside["view"]["shown"]["tab"], run["members"][0]["tab"], "{inside}");
    assert_eq!(inside["view"]["projection"], run["projection"], "{inside}");
    assert_eq!(session.report().shown, window, "the window kept showing what it showed");
}
