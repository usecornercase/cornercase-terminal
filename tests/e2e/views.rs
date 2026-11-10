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
    let trusting = fake_trusting_agent(&session.dir);
    let config = json!({
        "worktrees_dir": session.dir.join("worktrees"),
        "agent_commands": agent_commands(&[("claude", &trusting)]),
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

fn is_message(line: &Value) -> bool {
    line["event"] == "message"
}

#[test]
fn notify_reaches_a_program_with_no_window() {
    let session = Session::new();
    let started = session.cold(&["start-server"]);
    assert!(started.status.success(), "start-server failed: {started:?}");
    let mut all = session.spawn(&["events", "--notices", "--json"]);
    session.wait_for_file("server.log", "it follows", |log| log.matches("command=events").count() == 1);
    let mut told = Followed::of(&mut all);

    session.says(&["notify", "the", "release", "build", "is", "ready"]);

    told.until("the message", is_message);
    let message = told.seen.iter().find(|line| is_message(line)).expect("a message");
    assert_eq!(
        (&message["kind"], &message["message"], &message["panes"]),
        (&json!("notify"), &json!("the release build is ready"), &json!([]))
    );
    assert!(message.get("project").is_none() && message.get("workspace").is_none(), "{message}");
    session.says(&["notify", "again"]);
    told.until("the second message", |line| is_message(line) && line["message"] == "again");
}

#[test]
fn notify_names_the_pane_it_ran_in_and_reaches_that_panes_reader() {
    let app = Harness::start();
    let pane = panes(app.session.report()).next().expect("a pane").id.to_string();
    let mut all = app.session.spawn(&["events", "--notices", "--json"]);
    let mut one = app.session.spawn(&["events", "--pane", &pane, "--notices", "--json"]);
    let mut plain = app.session.spawn(&["events", "--json"]);
    app.session.wait_for_file("server.log", "all follow", |log| log.matches("command=events").count() == 3);
    let (mut told, mut pane_told, mut silent) =
        (Followed::of(&mut all), Followed::of(&mut one), Followed::of(&mut plain));

    app.session.says(&["notify", "from", "nowhere"]);
    told.until("the message from outside", |line| is_message(line) && line["message"] == "from nowhere");
    let command = format!("{} notify from-the-pane", bin());
    app.session.says(&["send", "--pane", &pane, "--enter", &command]);

    pane_told.until("the pane's message", is_message);
    told.until("the pane's message", |line| is_message(line) && line["message"] == "from-the-pane");
    let heard = told.seen.iter().rfind(|line| is_message(line)).expect("a message");
    assert_eq!(heard["panes"], json!([pane.parse::<u64>().expect("a pane id")]));
    assert!(heard.get("workspace").is_some() && heard.get("pane").is_none(), "{heard}");
    let own: Vec<_> = pane_told.seen.iter().filter(|line| is_message(line)).collect();
    assert_eq!(own.len(), 1, "{own:?}");
    app.session.says(&["close", "--pane", &pane]);
    silent.until("the pane closes", |line| told_closed(line, &pane));
    assert!(silent.seen.iter().all(|line| !is_message(line)), "{:?}", silent.seen);
}

fn told_closed(line: &Value, pane: &str) -> bool {
    line["event"] == "closed"
        && line["kind"] == "pane"
        && line["pane"].as_u64().map(|id| id.to_string()).as_deref() == Some(pane)
}
