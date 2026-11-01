use serde_json::{Value, json};

use super::machines::{VPS, in_state, reach, start_listing, wait_for_machine};
use super::*;

const EMAIL: &str = "ana@example.com";
const INITIALIZE: &str = r#"{"type":"control_response","response":{"subtype":"success","request_id":"initialize","response":{"account":{"email":"ana@example.com","organization":"Acme","apiProvider":"firstParty"}}}}"#;
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
    assert_eq!(out.status.code(), Some(3), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Claude Code · max plan  updated"), "{text}");
    assert!(text.contains("  usage unavailable: codex exited without answering"), "{text}");

    let out = session.cli(&["usage", "--cached", "--json"]);
    assert_eq!(out.status.code(), Some(3), "{out:?}");
    let plans: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(plans["agents"][0]["limits"][0]["resets_at"], "2026-10-04T13:59:59.000Z");
    assert!(plans["agents"][1]["error"].is_string(), "{plans}");
    assert_eq!(std::fs::read_to_string(runs).expect("runs").lines().count(), 1);
}

fn probes(session: &Session, initialize: &str) -> Value {
    let (claude, codex) = (session.dir.join("claude-probe"), session.dir.join("codex-probe"));
    write_executable(&claude, &format!("#!/bin/sh\nread -r a\nread -r b\n{initialize}echo '{CLAUDE_ANSWER}'\n"));
    write_executable(&codex, "#!/bin/sh\nexit 1\n");
    json!({"claude": claude, "codex": codex})
}

fn linked_with_probes() -> (Arc<Session>, Arc<Session>) {
    let machine = Session::new();
    let there = json!({"agent_commands": probes(&machine, &format!("echo '{INITIALIZE}'\n"))});
    std::fs::write(machine.dir.join("config.json"), there.to_string()).expect("write the machine's config");
    let here = Session::new();
    start_listing(&here, &[(VPS, VPS)], &[(VPS, &reach(&machine))], json!({"agent_commands": probes(&here, "")}));
    wait_for_machine(&here, VPS, "the machine is linked", in_state("linked"));
    (machine, here)
}

fn claude_in(agents: &Value) -> Value {
    let agents = agents.as_array().expect("the agents");
    agents.iter().find(|plan| plan["agent"] == "claude").cloned().expect("claude")
}

#[test]
fn every_machine_s_limits_come_with_this_computer_s_and_an_identity_but_no_email() {
    let (machine, here) = linked_with_probes();

    let out = here.cli(&["usage", "--machine", "all", "--json"]);

    assert_eq!(out.status.code(), Some(3), "{out:?}");
    let every: Value = serde_json::from_slice(&out.stdout).expect("json");
    let there = &every["machines"][0];
    assert_eq!((&there["name"], &there["state"], &there["error"]), (&json!(VPS), &json!("linked"), &json!(null)));
    let (mine, theirs) = (claude_in(&every["agents"]), claude_in(&there["agents"]));
    assert_eq!((&mine["plan"], &theirs["plan"]), (&json!("max"), &json!("max")));
    assert!(mine.get("identity").is_none() && theirs["identity"].is_string(), "{every}");
    for session in [&here, &machine] {
        session.wait_for_file("server.log", "the request is logged", |log| log.contains("command=usage"));
    }
    let logs = [&here, &machine].map(|session| std::fs::read_to_string(session.dir.join("server.log")).expect("log"));
    for text in [String::from_utf8_lossy(&out.stdout).to_string(), logs.concat()] {
        assert!(!text.contains(EMAIL) && !text.contains("Acme"), "{text}");
    }
}

#[test]
fn a_machine_s_stats_are_read_by_its_name_and_an_unknown_one_is_not_found() {
    let (_machine, here) = linked_with_probes();

    let vps: Value = serde_json::from_str(&here.says(&["stats", "--machine", VPS, "--json"])).expect("json");
    let summed = here.says(&["stats", "--machine", "all"]);
    let nope = here.cli(&["stats", "--machine", "nope"]);

    assert_eq!((&vps["machine"]["name"], &vps["machine"]["state"]), (&json!(VPS), &json!("linked")));
    assert!(vps["windows"]["total"]["prompts"].is_number(), "{vps}");
    assert!(summed.ends_with("summed from:\n▭ this computer\n● vps  read just now"), "{summed}");
    let refused = String::from_utf8_lossy(&nope.stderr);
    assert!(!nope.status.success() && refused.contains("no machine is called `nope`"), "{nope:?}");
}
