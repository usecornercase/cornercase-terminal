use serde_json::Value;

use super::*;

fn started(session: &Session, args: &[&str]) -> Value {
    let out = session.cold(&[&["start-server", "--json"], args].concat());
    assert!(out.status.success(), "start-server failed: {out:?}");
    serde_json::from_slice(&out.stdout).expect("a JSON answer")
}

fn pane_ids(session: &Session) -> Vec<u64> {
    panes(session.report()).map(|pane| pane.id).collect()
}

#[test]
fn a_server_started_without_a_window_runs_agents_and_a_window_finds_them_later() {
    let session = agent_session();

    let first = started(&session, &[]);
    let opened = pane_ids(&session);
    let again = started(&session, &["--cols", "200", "--rows", "60"]);

    assert_eq!(
        (&first["already"], &first["size"], &first["cols"], &first["rows"], &first["restored"]),
        (&false.into(), &"start-server".into(), &160.into(), &48.into(), &false.into())
    );
    assert_eq!((&again["already"], &again["cols"], &again["rows"]), (&true.into(), &160.into(), &48.into()));
    assert_eq!(pane_ids(&session), opened);
    assert_eq!(opened.len(), 1, "one shell opens in the current folder");

    assert_eq!(first["theme"]["from"], "default");
    let tab = session.says(&["new-tab", "--name", "build"]);
    let start = session.spawn(&["start", "claude", "--prompt", "fix the login", "--wait", "--timeout", "30"]);
    session.wait_for_working();
    std::fs::write(session.claude_dir().join("finish"), "").expect("let the agent finish");
    let out = Session::output(start);
    let [agent, ended] = out.lines().collect::<Vec<_>>()[..] else { panic!("not a pane and an ending: {out}") };
    let said = session.says(&["read", "--pane", agent, "--last-message"]);
    let before = pane_ids(&session);

    let mut app = Harness::open(Arc::clone(&session), ROWS, COLS);
    app.wait_for("the window shows the project and the agent's tab", |s| {
        s.contains(&first_entry()) && s.contains("claude") && s.contains("build")
    });

    let status: Value = serde_json::from_str(&session.says(&["status", "--json"])).expect("a status");
    assert_eq!(status["theme"]["from"], "window");
    assert_eq!((ended, said.as_str()), ("done", "said: fix the login"));
    assert!(before.contains(&tab.parse().expect("a pane id")), "{before:?}");
    assert_eq!(before.len(), 3);
    assert_eq!(pane_ids(&session), before, "the window opened no other shell");
    let log = std::fs::read_to_string(session.dir.join("server.log")).expect("the server log");
    let restores = log.matches("session restored").count() + log.matches("no session to restore").count();
    assert_eq!(restores, 1, "{log}");
}

#[test]
fn a_size_out_of_range_is_refused_before_any_server_starts() {
    let session = Session::new();

    let outs = [["--cols", "89"], ["--cols", "1001"], ["--rows", "9"], ["--rows", "501"]]
        .map(|size| session.cold(&[&["start-server"], &size[..]].concat()));

    for out in &outs {
        assert!(refused(out, 1, "must be between"), "{out:?}");
    }
    assert!(!session.socket().exists());
}

#[test]
fn a_cold_start_restores_the_session_the_last_server_saved() {
    let session = Session::new();
    started(&session, &[]);
    session.says(&["new-tab", "--name", "kept"]);
    session.wait_for_saved("the tab is saved", |saved| saved.contains("kept"));
    let out = session.run("kill-server");
    assert!(out.status.success(), "{out:?}");

    let again = started(&session, &[]);

    assert_eq!(
        (&again["already"], &again["restored"], &again["complete"]),
        (&false.into(), &true.into(), &true.into())
    );
    let status: Value = serde_json::from_str(&session.says(&["status", "--json"])).expect("a status");
    let tabs = status["projects"][0]["workspaces"][0]["tabs"].as_array().expect("tabs").clone();
    assert!(tabs.iter().any(|tab| tab["name"] == "kept"), "{status}");
    assert_eq!(pane_ids(&session).len(), 2);
}

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/theme/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn asks_for_the_background(session: &Session, pane: &str, marker: u32) {
    let probe = format!(
        "clear; stty -icanon -echo min 0 time 20; printf '\\033]11;?\\007'; cat -v; stty sane; echo; echo asked-\"\"{marker}"
    );
    session.says(&["send", "--pane", pane, "--enter", &probe]);
    session.says(&["wait", "--pane", pane, "--text", &format!("^asked-{marker}$"), "--timeout", "30"]);
}

#[test]
fn a_theme_given_at_start_answers_programs_and_a_change_follows() {
    let session = Session::new();

    let first = started(&session, &["--theme", &fixture("dark.json")]);
    let pane = session.says(&["new-tab", "--name", "probe"]);
    asks_for_the_background(&session, &pane, 1);
    let dark = session.says(&["read", "--pane", &pane]);
    let light: Value = serde_json::from_str(&session.says(&["theme", &fixture("light.json"), "--json"])).expect("json");
    asks_for_the_background(&session, &pane, 2);
    let seen = session.says(&["read", "--pane", &pane]);
    let status: Value = serde_json::from_str(&session.says(&["status", "--json"])).expect("a status");
    let cleared = session.says(&["theme", "--clear", "--json"]);

    assert_eq!(first["theme"], serde_json::json!({"from": "client", "scheme": "dark"}));
    assert!(dark.contains("rgb:1a1a/1b1b/2626"), "{dark}");
    assert_eq!(light, serde_json::json!({"from": "client", "scheme": "light"}));
    assert!(seen.contains("rgb:fbfb/fafa/f6f6"), "{seen}");
    assert_eq!(status["theme"], light);
    assert_eq!(
        serde_json::from_str::<Value>(&cleared).expect("json"),
        serde_json::json!({"from": "default", "scheme": null})
    );
}

#[test]
fn a_new_install_shows_the_tour_on_its_first_window() {
    let session = Session::new();
    std::fs::remove_file(session.dir.join("tour.json")).expect("a new install has no tour file");

    let mut app = Harness::open(Arc::clone(&session), ROWS, COLS);
    app.wait_for("the tour's card", |s| s.contains("Get started") && s.contains("Projects"));

    session.wait_for_file("tour.json", "the tour is marked as offered", |text| text.contains("\"offered\": true"));
}
