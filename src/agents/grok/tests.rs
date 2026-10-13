use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::json;

use super::*;
use crate::agents::Known;
use crate::agents::activity::Activity;
use crate::agents::context::counting::{Usage, tests::sum};
use crate::agents::context::{Context, Look, Pane, tests::pane_look};
use crate::agents::stats::Tokens;
use crate::test_util::{Sleeper, TempDir, fixture, wait_until};

fn home_with(files: &[(&str, &str)]) -> (TempDir, PathBuf) {
    let home = TempDir::new();
    let cwd = home.path().join("project");
    std::fs::create_dir_all(&cwd).expect("project");
    let dir = home.path().join("sessions").join(session::encode_path(&cwd.to_string_lossy())).join("sess");
    std::fs::create_dir_all(&dir).expect("session");
    for (name, text) in files {
        std::fs::write(dir.join(name), text).expect("file");
    }
    (home, cwd)
}

fn bind(home: &Path, pid: i32, cwd: &Path, start: u64) {
    let row = json!([{ "session_id": "sess", "pid": pid, "pid_start": start, "cwd": cwd }]);
    std::fs::write(home.join("active_sessions.json"), row.to_string()).expect("active sessions");
}

fn running(home: &Path, cwd: &Path) -> Sleeper {
    Sleeper::named(&home.join("grok"), cwd, &[("GROK_HOME", &home.to_string_lossy())])
}

fn seen(home: &Path, cwd: &Path) -> (Sleeper, session::Session) {
    let sleeper = running(home, cwd);
    let start = crate::process::started(sleeper.pid()).unwrap_or(0);
    bind(home, sleeper.pid(), cwd, start);
    let found = session::look(sleeper.pid()).expect("the session");
    (sleeper, found)
}

fn prompt_count(session: &session::Session) -> usize {
    session.counted.iter().filter(|record| record.usage == Usage::Prompt).count()
}

fn tokens_of(session: &session::Session) -> Tokens {
    session.counted.iter().chain(session.extras.iter().flat_map(|(_, records)| records)).fold(
        Tokens::default(),
        |mut sum, record| {
            if let Usage::Completed(tokens) = record.usage {
                sum.input += tokens.input;
                sum.out += tokens.out;
                sum.cache_read += tokens.cache_read;
                sum.cache_write += tokens.cache_write;
            }
            sum
        },
    )
}

#[test]
fn a_path_is_percent_encoded_the_way_the_session_folder_is() {
    assert_eq!(
        session::encode_path("/home/user/dir with space/café"),
        "%2Fhome%2Fuser%2Fdir%20with%20space%2Fcaf%C3%A9"
    );
}

#[test]
fn the_pane_matches_its_foreground_pid_and_keeps_a_slug_folder() {
    let home = TempDir::new();
    let cwd = home.path().join("work");
    std::fs::create_dir_all(&cwd).expect("work");
    let slug = home.path().join("sessions").join("slug-not-the-encoding");
    let dir = slug.join("sess");
    std::fs::create_dir_all(&dir).expect("session");
    std::fs::write(slug.join(".cwd"), cwd.to_string_lossy().as_bytes()).expect("cwd file");
    std::fs::write(dir.join("summary.json"), json!({"info": {"id": "sess", "cwd": cwd}}).to_string()).expect("summary");
    std::fs::write(dir.join("events.jsonl"), "").expect("events");
    let sleeper = running(home.path(), &cwd);
    let start = crate::process::started(sleeper.pid()).unwrap_or(7);
    bind(home.path(), sleeper.pid(), &cwd, start);

    let found = session::look(sleeper.pid()).expect("slug session");

    assert_eq!(found.path, dir);
    if let Some(actual) = crate::process::started(sleeper.pid()) {
        bind(home.path(), sleeper.pid(), &cwd, actual.saturating_add(1));
        assert!(session::look(sleeper.pid()).is_none());
    }
}

#[test]
fn a_finished_turn_is_idle_with_its_model_and_one_call_of_context() {
    let (home, cwd) = home_with(&[
        ("events.jsonl", fixture!("grok/1.0.52/events.jsonl")),
        ("updates.jsonl", fixture!("grok/1.0.52/updates.jsonl")),
        ("chat_history.jsonl", fixture!("grok/1.0.52/chat_history.jsonl")),
        ("summary.json", fixture!("grok/1.0.52/summary.json")),
        ("signals.json", fixture!("grok/1.0.52/signals.json")),
    ]);
    let (_sleeper, found) = seen(home.path(), &cwd);

    assert_eq!(found.activity, Activity::Idle);
    assert_eq!(found.context, Some(Context { model: "grok-4.7".into(), percent: Some(8) }));
    assert_eq!(prompt_count(&found), 1);
    assert_eq!(tokens_of(&found), Tokens { input: 19456, out: 12, cache_read: 1024, cache_write: 8 });
    assert!(found.resumable);
    let said = session::last_message(&found.path).expect("read").expect("message");
    assert_eq!(said.text, "Hi.");
    assert!(said.written.is_some());
}

#[test]
fn permission_and_a_question_wait_and_a_finished_turn_does_not() {
    let events = r#"{"ts":"2026-10-13T21:10:21.697Z","type":"turn_started","turn_number":0,"model_id":"grok-4.7","session_relationship":"primary"}
{"ts":"2026-10-13T21:14:00.562Z","type":"phase_changed","phase":"permission_prompt"}
{"ts":"2026-10-13T21:14:00.562Z","type":"permission_requested","tool_name":"write"}
"#;
    let (home, cwd) = home_with(&[("events.jsonl", events), ("summary.json", r#"{"current_model_id":"grok-4.7"}"#)]);
    let (sleeper, found) = seen(home.path(), &cwd);
    assert_eq!(found.activity, Activity::Waiting);
    let asked = r#"{"ts":"2026-10-13T21:15:05.647Z","type":"permission_resolved","tool_name":"write","decision":"allow","wait_ms":1}
{"ts":"2026-10-13T21:15:05.647Z","type":"phase_changed","phase":"tool_execution"}
{"ts":"2026-10-13T21:15:05.700Z","type":"tool_started","tool_name":"ask_user_question"}
{"ts":"2026-10-13T21:15:05.764Z","type":"permission_resolved","tool_name":"ask_user_question","decision":"allow","wait_ms":0}
"#;
    std::fs::write(found.path.join("events.jsonl"), format!("{events}{asked}")).expect("events");
    assert_eq!(session::look(sleeper.pid()).expect("asked").activity, Activity::Waiting);
    std::fs::write(
        found.path.join("events.jsonl"),
        format!(
            "{events}{asked}{{\"ts\":\"2026-10-13T21:15:54.437Z\",\"type\":\"turn_ended\",\"outcome\":\"cancelled\"}}\n"
        ),
    )
    .expect("ended");
    let ended = session::look(sleeper.pid()).expect("ended");
    assert_eq!((ended.activity, ended.turn_open), (Activity::Idle, false));
}

#[test]
fn a_title_asks_only_while_the_turn_is_open() {
    let (home, cwd) = home_with(&[(
        "events.jsonl",
        r#"{"ts":"2026-10-13T21:10:21.697Z","type":"turn_started","turn_number":0,"model_id":"grok-4.7","session_relationship":"primary"}
"#,
    )]);
    let sleeper = running(home.path(), &cwd);
    let start = crate::process::started(sleeper.pid()).unwrap_or(0);
    bind(home.path(), sleeper.pid(), &cwd, start);
    let look = Look { title: "⚠ Action Required - grok".into(), ..pane_look(sleeper.pid()) };
    let mut pane = Pane::watching(sleeper.pid(), Known::Grok, UNIX_EPOCH);
    wait_until("the open turn waits", || {
        pane.update(Some(Known::Grok), &look);
        pane.activity(&look.title) == Some(Activity::Waiting)
    });
    std::fs::write(
        session_dir(home.path(), &cwd).join("events.jsonl"),
        r#"{"ts":"2026-10-13T21:10:21.697Z","type":"turn_started","turn_number":0,"model_id":"grok-4.7","session_relationship":"primary"}
{"ts":"2026-10-13T21:10:30.658Z","type":"turn_ended","outcome":"completed"}
"#,
    )
    .expect("ended");
    let titled = Look { title: "⠙ - Waiting - grok".into(), ..pane_look(sleeper.pid()) };
    wait_until("a finished turn stays idle", || {
        pane.update(Some(Known::Grok), &titled);
        pane.activity(&titled.title) == Some(Activity::Idle)
    });
}

fn session_dir(home: &Path, cwd: &Path) -> PathBuf {
    home.join("sessions").join(session::encode_path(&cwd.to_string_lossy())).join("sess")
}

#[test]
fn compaction_and_a_model_change_clear_the_percentage() {
    let updates = format!(
        "{}\n{}",
        fixture!("grok/1.0.52/updates.jsonl").trim_end(),
        "{\"timestamp\":1791926822,\"method\":\"session/update\",\"params\":{\"update\":{\"sessionUpdate\":\"auto_compact_completed\",\"tokens_before\":25829,\"tokens_after\":17271}}}\n"
    );
    let (home, cwd) = home_with(&[
        ("events.jsonl", fixture!("grok/1.0.52/events.jsonl")),
        ("updates.jsonl", updates.as_str()),
        ("summary.json", fixture!("grok/1.0.52/summary.json")),
        ("signals.json", fixture!("grok/1.0.52/signals.json")),
    ]);
    let (sleeper, found) = seen(home.path(), &cwd);
    assert_eq!(found.context.as_ref().map(|ctx| ctx.percent), Some(None));
    std::fs::write(
        found.path.join("updates.jsonl"),
        r#"{"timestamp":1791927000,"method":"session/update","params":{"update":{"sessionUpdate":"turn_completed","prompt_id":"wide","usage":{"inputTokens":900000,"outputTokens":10,"cachedReadTokens":0,"cacheCreationTokens":0,"modelCalls":3}}}}
"#,
    )
    .expect("wide");
    std::fs::write(found.path.join("summary.json"), r#"{"current_model_id":"grok-4.6","context_window":256000}"#)
        .expect("model");
    let switched = session::look(sleeper.pid()).expect("switched");
    assert_eq!(switched.context, Some(Context { model: "grok-4.6".into(), percent: None }));
}

#[test]
fn a_side_turn_and_a_child_session_add_tokens_without_prompts() {
    let events = r#"{"ts":"2026-10-13T21:10:21.697Z","type":"turn_started","turn_number":0,"model_id":"grok-4.7","session_relationship":"worker"}
"#;
    let updates = r#"{"timestamp":1791925821,"method":"session/update","params":{"update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"hi"},"_meta":{"promptIndex":0}}}}
{"timestamp":1791925830,"method":"session/update","params":{"update":{"sessionUpdate":"turn_completed","prompt_id":"side","usage":{"inputTokens":100,"outputTokens":5,"cachedReadTokens":0,"cacheCreationTokens":1,"modelCalls":1}}}}
"#;
    let (home, cwd) = home_with(&[
        ("events.jsonl", events),
        ("updates.jsonl", updates),
        (
            "chat_history.jsonl",
            "{\"type\":\"user\",\"prompt_index\":0,\"content\":[{\"type\":\"text\",\"text\":\"hi\"}]}\n",
        ),
    ]);
    let child = session_dir(home.path(), &cwd).join("child");
    std::fs::create_dir_all(&child).expect("child");
    std::fs::write(child.join("updates.jsonl"), updates).expect("child updates");
    let (_sleeper, found) = seen(home.path(), &cwd);
    assert_eq!(prompt_count(&found), 1);
    assert_eq!(tokens_of(&found).out, 10);
    assert_eq!(found.extras.len(), 2);
}

#[test]
fn slash_and_bang_are_local_and_a_question_mark_is_not() {
    assert!(local_command("/session-info"));
    assert!(local_command("  !ls"));
    assert!(!local_command("?"));
}

#[test]
fn dangerous_aliases_become_always_approve() {
    let args = |list: &[&str]| list.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
    assert_eq!(normalized(&args(&["--yolo"])), args(&["--always-approve"]));
    assert_eq!(normalized(&args(&["--permission-mode", "bypassPermissions"])), args(&["--always-approve"]));
    assert_eq!(normalized(&args(&["--permission-mode=bypassPermissions"])), args(&["--always-approve"]));
    assert_eq!(normalized(&args(&["--permission-mode=dontAsk"])), args(&["--permission-mode", "dontAsk"]));
    assert_eq!(normalized(&args(&["--always-approve"])), args(&["--always-approve"]));
}

#[test]
fn counted_records_reach_the_ledger_once() {
    let (home, cwd) = home_with(&[
        ("events.jsonl", fixture!("grok/1.0.52/events.jsonl")),
        ("updates.jsonl", fixture!("grok/1.0.52/updates.jsonl")),
        ("chat_history.jsonl", fixture!("grok/1.0.52/chat_history.jsonl")),
        ("summary.json", fixture!("grok/1.0.52/summary.json")),
        ("signals.json", fixture!("grok/1.0.52/signals.json")),
    ]);
    let sleeper = running(home.path(), &cwd);
    let start = crate::process::started(sleeper.pid()).unwrap_or(0);
    bind(home.path(), sleeper.pid(), &cwd, start);
    let mut pane = Pane::watching(sleeper.pid(), Known::Grok, UNIX_EPOCH);
    let look = pane_look(sleeper.pid());
    wait_until("the prompt is counted", || {
        pane.update(Some(Known::Grok), &look);
        sum(pane.take_counted()).0 == 1
    });
    pane.update(Some(Known::Grok), &look);
    assert_eq!(sum(pane.take_counted()).0, 0);
}
