use serde_json::Value;

use super::*;

const CLAUDE_ANSWER: &str = r#"{"type":"control_response","response":{"subtype":"success","request_id":"usage","response":{"subscription_type":"max","rate_limits_available":true,"rate_limits":{"limits":[{"kind":"session","percent":42,"severity":"normal","resets_at":"2026-10-04T13:59:59+00:00"}]}}}}"#;

#[test]
fn the_plan_limits_are_read_through_the_server_and_then_from_its_cache() {
    let session = Session::new();
    let claude = session.dir.join("claude-probe");
    let runs = session.dir.join("claude-probe.runs");
    write_executable(
        &claude,
        &format!("#!/bin/sh\necho run >> \"$0.runs\"\nread -r a\nread -r b\necho '{CLAUDE_ANSWER}'\n"),
    );
    let codex = session.dir.join("codex-probe");
    write_executable(&codex, "#!/bin/sh\nexit 1\n");
    let config =
        format!(r#"{{"agent_commands": {{"claude": "{}", "codex": "{}"}}}}"#, claude.display(), codex.display());
    std::fs::write(session.dir.join("config.json"), config).expect("write the config");
    let started = session.cold(&["start-server"]);
    assert!(started.status.success(), "start-server failed: {started:?}");

    let out = session.cli(&["usage"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Claude Code · max plan  updated"), "{text}");
    assert!(text.contains("  usage unavailable: codex exited without answering"), "{text}");

    let out = session.cli(&["usage", "--cached", "--json"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let plans: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(plans["agents"][0]["limits"][0]["resets_at"], "2026-10-04T13:59:59.000Z");
    assert!(plans["agents"][1]["error"].is_string(), "{plans}");
    assert_eq!(std::fs::read_to_string(runs).expect("runs").lines().count(), 1);
}
