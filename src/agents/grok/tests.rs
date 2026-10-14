use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use serde_json::json;

use super::*;
use crate::agents::activity::Activity;
use crate::agents::context::counting::{Ledger, Usage, tests::sum};
use crate::agents::context::{Context, Look, Pane, tests::pane_look};
use crate::agents::stats::Tokens;
use crate::agents::{DEFAULT_TRUST_PROMPT, Known, TrustPrompt, asks_trust};
use crate::test_util::{Sleeper, TempDir, fixture, wait_until};
use crate::time::Stamp;

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
fn permission_a_question_and_a_cancelled_turn_come_from_the_session() {
    assert_eq!(activity_of(fixture!("grok/1.0.52/working-events.jsonl")), Activity::Working);
    assert_eq!(activity_of(fixture!("grok/1.0.52/permission-events.jsonl")), Activity::Waiting);
    assert_eq!(activity_of(fixture!("grok/1.0.52/question-events.jsonl")), Activity::Waiting);
    assert_eq!(activity_of(fixture!("grok/1.0.52/question-answered-events.jsonl")), Activity::Working);
    assert_eq!(activity_of(fixture!("grok/1.0.52/plan-open-events.jsonl")), Activity::Waiting);
    assert_eq!(activity_of(fixture!("grok/1.0.52/plan-continued-events.jsonl")), Activity::Working);
    assert_eq!(activity_of(fixture!("grok/1.0.52/exit-open-events.jsonl")), Activity::Waiting);
    assert_eq!(activity_of(fixture!("grok/1.0.52/exit-continued-events.jsonl")), Activity::Working);
    assert_eq!(activity_of(fixture!("grok/1.0.52/permission-late-resolution-events.jsonl")), Activity::Waiting);
    assert_eq!(activity_of(fixture!("grok/1.0.52/cancelled-events.jsonl")), Activity::Idle);
}

fn activity_of(events: &str) -> Activity {
    let (home, cwd) = home_with(&[("events.jsonl", events)]);
    seen(home.path(), &cwd).1.activity
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
    age(&session_dir(home.path(), &cwd).join("signals.json"), 1_791_925_000);
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
    let events = r#"{"ts":"2026-10-13T21:10:20.000Z","type":"turn_started","turn_number":0,"model_id":"grok-4.7","session_relationship":"worker"}
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
    assert_eq!(prompt_count(&found), 0);
    assert!(found.prompted.is_none());
    assert_eq!(tokens_of(&found).out, 10);
    assert_eq!(found.extras.len(), 2);
}

#[test]
fn repeated_indexes_confirm_the_later_submission() {
    let (home, cwd) = home_with(&[
        ("events.jsonl", fixture!("grok/1.0.52/repeated-prompt-index-events.jsonl")),
        ("updates.jsonl", fixture!("grok/1.0.52/repeated-prompt-index-updates.jsonl")),
        ("chat_history.jsonl", fixture!("grok/1.0.52/repeated-prompt-index-chat_history.jsonl")),
    ]);
    let found = seen(home.path(), &cwd).1;
    let mut times: Vec<_> =
        found.counted.iter().filter(|record| record.usage == Usage::Prompt).map(|record| record.at).collect();
    times.sort();
    assert_eq!(times.len(), 3);
    assert!(times.windows(2).all(|pair| pair[0] < pair[1]));
    let since = times[0] + Duration::from_millis(1);
    assert!(found.prompted.is_some_and(|at| at >= since));
    assert!(times[0] < since);
}

#[test]
fn a_later_multi_call_hides_the_older_one_call_percentage() {
    let (home, cwd) = home_with(&[
        ("updates.jsonl", fixture!("grok/1.0.52/old-single-call-context-updates.jsonl")),
        ("signals.json", fixture!("grok/1.0.52/old-single-call-context-signals.json")),
        ("summary.json", fixture!("grok/1.0.52/summary.json")),
        ("events.jsonl", fixture!("grok/1.0.52/events.jsonl")),
    ]);
    let found = seen(home.path(), &cwd).1;
    assert_eq!(found.context.as_ref().map(|ctx| ctx.percent), Some(None));
}

#[test]
fn switching_the_model_away_and_back_does_not_restore_the_percentage() {
    let (home, cwd) = home_with(&[
        ("events.jsonl", fixture!("grok/1.0.52/model-switch-away-and-back-events.jsonl")),
        ("updates.jsonl", fixture!("grok/1.0.52/model-switch-away-and-back-updates.jsonl")),
        ("summary.json", fixture!("grok/1.0.52/model-switch-away-and-back-summary.json")),
        ("signals.json", fixture!("grok/1.0.52/model-switch-away-and-back-signals.json")),
    ]);
    let (sleeper, found) = seen(home.path(), &cwd);
    assert_eq!(found.context.as_ref().map(|ctx| ctx.percent), Some(None));
    let first = fixture!("grok/1.0.52/model-switch-away-and-back-updates.jsonl").lines().next().expect("reply");
    std::fs::write(found.path.join("updates.jsonl"), format!("{first}\n")).expect("first reply");
    std::fs::write(found.path.join("summary.json"), r#"{"current_model_id":"grok-4.7","context_window":256000}"#)
        .expect("model");
    let shown = session::follow(sleeper.pid(), Some(found)).expect("first model");
    assert_eq!(shown.context, Some(Context { model: "grok-4.7".into(), percent: Some(8) }));
    std::fs::write(shown.path.join("summary.json"), r#"{"current_model_id":"grok-4.6","context_window":256000}"#)
        .expect("away");
    let away = session::follow(sleeper.pid(), Some(shown)).expect("away");
    assert_eq!(away.context, Some(Context { model: "grok-4.6".into(), percent: None }));
    std::fs::write(away.path.join("summary.json"), r#"{"current_model_id":"grok-4.7","context_window":256000}"#)
        .expect("back");
    let back = session::follow(sleeper.pid(), Some(away)).expect("back");
    assert_eq!(back.context, Some(Context { model: "grok-4.7".into(), percent: None }));
}

#[test]
fn a_summary_only_switch_back_does_not_restore_matching_signals() {
    let events = r#"{"ts":"2026-10-13T21:10:01.000Z","type":"turn_started","model_id":"grok-4.7","session_relationship":"primary"}
{"ts":"2026-10-13T21:10:10.000Z","type":"turn_ended","outcome":"completed"}
"#;
    let updates = r#"{"timestamp":1791925810,"params":{"update":{"sessionUpdate":"turn_completed","prompt_id":"one","usage":{"inputTokens":20480,"outputTokens":1,"cachedReadTokens":0,"cacheCreationTokens":0,"modelCalls":1}}}}
"#;
    let (home, cwd) = home_with(&[
        ("events.jsonl", events),
        ("updates.jsonl", updates),
        ("summary.json", r#"{"current_model_id":"grok-4.7","context_window":256000}"#),
        ("signals.json", r#"{"contextWindowTokens":256000,"contextTokensUsed":20480,"primaryModelId":"grok-4.7"}"#),
    ]);
    let signals = session_dir(home.path(), &cwd).join("signals.json");
    age(&signals, 1_791_925_900);
    let (sleeper, found) = seen(home.path(), &cwd);
    assert_eq!(found.context.as_ref().and_then(|ctx| ctx.percent), Some(8));
    std::fs::write(found.path.join("summary.json"), r#"{"current_model_id":"grok-4.6","context_window":256000}"#)
        .expect("away");
    let away = session::follow(sleeper.pid(), Some(found)).expect("away");
    assert_eq!(away.context.as_ref().and_then(|ctx| ctx.percent), None);
    std::fs::write(away.path.join("summary.json"), r#"{"current_model_id":"grok-4.7","context_window":256000}"#)
        .expect("back");
    let back = session::follow(sleeper.pid(), Some(away)).expect("back");
    assert_eq!(back.context.as_ref().and_then(|ctx| ctx.percent), None);
    assert_eq!(
        std::fs::metadata(&signals).expect("signals").modified().expect("mtime"),
        UNIX_EPOCH + Duration::from_secs(1_791_925_900)
    );
}

#[test]
fn a_cancelled_turn_without_text_keeps_the_earlier_answer_time() {
    let (home, cwd) = home_with(&[
        ("events.jsonl", fixture!("grok/1.0.52/last-message-after-cancel-events.jsonl")),
        ("updates.jsonl", fixture!("grok/1.0.52/last-message-after-cancel-updates.jsonl")),
        ("chat_history.jsonl", fixture!("grok/1.0.52/last-message-after-cancel-chat_history.jsonl")),
    ]);
    let found = seen(home.path(), &cwd).1;
    let said = session::last_message(&found.path).expect("read").expect("message");
    assert_eq!(said.text, "old assistant");
    let written = UNIX_EPOCH + Duration::from_secs(1_791_925_808);
    assert_eq!(said.written.as_deref(), Some(Stamp(written).to_string().as_str()));
}

#[test]
fn an_open_turn_behind_a_megabyte_reads_as_working() {
    let (home, cwd) = home_with(&[("events.jsonl", fixture!("grok/1.0.52/working-events.jsonl"))]);
    let path = session_dir(home.path(), &cwd).join("events.jsonl");
    grow_past_a_megabyte(&path);
    assert_eq!(seen(home.path(), &cwd).1.activity, Activity::Working);
}

#[test]
fn a_watched_turn_stays_open_after_it_grows_past_a_megabyte() {
    let (home, cwd) = home_with(&[("events.jsonl", fixture!("grok/1.0.52/working-events.jsonl"))]);
    let (sleeper, found) = seen(home.path(), &cwd);
    assert_eq!(found.activity, Activity::Working);
    let path = found.path.join("events.jsonl");
    grow_past_a_megabyte(&path);
    std::fs::write(
        &path,
        format!(
            "{}{{\"ts\":\"2026-10-13T21:16:00.000Z\",\"type\":\"permission_requested\",\"tool_name\":\"write\"}}\n",
            std::fs::read_to_string(&path).expect("events")
        ),
    )
    .expect("permission");
    let waiting = session::follow(sleeper.pid(), Some(found)).expect("grown");
    assert_eq!(waiting.activity, Activity::Waiting);
    std::fs::write(
        waiting.path.join("events.jsonl"),
        format!(
            "{}{{\"ts\":\"2026-10-13T21:16:30.000Z\",\"type\":\"turn_ended\",\"outcome\":\"cancelled\",\"cancellation_context\":{{\"trigger\":\"ctrl_c\"}}}}\n",
            std::fs::read_to_string(waiting.path.join("events.jsonl")).expect("events")
        ),
    )
    .expect("cancel");
    let cancelled = session::follow(sleeper.pid(), Some(waiting)).expect("cancelled");
    assert_eq!(cancelled.activity, Activity::Idle);
    std::fs::write(
        cancelled.path.join("events.jsonl"),
        format!(
            "{}{{\"ts\":\"2026-10-13T21:17:00.000Z\",\"type\":\"turn_started\",\"model_id\":\"grok-4.7\",\"session_relationship\":\"primary\"}}\n{{\"ts\":\"2026-10-13T21:17:05.000Z\",\"type\":\"turn_ended\",\"outcome\":\"completed\"}}\n",
            std::fs::read_to_string(cancelled.path.join("events.jsonl")).expect("events")
        ),
    )
    .expect("later");
    let later = session::follow(sleeper.pid(), Some(cancelled)).expect("later");
    assert_eq!((later.activity, later.turn_open), (Activity::Idle, false));
}

fn grow_past_a_megabyte(path: &Path) {
    let mut body = std::fs::read(path).expect("events");
    let line = b"{\"ts\":\"2026-10-13T21:10:22.100Z\",\"type\":\"phase_changed\",\"phase\":\"streaming_text\"}\n";
    while body.len() <= 1024 * 1024 {
        body.extend_from_slice(line);
    }
    std::fs::write(path, body).expect("grow");
}

#[test]
fn a_monitor_event_between_prompts_is_not_a_prompt() {
    let (home, cwd) = home_with(&[
        ("events.jsonl", fixture!("grok/1.0.52/events.jsonl")),
        ("updates.jsonl", fixture!("grok/1.0.52/synthetic-monitor-updates.jsonl")),
    ]);
    let found = seen(home.path(), &cwd).1;
    let times: Vec<_> =
        found.counted.iter().filter(|record| record.usage == Usage::Prompt).map(|record| record.at).collect();
    assert_eq!(times.len(), 2);
    assert!(found.prompted.is_some_and(|at| at == *times.iter().max().expect("later")));
    assert!(times.iter().all(|at| *at != UNIX_EPOCH + Duration::from_millis(1_791_925_810_100)));
}

#[test]
fn a_quoted_reminder_is_still_a_prompt() {
    let updates = r#"{"timestamp":1791925801,"params":{"_meta":{"agentTimestampMs":1791925801100},"update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"please read <system-reminder> and continue"},"_meta":{"promptIndex":0}}}}
"#;
    let (home, cwd) = home_with(&[("updates.jsonl", updates)]);
    assert_eq!(prompt_count(&seen(home.path(), &cwd).1), 1);
}

fn prompt_line(ms: u64, index: u64, text: &str, synthetic: Option<bool>) -> String {
    let mut update = json!({
        "sessionUpdate": "user_message_chunk",
        "content": {"type": "text", "text": text},
        "_meta": {"promptIndex": index}
    });
    if let Some(flag) = synthetic {
        update["_meta"]["syntheticPrompt"] = json!(flag);
    }
    let line = json!({
        "timestamp": ms / 1000,
        "params": {"_meta": {"agentTimestampMs": ms}, "update": update}
    });
    format!("{line}\n")
}

#[test]
fn an_explicit_human_tag_confirms_the_later_submission_on_follow_and_on_recovery() {
    let first = prompt_line(1_791_925_801_100, 0, "hello", Some(false));
    let (home, cwd) = home_with(&[("updates.jsonl", &first)]);
    let (sleeper, found) = seen(home.path(), &cwd);
    assert_eq!(prompt_count(&found), 1);
    let system =
        prompt_line(1_791_925_820_100, 1, "<system-reminder> is the literal XML tag I need explained", Some(false));
    let info = prompt_line(1_791_925_830_100, 2, "<user_info> is the literal tag I need explained", Some(false));
    let bare = prompt_line(1_791_925_840_100, 3, "<system-reminder>\nkeep going", None);
    let path = found.path.join("updates.jsonl");
    std::fs::write(&path, format!("{first}{system}{info}{bare}")).expect("append");
    let next = session::follow(sleeper.pid(), Some(found)).expect("appended");
    let times: Vec<_> =
        next.counted.iter().filter(|record| record.usage == Usage::Prompt).map(|record| record.at).collect();
    assert_eq!(times.len(), 3);
    let later = UNIX_EPOCH + Duration::from_millis(1_791_925_830_100);
    let since = (UNIX_EPOCH + Duration::from_millis(1_791_925_801_100)) + Duration::from_millis(1);
    assert_eq!(next.prompted, Some(later));
    assert!(next.prompted.is_some_and(|at| at >= since));
    assert!(times.iter().all(|at| *at != UNIX_EPOCH + Duration::from_millis(1_791_925_840_100)));
    let recovered = prompt_line(1_791_925_850_100, 4, "<user_info> recovered", Some(false));
    std::fs::write(&path, recovered).expect("reset");
    let again = session::follow(sleeper.pid(), Some(next)).expect("recovered");
    let recovered_at = UNIX_EPOCH + Duration::from_millis(1_791_925_850_100);
    assert_eq!(again.prompted, Some(recovered_at));
    assert!(again.counted.iter().any(|record| record.usage == Usage::Prompt && record.at == recovered_at));
}

#[test]
fn a_rewritten_chat_keeps_the_prompt_from_the_same_poll() {
    let updates = r#"{"timestamp":1791925801,"params":{"_meta":{"eventId":"human-A","agentTimestampMs":1791925801100},"update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"small prompt"},"_meta":{"promptIndex":0}}}}
"#;
    let (home, cwd) = home_with(&[
        ("updates.jsonl", updates),
        ("chat_history.jsonl", "{\"type\":\"user\",\"prompt_index\":0,\"content\":\"small prompt\"}\n"),
    ]);
    let (sleeper, found) = seen(home.path(), &cwd);
    assert_eq!(prompt_count(&found), 1);
    std::fs::write(
        found.path.join("chat_history.jsonl"),
        "{\"type\":\"user\",\"synthetic_reason\":\"compaction_meta\",\"content\":\"summary\"}\n{\"type\":\"user\",\"prompt_index\":1,\"content\":\"small prompt\"}\n",
    )
    .expect("rewrite");
    std::fs::write(
        found.path.join("updates.jsonl"),
        format!(
            "{updates}{{\"timestamp\":1791925820,\"params\":{{\"_meta\":{{\"eventId\":\"human-B\",\"agentTimestampMs\":1791925820100}},\"update\":{{\"sessionUpdate\":\"user_message_chunk\",\"content\":{{\"type\":\"text\",\"text\":\"small prompt\"}},\"_meta\":{{\"promptIndex\":1}}}}}}}}\n"
        ),
    )
    .expect("append");
    let next = session::follow(sleeper.pid(), Some(found)).expect("rewritten");
    assert_eq!(next.prompted, Some(UNIX_EPOCH + Duration::from_millis(1_791_925_820_100)));
    assert_eq!(prompt_count(&next), 2);
}

#[test]
fn a_completed_worker_turn_does_not_confirm_the_parent() {
    let events = r#"{"ts":"2026-10-13T21:10:01.000Z","type":"turn_started","model_id":"grok-4.7","session_relationship":"primary"}
{"ts":"2026-10-13T21:10:05.000Z","type":"turn_ended","outcome":"completed"}
{"ts":"2026-10-13T21:10:06.000Z","type":"turn_started","model_id":"grok-4.7","session_relationship":"worker"}
{"ts":"2026-10-13T21:10:10.000Z","type":"turn_ended","outcome":"completed"}
{"ts":"2026-10-13T21:10:20.000Z","type":"turn_started","model_id":"grok-4.7","session_relationship":"primary"}
{"ts":"2026-10-13T21:10:30.000Z","type":"turn_ended","outcome":"completed"}
"#;
    let updates = r#"{"timestamp":1791925801,"params":{"_meta":{"agentTimestampMs":1791925801100},"update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"one"},"_meta":{"promptIndex":0}}}}
{"timestamp":1791925801,"params":{"update":{"sessionUpdate":"turn_completed","prompt_id":"p","usage":{"inputTokens":1000,"outputTokens":1,"cachedReadTokens":0,"cacheCreationTokens":0,"modelCalls":1}}}}
{"timestamp":1791925806,"params":{"_meta":{"agentTimestampMs":1791925806100},"update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"worker"},"_meta":{"promptIndex":1}}}}
{"timestamp":1791925806,"params":{"update":{"sessionUpdate":"turn_completed","prompt_id":"w","usage":{"inputTokens":50,"outputTokens":2,"cachedReadTokens":0,"cacheCreationTokens":0,"modelCalls":1}}}}
{"timestamp":1791925820,"params":{"_meta":{"agentTimestampMs":1791925820100},"update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"two"},"_meta":{"promptIndex":0}}}}
{"timestamp":1791925820,"params":{"update":{"sessionUpdate":"turn_completed","prompt_id":"q","usage":{"inputTokens":2000,"outputTokens":3,"cachedReadTokens":0,"cacheCreationTokens":0,"modelCalls":1}}}}
"#;
    let (home, cwd) = home_with(&[
        ("events.jsonl", events),
        ("updates.jsonl", updates),
        ("summary.json", r#"{"current_model_id":"grok-4.7","context_window":256000}"#),
    ]);
    let found = seen(home.path(), &cwd).1;
    assert_eq!(prompt_count(&found), 2);
    assert_eq!(found.prompted, Some(UNIX_EPOCH + Duration::from_millis(1_791_925_820_100)));
    assert_eq!(tokens_of(&found).out, 6);
    assert_eq!(
        found.context.as_ref().and_then(|ctx| ctx.percent),
        Some(crate::agents::context::percent(2000, 256_000))
    );
}

#[test]
fn a_worker_completion_after_turn_ended_keeps_the_parent_percentage() {
    let events = r#"{"ts":"2026-10-13T21:10:01.000Z","type":"turn_started","model_id":"grok-4.7","session_relationship":"primary"}
{"ts":"2026-10-13T21:10:05.000Z","type":"turn_ended","outcome":"completed"}
{"ts":"2026-10-13T21:10:06.000Z","type":"turn_started","model_id":"grok-4.7","session_relationship":"worker"}
{"ts":"2026-10-13T21:10:10.000Z","type":"turn_ended","outcome":"completed"}
"#;
    let updates = r#"{"timestamp":1791925804,"params":{"_meta":{"agentTimestampMs":1791925804500},"update":{"sessionUpdate":"turn_completed","prompt_id":"parent","usage":{"inputTokens":20480,"outputTokens":1,"cachedReadTokens":0,"cacheCreationTokens":0,"modelCalls":1}}}}
{"timestamp":1791925810,"params":{"_meta":{"agentTimestampMs":1791925810027},"update":{"sessionUpdate":"turn_completed","prompt_id":"worker","usage":{"inputTokens":128000,"outputTokens":9,"cachedReadTokens":0,"cacheCreationTokens":0,"modelCalls":1}}}}
"#;
    let (home, cwd) = home_with(&[
        ("events.jsonl", events),
        ("updates.jsonl", updates),
        ("summary.json", r#"{"current_model_id":"grok-4.7","context_window":256000}"#),
    ]);
    let found = seen(home.path(), &cwd).1;
    let outs = |records: &[crate::agents::context::counting::Record]| -> u64 {
        records
            .iter()
            .map(|record| match record.usage {
                Usage::Completed(tokens) => tokens.out,
                _ => 0,
            })
            .sum()
    };
    assert_eq!(outs(&found.counted), 1);
    assert_eq!(found.extras.iter().map(|(_, records)| outs(records)).sum::<u64>(), 9);
    assert_eq!(found.context.as_ref().map(|ctx| ctx.model.as_str()), Some("grok-4.7"));
    assert_eq!(found.context.as_ref().and_then(|ctx| ctx.percent), Some(8));
}

#[test]
fn a_new_model_reply_in_the_same_poll_sets_the_percentage() {
    let events = r#"{"ts":"2026-10-13T21:10:01.000Z","type":"turn_started","model_id":"grok-4.7"}
{"ts":"2026-10-13T21:10:10.000Z","type":"turn_ended"}
"#;
    let updates = r#"{"timestamp":1791925810,"params":{"update":{"sessionUpdate":"turn_completed","prompt_id":"one","usage":{"inputTokens":20480,"outputTokens":1,"cachedReadTokens":0,"cacheCreationTokens":0,"modelCalls":1}}}}
"#;
    let (home, cwd) = home_with(&[
        ("events.jsonl", events),
        ("updates.jsonl", updates),
        ("summary.json", r#"{"current_model_id":"grok-4.7","context_window":256000}"#),
    ]);
    let (sleeper, found) = seen(home.path(), &cwd);
    assert_eq!(found.context.as_ref().and_then(|ctx| ctx.percent), Some(8));
    std::fs::write(
        found.path.join("events.jsonl"),
        format!(
            "{events}{{\"ts\":\"2026-10-13T21:10:20.000Z\",\"type\":\"turn_started\",\"model_id\":\"grok-4.6\"}}\n{{\"ts\":\"2026-10-13T21:10:30.000Z\",\"type\":\"turn_ended\"}}\n"
        ),
    )
    .expect("switch");
    std::fs::write(
        found.path.join("updates.jsonl"),
        format!(
            "{updates}{{\"timestamp\":1791925830,\"params\":{{\"update\":{{\"sessionUpdate\":\"turn_completed\",\"prompt_id\":\"two\",\"usage\":{{\"inputTokens\":61440,\"outputTokens\":1,\"cachedReadTokens\":0,\"cacheCreationTokens\":0,\"modelCalls\":1}}}}}}}}\n"
        ),
    )
    .expect("usage");
    std::fs::write(found.path.join("summary.json"), r#"{"current_model_id":"grok-4.6","context_window":256000}"#)
        .expect("summary");
    let switched = session::follow(sleeper.pid(), Some(found)).expect("switched");
    assert_eq!(switched.context, Some(Context { model: "grok-4.6".into(), percent: Some(24) }));
    let again = session::follow(sleeper.pid(), Some(switched)).expect("again");
    assert_eq!(again.context.as_ref().and_then(|ctx| ctx.percent), Some(24));
}

#[test]
fn a_multi_call_turn_shows_the_signals_written_with_it() {
    let (home, cwd) = home_with(&[
        ("events.jsonl", fixture!("grok/1.0.52/events.jsonl")),
        ("updates.jsonl", fixture!("grok/1.0.52/multi-call-updates.jsonl")),
        ("summary.json", fixture!("grok/1.0.52/summary.json")),
        ("signals.json", fixture!("grok/1.0.52/multi-call-signals.json")),
    ]);
    let (sleeper, found) = seen(home.path(), &cwd);
    assert_eq!(found.context.as_ref().and_then(|ctx| ctx.percent), Some(50));
    age(&found.path.join("signals.json"), 1_791_925_000);
    let stale = session::look(sleeper.pid()).expect("stale");
    assert_eq!(stale.context.as_ref().and_then(|ctx| ctx.percent), None);
}

#[test]
fn thousands_of_prompts_stay_bounded_and_count_once() {
    let (home, cwd) = home_with(&[("updates.jsonl", "")]);
    let (sleeper, mut found) = seen(home.path(), &cwd);
    let mut ledger = Ledger::default();
    let mut total = 0_usize;
    for batch in 0..4u64 {
        let mut text = std::fs::read_to_string(found.path.join("updates.jsonl")).expect("updates");
        for n in 0..500u64 {
            let n = batch * 500 + n;
            let ms = 1_791_925_800_000 + n;
            writeln!(
                text,
                "{{\"timestamp\":1791925800,\"params\":{{\"_meta\":{{\"agentTimestampMs\":{ms}}},\"update\":{{\"sessionUpdate\":\"user_message_chunk\",\"content\":{{\"type\":\"text\",\"text\":\"p\"}},\"_meta\":{{\"promptIndex\":0}}}}}}}}"
            )
            .expect("line");
        }
        std::fs::write(found.path.join("updates.jsonl"), text).expect("batch");
        found = session::follow(sleeper.pid(), Some(found)).expect("batch");
        assert!(found.retained_prompts() <= 32);
        let records = std::mem::take(&mut found.counted);
        total += records.iter().filter(|record| record.usage == Usage::Prompt).count();
        ledger.apply(&found.id, UNIX_EPOCH, records);
    }
    assert_eq!(total, 2000);
    let path = found.path.join("updates.jsonl");
    let body = std::fs::read(&path).expect("body");
    std::fs::write(&path, &body[..body.len() / 2]).expect("shrink");
    std::fs::write(&path, body).expect("restore");
    let mut replayed = session::follow(sleeper.pid(), Some(found)).expect("replay");
    ledger.apply(&replayed.id, UNIX_EPOCH, std::mem::take(&mut replayed.counted));
    assert_eq!(sum(ledger.take()).0, 2000);
}

#[test]
fn an_unfinished_turn_past_the_backwards_scan_stays_working() {
    session::set_back_limit(4096);
    let (home, cwd) = home_with(&[("events.jsonl", fixture!("grok/1.0.52/working-events.jsonl"))]);
    let path = session_dir(home.path(), &cwd).join("events.jsonl");
    grow_past_a_megabyte(&path);
    let (sleeper, found) = seen(home.path(), &cwd);
    assert_eq!((found.activity, found.turn_open), (Activity::Working, true));
    std::fs::write(
        found.path.join("events.jsonl"),
        format!(
            "{}{{\"ts\":\"2026-10-13T21:20:00.000Z\",\"type\":\"turn_ended\",\"outcome\":\"completed\"}}\n",
            std::fs::read_to_string(found.path.join("events.jsonl")).expect("events")
        ),
    )
    .expect("end");
    let ended = session::follow(sleeper.pid(), Some(found)).expect("ended");
    session::set_back_limit(64 * 1024 * 1024);
    assert_eq!((ended.activity, ended.turn_open), (Activity::Idle, false));
}

fn age(path: &Path, secs: u64) {
    let file = std::fs::File::options().write(true).open(path).expect("open");
    file.set_modified(UNIX_EPOCH + Duration::from_secs(secs)).expect("mtime");
}

#[test]
fn the_trust_screen_matches_the_default_prompt() {
    let mut trust = TrustPrompt::default();
    let pattern = trust.regex(DEFAULT_TRUST_PROMPT).expect("pattern");
    assert!(asks_trust(pattern, fixture!("grok/1.0.52/trust.txt")));
    let row: serde_json::Value = serde_json::from_str(fixture!("grok/1.0.52/active_sessions.json")).expect("row");
    assert_eq!(row[0]["cwd"], "/home/user/project");
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
