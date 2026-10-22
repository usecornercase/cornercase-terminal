use std::io::Write;
use std::time::UNIX_EPOCH;

use rstest::rstest;

use super::*;
use crate::agents::gemini::fake::FakeGemini;
use crate::test_util::TempDir;

const INITIAL: &str = crate::test_util::fixture!("gemini/0.63.0/initial.jsonl");
const PROMPT: &str = crate::test_util::fixture!("gemini/0.63.0/prompt.jsonl");
const REPLY: &str = crate::test_util::fixture!("gemini/0.63.0/reply.jsonl");
const COMPACTED: &str = crate::test_util::fixture!("gemini/0.63.0/compacted.jsonl");

fn written(text: &str) -> (TempDir, Session) {
    let dir = TempDir::new();
    let path = dir.path().join("session.jsonl");
    std::fs::write(&path, format!("{INITIAL}{text}")).expect("write session");
    let mut session = Session::new(path, FakeGemini::ID.into());
    session.update(None).expect("read session");
    (dir, session)
}

fn append(session: &mut Session, text: &str) {
    std::fs::OpenOptions::new()
        .append(true)
        .open(&session.path)
        .expect("open session")
        .write_all(text.as_bytes())
        .expect("append records");
    session.update(None).expect("update session");
}

#[test]
fn the_initial_context_is_neither_a_prompt_nor_a_conversation_to_resume() {
    let (_dir, session) = written("");
    assert_eq!((session.prompted, session.id(), session.context.as_ref()), (None, None, None));
}

#[test]
fn real_prompts_and_replies_supply_times_model_usage_and_resume() {
    let (_dir, session) = written(&format!("{PROMPT}{REPLY}"));
    let prompt: Value = serde_json::from_str(PROMPT).expect("prompt");
    assert_eq!(session.prompted, prompt["timestamp"].as_str().and_then(Stamp::parse));
    assert_eq!(session.id(), Some(FakeGemini::ID));
    assert_eq!(session.context, Some(Context { model: "gemini-3.8-flash".into(), percent: Some(1) }));
    assert_eq!(last_message(&session.path).expect("message").map(|s| s.text), Some("OK".into()));
}

#[test]
fn the_latest_update_of_a_message_wins() {
    let (_dir, mut session) = written(REPLY);
    let mut changed: Value = serde_json::from_str(REPLY).expect("reply");
    changed["tokens"]["input"] = Value::from(524_288);
    changed["content"] = Value::from("Updated answer");
    append(&mut session, &format!("{changed}\n"));
    assert_eq!(session.replies.len(), 1);
    assert_eq!(session.context.as_ref().and_then(|c| c.percent), Some(50));
    assert_eq!(last_message(&session.path).expect("message").map(|s| s.text), Some("Updated answer".into()));
    changed["content"] = Value::from("");
    append(&mut session, &format!("{changed}\n"));
    assert_eq!(last_message(&session.path).expect("no old update"), None);
}

#[test]
fn compression_replaces_the_history_and_clears_old_usage_without_confirming_a_prompt() {
    let (_dir, mut session) = written(&format!("{PROMPT}{REPLY}"));
    let before = session.prompted;
    append(&mut session, COMPACTED);
    assert_eq!(session.prompted, before);
    assert_eq!(session.context, Some(Context { model: "gemini-3.8-flash".into(), percent: None }));
    assert_eq!(last_message(&session.path).expect("no hidden summary"), None);
    assert_eq!(session.id(), Some(FakeGemini::ID));
}

#[test]
fn a_compression_is_said_until_a_reply_with_usage_follows() {
    let (_dir, mut session) = written(&format!("{PROMPT}{REPLY}"));
    let at = Stamp::parse("2026-10-10T12:47:31.593Z");

    append(&mut session, COMPACTED);
    let marked = session.compaction;
    append(&mut session, REPLY);

    assert_eq!((marked, session.compaction), (Compaction { at, hides: true }, Compaction { at, hides: false }));
}

#[test]
fn a_compressed_history_is_resumable_when_discovered_without_its_original_prompts() {
    let (_dir, session) = written(COMPACTED);
    assert_eq!(session.id(), Some(FakeGemini::ID));
    assert_eq!((session.prompted, session.context.as_ref()), (None, None));
    assert_eq!(last_message(&session.path).expect("no hidden summary"), None);
}

#[test]
fn a_resumed_history_keeps_message_times_and_omits_thought_parts() {
    let (_dir, session) = written(crate::test_util::fixture!("gemini/0.63.0/resumed.jsonl"));
    let said = last_message(&session.path).expect("read resumed reply").expect("resumed reply");
    assert!(said.text.starts_with("If there were finitely many primes"));
    assert!(!said.text.contains("Formulating Prime Proof"));
    assert_eq!(said.written.as_deref(), Some("2026-10-10T13:10:47.566Z"));
    assert_eq!(session.prompted, Stamp::parse("2026-10-10T13:10:42.278Z"));
    assert_eq!(session.context, Some(Context { model: "gemini-3.8-flash".into(), percent: Some(1) }));
}

#[rstest]
#[case::tool_result(serde_json::json!([{"functionResponse":{"name":"shell","response":{"output":"OK"}}}]), false)]
#[case::setup(serde_json::json!([{"text":"<session_context>setup"}]), false)]
#[case::hook(serde_json::json!([{"text":" \n <hook_context>setup"}]), false)]
#[case::summary(serde_json::json!([{"text":"<state_snapshot>summary"}]), false)]
#[case::scratchpad(serde_json::json!([{"text":"<scratchpad>summary"}]), false)]
#[case::slash(serde_json::json!([{"text":" \n /clear"}]), false)]
#[case::help(serde_json::json!(" \n ?help"), false)]
#[case::empty(serde_json::json!(" \n \t "), false)]
#[case::real(serde_json::json!([{"text":"fix the hook / command"}]), true)]
fn user_content_controls_confirmation_and_resumability(#[case] content: Value, #[case] expected: bool) {
    let entry = serde_json::json!({
        "id": "candidate", "type": "user", "content": content, "timestamp": "2026-10-10T13:00:00.000Z"
    });
    let (_dir, session) = written(&format!("{entry}\n"));
    assert_eq!(session.prompted.is_some(), expected);
    assert_eq!(session.id().is_some(), expected);
}

#[test]
fn a_model_switch_shows_the_id_without_old_usage_until_it_replies() {
    let (_dir, mut session) = written(REPLY);
    append(
        &mut session,
        "{\"id\":\"switch\",\"type\":\"info\",\"timestamp\":\"2026-10-10T13:00:00.000Z\",\
             \"content\":\"Model set to gemma-4-31b-it\"}\n",
    );
    assert_eq!(session.context, Some(Context { model: "gemma-4-31b-it".into(), percent: None }));
    let mut reply: Value = serde_json::from_str(REPLY).expect("reply");
    reply["id"] = Value::from("new-reply");
    reply["model"] = Value::from("gemma-4-31b-it");
    reply["timestamp"] = Value::from("2026-10-10T13:01:00.000Z");
    reply["tokens"]["input"] = Value::from(128_000);
    append(&mut session, &format!("{reply}\n"));
    assert_eq!(session.context.as_ref().and_then(|c| c.percent), Some(50));
}

#[test]
fn an_unsaved_picker_selection_clears_usage_and_a_fresh_reply_restores_it() {
    let (_dir, mut session) = written(REPLY);
    session.update(Some("gemini-3.5-flash-lite".into())).expect("read the selected model");
    assert_eq!(session.context, Some(Context { model: "gemini-3.5-flash-lite".into(), percent: None }));
    session.update(Some("auto".into())).expect("return to auto");
    assert_eq!(session.context, Some(Context { model: "auto".into(), percent: None }));
    let mut reply: Value = serde_json::from_str(REPLY).expect("reply");
    reply["id"] = Value::from("fresh-reply");
    reply["timestamp"] = Value::from(Stamp(SystemTime::now() + std::time::Duration::from_secs(1)).to_string());
    append(&mut session, &format!("{reply}\n"));
    assert_eq!(session.context, Some(Context { model: "gemini-3.8-flash".into(), percent: Some(1) }));
    std::fs::write(&session.path, INITIAL).expect("clear the history");
    session.update(None).expect("read cleared history");
    assert_eq!(session.context, None);
}

#[test]
fn the_current_footer_takes_precedence_over_historical_model_settings() {
    let (_dir, session) = written(&format!(
        "{REPLY}{{\"id\":\"setting\",\"type\":\"info\",\"content\":\"Model set to auto\",\
             \"timestamp\":\"2026-10-10T13:00:00.000Z\"}}\n"
    ));
    let mut first = Session::new(session.path, FakeGemini::ID.into());
    first.update(Some("gemini-3.5-flash-lite".into())).expect("first discovery with the current footer");
    assert_eq!(first.context, Some(Context { model: "gemini-3.5-flash-lite".into(), percent: None }));
}

#[test]
fn only_complete_lines_count_and_a_truncation_starts_over() {
    let (_dir, mut session) = written(PROMPT);
    let partial = REPLY.trim_end();
    append(&mut session, partial);
    assert_eq!(session.context, None);
    append(&mut session, "\n");
    assert!(session.context.is_some());
    std::fs::write(&session.path, INITIAL).expect("truncate session");
    session.update(None).expect("read truncated file");
    assert_eq!((session.prompted, session.id(), session.context.as_ref()), (None, None, None));
}

#[test]
fn replacement_and_deletion_do_not_keep_old_context() {
    let (_dir, mut session) = written(REPLY);
    let next = session.path.with_extension("next");
    std::fs::write(&next, INITIAL).expect("new inode");
    std::fs::rename(next, &session.path).expect("replace session");
    session.update(None).expect("read replacement");
    assert_eq!(session.context, None);
    std::fs::remove_file(&session.path).expect("delete file");
    assert!(session.update(None).is_none());
}

#[test]
fn discovery_uses_one_home_and_folder_and_rejects_stale_or_subagent_sessions() {
    let project = TempDir::new();
    let fake = FakeGemini::new(project.path());
    let first = fake.start(project.path());
    fake.append(REPLY);
    assert!(look(first.pid(), UNIX_EPOCH, None, None).is_some());
    let second = fake.start(project.path());
    assert!(look(first.pid(), UNIX_EPOCH, None, None).is_none());
    drop(second);
    assert!(look(first.pid(), SystemTime::now(), None, None).is_none());
    let other_home = FakeGemini::new(project.path());
    let _other = other_home.start(project.path());
    assert!(look(first.pid(), UNIX_EPOCH, None, None).is_some());
    std::fs::write(&fake.session, INITIAL.replace("\"main\"", "\"subagent\"")).expect("subagent header");
    assert!(look(first.pid(), UNIX_EPOCH, None, None).is_none());
}

#[test]
fn a_process_the_agent_starts_during_the_scan_is_not_another_gemini() {
    let project = TempDir::new();
    let fake = FakeGemini::new(project.path());
    fake.append(REPLY);
    let go = project.path().join("go");
    let parent = project.path().join("gemini");
    crate::test_util::write_executable(
        &parent,
        "#!/bin/sh\nwhile [ ! -e \"$1\" ]; do sleep 0.02; done\n\"$2\" \"$3\" \"$4\"\n",
    );
    let mut forker = std::process::Command::new(&parent)
        .arg(&go)
        .arg(&fake.script)
        .arg(&fake.session)
        .arg(fake.dir.path())
        .current_dir(project.path())
        .env_clear()
        .env("GEMINI_CLI_HOME", fake.dir.path())
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("spawn the parent");
    let pid = i32::try_from(forker.id()).expect("pid fits");
    crate::test_util::wait_until("the parent runs", || process::args(pid).len() > 1);

    let found = look_among(pid, UNIX_EPOCH, None, None, || {
        std::fs::write(&go, "").expect("signal the parent");
        crate::test_util::wait_until("the parent starts a gemini", || {
            process::children(pid).into_iter().any(|child| named(&process::args(child), NAME))
        });
        process::all()
    });
    let _ = forker.kill();
    let _ = forker.wait();
    assert!(found.is_some());
}

#[rstest]
#[case::model(
    "workspace (/directory)          sandbox          /model\n\
         /tmp/project                 no sandbox    gemini-3.8-flash",
    Some("gemini-3.8-flash")
)]
#[case::automatic("workspace (/directory)  sandbox  /model\n/tmp/project      no sandbox      Auto", Some("auto"))]
#[case::context_only("/model  context\nAuto  1% used", Some("auto"))]
#[case::quota(
    "workspace (/directory)  sandbox  /model  context  quota\n\
         /tmp/project  no sandbox  gemini-3.8-flash  1% used  95% left",
    Some("gemini-3.8-flash")
)]
#[case::hidden_workspace(
    "sandbox  /model  context  quota\nno sandbox  gemma-4-31b-it  50% used  95% left",
    Some("gemma-4-31b-it")
)]
#[case::branch_with_custom_model(
    "workspace (/directory)  branch  /model\n/tmp/project  gemini-foo  custom-model",
    None
)]
#[case::mismatched_cells("sandbox  /model  context\nno sandbox  gemini-3.8-flash", None)]
#[case::truncated("workspace (/directory)  sandbox  /model\n/tmp/project  no sandbox  gemini-3…", None)]
#[case::dialog("Select Model\n● gemini-3.8-flash", None)]
#[case::outside_footer("/model  context\ngemini-3.8-flash  1% used\none\ntwo\nthree\nfour\nfive\nsix", None)]
fn the_labelled_footer_only_supplies_complete_model_ids(#[case] screen: &str, #[case] expected: Option<&str>) {
    assert_eq!(footer_model(screen).as_deref(), expected);
}

mod counting_records {
    use super::*;
    use crate::agents::context::counting::{
        Ledger,
        tests::{since, sum},
    };

    #[test]
    fn rewritten_replies_add_only_the_increase_and_restored_history_adds_nothing() {
        let mut session = Session::new(PathBuf::from("synthetic.jsonl"), "synthetic".into());
        for line in crate::test_util::fixture!("gemini/0.63.0/usage.jsonl").lines() {
            session.apply(&serde_json::from_str(line).expect("record"));
        }
        let mut ledger = Ledger::default();
        ledger.apply("a", since(), std::mem::take(&mut session.counted));
        assert_eq!(
            sum(ledger.take()),
            (1, crate::agents::stats::Tokens { input: 90, out: 28, cache_read: 30, cache_write: 0 })
        );
        for line in crate::test_util::fixture!("gemini/0.63.0/usage-restore.jsonl").lines() {
            session.apply(&serde_json::from_str(line).expect("record"));
        }
        ledger.apply("a", since(), std::mem::take(&mut session.counted));
        assert_eq!(sum(ledger.take()).1.out, 0);
    }
}

mod conversation {
    use super::*;

    fn heard(session: &Session) -> Vec<(Role, String)> {
        let mut heard = Vec::new();
        said_all(&session.path, &mut |spoken| {
            heard.push((spoken.role, spoken.said.text));
            true
        })
        .expect("read the session");
        heard
    }

    #[test]
    fn gives_the_prompt_and_its_reply_newest_first() {
        let (_dir, session) = written(&format!("{PROMPT}{REPLY}"));

        assert_eq!(heard(&session), [(Role::Reply, "OK".into()), (Role::Prompt, "Reply only OK.".into())]);
    }

    #[test]
    fn the_latest_update_of_a_message_wins_and_a_set_of_messages_ends_the_history() {
        let (_dir, mut session) = written(&format!("{PROMPT}{REPLY}"));
        let mut changed: Value = serde_json::from_str(REPLY).expect("reply");
        changed["content"] = Value::from("Updated answer");
        append(&mut session, &format!("{changed}\n"));
        assert_eq!(heard(&session)[0], (Role::Reply, "Updated answer".into()));
        assert_eq!(heard(&session).len(), 2);

        let keep = serde_json::json!({ "$set": { "messages": [changed] } });
        append(&mut session, &format!("{keep}\n"));
        assert_eq!(heard(&session), [(Role::Reply, "Updated answer".into())]);
    }

    #[test]
    fn local_input_and_setup_text_are_not_prompts() {
        let slash = serde_json::json!({ "id": "a", "type": "user", "content": [{ "text": "/clear" }] });
        let setup = serde_json::json!({ "id": "b", "type": "user", "content": [{ "text": "<session_context>x" }] });
        let (_dir, session) = written(&format!("{slash}\n{setup}\n"));

        assert_eq!(heard(&session), []);
    }
}
