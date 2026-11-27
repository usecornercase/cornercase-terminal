use serde_json::{Value, json};

use super::commands::agent_app;
use super::*;

fn pending_once(session: &Session, pane: &str, ready: impl Fn(&std::process::Output) -> bool) -> std::process::Output {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let out = session.cli(&["read", "--pending", "--pane", pane, "--json"]);
        if ready(&out) {
            return out;
        }
        assert!(Instant::now() < deadline, "timed out waiting for the pending question: {out:?}");
        thread::sleep(POLL);
    }
}

fn asking(session: &Session) -> String {
    let pane = session.says(&["start", "claude", "--prompt", "please choose"]);
    session.wait_for_working();
    std::fs::write(session.claude_dir().join("finish"), "").expect("let the agent ask");
    assert_eq!(session.says(&["wait", "--pane", &pane, "--until", "waiting", "--timeout", "30"]), "waiting");
    pane
}

fn the_question(pane: &str) -> Value {
    let fixture = format!("{}/tests/fixtures/control/pending-question/v1/question.json", env!("CARGO_MANIFEST_DIR"));
    let mut expected: Value =
        serde_json::from_str(&std::fs::read_to_string(fixture).expect("the fixture")).expect("json");
    expected["pane"] = pane.parse::<u64>().expect("a pane id").into();
    expected
}

#[test]
fn a_waiting_agents_question_is_read_as_data_and_is_gone_once_answered() {
    let app = agent_app();
    let session = &app.session;
    let pane = asking(session);

    let asked = pending_once(session, &pane, |out| out.status.success() && out.stdout != b"null\n");
    let text = session.says(&["read", "--pending", "--pane", &pane]);
    session.says(&["keys", "--pane", &pane, "1", "enter"]);
    let answered = pending_once(session, &pane, |out| out.stdout == b"null\n");

    assert_eq!(serde_json::from_slice::<Value>(&asked.stdout).expect("json"), the_question(&pane));
    assert_eq!(
        text,
        "Database: Which database should the service use?\n  \
         1. Postgres - What the other services use\n  \
         2. SQLite - One file, no server"
    );
    assert_eq!(answered.status.code(), Some(0), "{answered:?}");
}

#[test]
fn a_shell_fails_with_the_reason_and_leaves_the_others_read() {
    let app = agent_app();
    let session = &app.session;
    let pane = asking(session);
    pending_once(session, &pane, |out| out.status.success() && out.stdout != b"null\n");
    let shell = session.report().projects[0].workspaces[0].tabs[0].panes[0].id.to_string();

    let alone = session.cli(&["read", "--pending", "--pane", &shell]);
    let both = session.cli(&["read", "--pending", "--pane", &pane, "--pane", &shell, "--json"]);

    assert!(
        refused(&alone, 1, &format!("pane {shell} runs no agent, and cornercase reads what an agent asks only")),
        "{alone:?}"
    );
    assert_eq!(both.status.code(), Some(3), "{both:?}");
    let panes: Value = serde_json::from_slice(&both.stdout).expect("json");
    assert_eq!(panes["panes"][0], the_question(&pane));
    assert_eq!(panes["panes"][1]["pane"], json!(shell.parse::<u64>().expect("a pane id")));
    assert!(panes["panes"][1]["error"].as_str().is_some_and(|error| error.contains("runs no agent")), "{panes}");
}
