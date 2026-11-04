use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use super::*;

fn stamp(secs: u64) -> String {
    let days = i64::try_from(secs / 86_400).expect("days") + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted + 2) / 5 + 1;
    let month = if shifted < 10 { shifted + 3 } else { shifted - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    let (hour, minute, second) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.000Z")
}

fn written(session: &Session, project: &std::path::Path) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).expect("the clock").as_secs();
    let at = stamp(now - 2 * 86_400);
    let transcript = session.dir.join("c1.jsonl");
    let prompt = format!(
        r#"{{"type":"user","origin":{{"kind":"human"}},"timestamp":"{at}","message":{{"content":[{{"type":"text","text":"add a rate limit"}}]}}}}"#
    );
    std::fs::write(&transcript, prompt + "\n").expect("write the transcript");
    let line = serde_json::json!({
        "v": 1, "at": at, "event": "prompt", "to": null, "agent": "claude", "account": null,
        "conversation": "c1", "record": {"claude": transcript}, "pane": null, "boot": null,
        "project": {"label": "api", "path": project}, "workspace": {"label": "rate-limit", "branch": "feat/x", "path": project, "worktree": false},
        "tab": "claude", "pull_request": null
    });
    let journal = session.dir.join("server.journal");
    std::fs::create_dir_all(&journal).expect("the journal folder");
    std::fs::write(journal.join(format!("{}.jsonl", &at[..10])), format!("{line}\n")).expect("write the journal");
}

fn start(session: &Session, config: &str) {
    std::fs::write(session.dir.join("config.json"), config).expect("write the config");
    let started = session.cold(&["start-server"]);
    assert!(started.status.success(), "start-server failed: {started:?}");
}

#[test]
fn the_dossier_of_yesterday_lists_the_prompts_of_the_last_day_with_activity() {
    let session = Session::new();
    start(&session, "{}");
    written(&session, &session.dir);

    let text = session.says(&["recap", "--dossier", "yesterday"]);

    assert!(text.contains("## [c1] api › rate-limit"), "{text}");
    assert!(text.contains("\"add a rate limit\""), "{text}");
    let json: Value =
        serde_json::from_str(&session.says(&["recap", "--dossier", "yesterday", "--json"])).expect("json");
    assert!(json["places"]["c1"]["project"]["label"] == "api" && json["text"].is_null(), "{json}");
}

#[test]
fn a_recap_is_written_by_the_agent_and_names_its_places() {
    let session = Session::new();
    let claude = session.dir.join("claude-recap");
    let result = r###"{"type":"result","subtype":"success","is_error":false,"result":"## On you\n- [c1] asked \"add a rate limit\"","usage":{"input_tokens":3,"output_tokens":4}}"###;
    write_executable(&claude, &format!("#!/bin/sh\necho x >> \"$0.runs\"\ncat > /dev/null\necho '{result}'\n"));
    start(&session, &format!(r#"{{"agent_commands": {{"claude": "{}"}}, "recap_agent": "claude"}}"#, claude.display()));
    written(&session, &session.dir);

    let text = session.says(&["recap", "yesterday"]);

    assert!(text.contains("yesterday · "), "{text}");
    assert!(text.contains("- api › rate-limit › claude asked \"add a rate limit\""), "{text}");
    let again = session.says(&["recap", "yesterday", "--json"]);
    let json: Value = serde_json::from_str(&again).expect("json");
    assert_eq!(json["agent"], "claude");
    assert_eq!(std::fs::read_to_string(session.dir.join("claude-recap.runs")).expect("runs").lines().count(), 1);
    assert!(session.dir.join("server.recap").join("yesterday").join("recap.md").exists());
}

#[test]
fn the_contract_lists_the_recap_operation_and_feature() {
    let session = Session::new();
    start(&session, "{}");
    let contract: Value = serde_json::from_str(&session.says(&["contract", "--json"])).expect("json");
    let operations = contract["operations"].as_array().expect("operations");
    assert!(operations.iter().any(|op| op["name"] == "recap" && op["available"] == true), "{contract}");
    assert!(contract["features"].as_array().expect("features").iter().any(|f| f == "recap"), "{contract}");
}
