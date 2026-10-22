use std::io::Write;

use rstest::rstest;

use super::*;
use crate::agents::Known;
use crate::agents::codex::fake::FakeCodex;
use crate::agents::context::Pane;
use crate::agents::context::tests::pane_look;
use crate::test_util::{Running, Sleeper, TempDir, wait_until, write_executable};

const ID: &str = "019a1234-5678-7000-8000-000000000001";
const CONTEXT: &str = crate::test_util::fixture!("codex/0.160.0/context.jsonl");
const MODEL_CHANGE: &str = crate::test_util::fixture!("codex/0.160.0/model-change.jsonl");
const COMPACTED: &str = crate::test_util::fixture!("codex/0.160.0/compacted.jsonl");
const UNAVAILABLE: &str = crate::test_util::fixture!("codex/0.160.0/unavailable.jsonl");
const MALFORMED: &str = crate::test_util::fixture!("codex/0.160.0/malformed.jsonl");
const VSCODE: &str = crate::test_util::fixture!("codex/0.160.0/vscode-context.jsonl");
const STARTED: &str = crate::test_util::fixture!("codex/0.160.0/turn-started.jsonl");
const COMPLETE: &str = crate::test_util::fixture!("codex/0.160.0/turn-complete.jsonl");
const ABORTED: &str = crate::test_util::fixture!("codex/0.160.0/turn-aborted.jsonl");
const USER_MESSAGE: &str = crate::test_util::fixture!("codex/0.160.0/user-message.jsonl");

struct Written {
    _dir: TempDir,
    rollout: Rollout,
}

impl Written {
    fn new(text: &str) -> Self {
        let dir = TempDir::new();
        let path = dir.path().join(format!("rollout-2026-10-05T12-00-00-{ID}.jsonl"));
        std::fs::write(&path, text).expect("write rollout");
        let mut rollout = Rollout::new(path);
        rollout.update();
        Self { _dir: dir, rollout }
    }

    fn append(&mut self, text: &str) -> Option<Context> {
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.rollout.path).expect("open rollout");
        file.write_all(text.as_bytes()).expect("append rollout");
        self.rollout.update();
        self.rollout.context()
    }
}

fn shown(model: &str, percent: Option<u16>) -> Context {
    Context { model: model.into(), percent }
}

#[test]
fn the_versioned_fixture_uses_current_context_instead_of_lifetime_or_cached_totals() {
    let file = Written::new(CONTEXT);

    assert_eq!(file.rollout.context(), Some(shown("gpt-5.4", Some(20))));
}

#[test]
fn the_observed_vscode_source_still_reports_its_model_and_current_usage() {
    let file = Written::new(&VSCODE.replace("01a10c4c-92a4-7f13-b88b-2799ce5d8fcf", ID));

    assert_eq!(file.rollout.context(), Some(shown("gpt-6.1-sol", Some(6))));
}

#[rstest]
#[case::model_change(MODEL_CHANGE, "gpt-5.4-mini")]
#[case::compaction(COMPACTED, "gpt-5.4")]
#[case::missing_window(UNAVAILABLE, "gpt-5.4")]
#[case::malformed(MALFORMED, "gpt-5.4")]
fn the_versioned_changes_discard_the_obsolete_percentage(#[case] text: &str, #[case] model: &str) {
    let mut file = Written::new(CONTEXT);

    assert_eq!(file.append(text), Some(shown(model, None)));
}

#[rstest]
#[case::zero_window("0", "50000", None)]
#[case::negative_window("-1", "50000", None)]
#[case::string_window("\"250000\"", "50000", None)]
#[case::negative_tokens("250000", "-1", None)]
#[case::missing_tokens("250000", "null", None)]
#[case::empty_context("250000", "0", Some(0))]
#[case::full_context("250000", "999999", Some(100))]
fn only_valid_reported_numbers_give_a_percentage(
    #[case] window: &str,
    #[case] tokens: &str,
    #[case] expected: Option<u16>,
) {
    let mut file = Written::new(CONTEXT);
    let event = format!(
        "{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"last_token_usage\":{{\"total_tokens\":{tokens}}},\"model_context_window\":{window}}}}}}}\n"
    );

    assert_eq!(file.append(&event), Some(shown("gpt-5.4", expected)));
}

#[test]
fn rate_limit_updates_do_not_change_context_usage() {
    let mut file = Written::new(CONTEXT);

    let found = file
        .append("{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":null,\"rate_limits\":{}}}\n");

    assert_eq!(found, Some(shown("gpt-5.4", Some(20))));
}

#[test]
fn a_copied_settings_snapshot_cannot_change_this_threads_model() {
    let mut file = Written::new(CONTEXT);

    let found = file.append(&MODEL_CHANGE.replace(ID, "019a1234-5678-7000-8000-000000000002"));

    assert_eq!(found, Some(shown("gpt-5.4", Some(20))));
}

#[test]
fn partial_lines_wait_until_the_write_is_complete() {
    let mut file = Written::new(CONTEXT);
    let (head, tail) = MODEL_CHANGE.split_at(120);

    assert_eq!(file.append(head), Some(shown("gpt-5.4", Some(20))));
    assert_eq!(file.append(tail), Some(shown("gpt-5.4-mini", None)));
}

#[test]
fn a_new_usage_event_restores_the_percentage_after_compaction() {
    let mut file = Written::new(CONTEXT);
    file.append(COMPACTED);

    assert_eq!(file.append(CONTEXT.lines().last().expect("token event")), Some(shown("gpt-5.4", None)));
    assert_eq!(file.append("\n"), Some(shown("gpt-5.4", Some(20))));
}

#[test]
fn a_compaction_marks_the_rollout_with_its_time_until_the_next_usage_event() {
    let mut file = Written::new(CONTEXT);
    let at = Stamp::parse("2026-10-05T12:02:02.000Z");

    file.append(COMPACTED);
    let marked = file.rollout.compaction();
    file.append(CONTEXT.lines().last().expect("token event"));
    file.append("\n");

    assert_eq!((marked, file.rollout.compaction()), (Compaction { at, hides: true }, Compaction { at, hides: false }));
}

#[test]
fn a_compaction_and_its_usage_event_in_one_read_keep_the_time() {
    let mut file = Written::new(CONTEXT);
    let usage = CONTEXT.lines().last().expect("token event");

    file.append(&format!("{COMPACTED}{usage}\n"));

    assert_eq!(file.rollout.context(), Some(shown("gpt-5.4", Some(20))));
    assert_eq!(file.rollout.compaction(), Compaction { at: Stamp::parse("2026-10-05T12:02:02.000Z"), hides: false });
}

#[rstest]
#[case::missing_header("")]
#[case::malformed_header("{broken}\n")]
#[case::missing_model(
    "{\"type\":\"session_meta\",\"payload\":{\"id\":\"019a1234-5678-7000-8000-000000000001\",\"source\":\"cli\"}}\n"
)]
fn unavailable_rollouts_show_nothing(#[case] text: &str) {
    assert_eq!(Written::new(text).rollout.context(), None);
}

#[test]
fn subagents_and_mismatched_session_ids_are_rejected() {
    for text in [CONTEXT.replace("\"cli\"", "{\"subagent\":{}}"), CONTEXT.replace(ID, "another-thread")] {
        assert_eq!(Written::new(&text).rollout.context(), None);
    }
}

#[test]
fn truncation_and_replacement_drop_the_previous_context() {
    let mut file = Written::new(CONTEXT);
    let header = format!("{}\n", CONTEXT.lines().next().expect("header"));
    std::fs::write(&file.rollout.path, &header).expect("truncate");
    file.rollout.update();
    assert_eq!(file.rollout.context(), None);
    file.append(&CONTEXT[header.len()..]);
    let replacement = file.rollout.path.with_extension("next");
    std::fs::write(&replacement, CONTEXT.replace("gpt-5.4", "gpt-6.0")).expect("replacement");
    std::fs::rename(replacement, &file.rollout.path).expect("replace rollout");
    file.rollout.update();
    assert_eq!(file.rollout.context(), Some(shown("gpt-6.0", Some(20))));
}

#[test]
fn a_deleted_rollout_clears_the_line() {
    let mut file = Written::new(CONTEXT);
    std::fs::remove_file(&file.rollout.path).expect("remove rollout");
    file.rollout.update();

    assert_eq!(file.rollout.context(), None);
}

#[rstest]
#[case::no_turn_yet(&[], false)]
#[case::started(&[STARTED], true)]
#[case::completed(&[STARTED, COMPLETE], false)]
#[case::interrupted(&[STARTED, ABORTED], false)]
#[case::started_again(&[STARTED, COMPLETE, STARTED], true)]
fn a_turn_lasts_from_its_start_until_it_completes_or_is_aborted(#[case] events: &[&str], #[case] turn: bool) {
    let mut file = Written::new(CONTEXT);
    for event in events {
        file.append(event);
    }

    assert_eq!(file.rollout.turn(), turn);
}

#[test]
fn a_prompt_is_recorded_when_codex_takes_it() {
    let mut file = Written::new(CONTEXT);
    let before = file.rollout.prompted();

    file.append(USER_MESSAGE);

    let sent = SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(1_791_201_606_291);
    assert_eq!((before, file.rollout.prompted()), (None, Some(sent)));
}

#[test]
fn a_prompt_without_a_time_counts_from_when_it_was_read() {
    let mut file = Written::new(CONTEXT);
    let untimed = USER_MESSAGE.lines().last().expect("the user message").replace("\"timestamp\"", "\"at\"");
    let read = SystemTime::now();

    file.append(&format!("{untimed}\n"));

    assert!(file.rollout.prompted().is_some_and(|at| at >= read), "{:?}", file.rollout.prompted());
}

#[test]
fn a_turn_already_running_is_found_in_the_initial_tail() {
    let file = Written::new(&format!("{CONTEXT}{STARTED}"));

    assert!(file.rollout.turn());
}

#[test]
fn a_large_append_does_not_skip_a_compaction() {
    let mut file = Written::new(CONTEXT);
    let filler = format!("{{\"type\":\"response_item\",\"payload\":\"{}\"}}\n", "x".repeat(1000));

    assert_eq!(file.append(&format!("{COMPACTED}{}", filler.repeat(1100))), Some(shown("gpt-5.4", None)));
    file.rollout.update();
    assert_eq!(file.rollout.context(), Some(shown("gpt-5.4", None)));
}

#[test]
fn an_initial_tail_without_a_model_does_not_reuse_a_percentage() {
    let filler = format!("{{\"type\":\"response_item\",\"payload\":\"{}\"}}\n", "x".repeat(1000));
    let file = Written::new(&format!("{CONTEXT}{}\n{}\n", filler.repeat(1100), CONTEXT.lines().last().expect("usage")));

    assert_eq!(file.rollout.context(), None);
}

#[test]
fn the_home_is_read_from_the_agent_process() {
    let dir = TempDir::new();
    let configured = Sleeper::with_env(&[("CODEX_HOME", dir.path().to_str().expect("path")), ("HOME", "/missing")]);
    let default = Sleeper::with_env(&[("HOME", dir.path().to_str().expect("path"))]);
    std::fs::create_dir(dir.path().join(".codex")).expect("default home");

    assert_eq!(home(configured.pid()).as_deref(), Some(dir.path()));
    assert_eq!(home(default.pid()), Some(dir.path().join(".codex")));
}

#[test]
fn oversized_records_cannot_leave_an_obsolete_percentage_forever() {
    let mut file = Written::new(CONTEXT);
    let compacted = format!("{{\"type\":\"compacted\",\"payload\":{{\"message\":\"{}\"}}}}\n", "x".repeat(2_100_000));

    assert_eq!(file.append(&compacted), None);
    file.rollout.update();
    file.rollout.update();
    assert_eq!(file.rollout.context(), None);
    assert_eq!(file.append(CONTEXT), Some(shown("gpt-5.4", Some(20))));
}

#[rstest]
#[case::native(false, "cli")]
#[case::wrapper(true, "cli")]
#[case::native_vscode_source(false, "vscode")]
#[case::wrapper_vscode_source(true, "vscode")]
fn a_fake_agent_is_associated_by_its_open_rollout_and_cleared_on_exit(#[case] wrapper: bool, #[case] source: &str) {
    let fake = FakeCodex::new(ID, "gpt-5.4", wrapper);
    std::fs::write(&fake.rollout, CONTEXT.replace("\"source\":\"cli\"", &format!("\"source\":\"{source}\"")))
        .expect("write the session source");
    let child =
        Running::spawn(std::process::Command::new(&fake.script).args(fake.args()).env("CODEX_HOME", fake.dir.path()));
    let pid = child.pid();
    let mut pane = Pane::default();
    wait_until("the fake agent opens its rollout", || {
        pane.update(Some(Known::Codex), &pane_look(pid));
        pane.context().is_some()
    });
    assert_eq!(pane.context().cloned(), Some(shown("gpt-5.4", Some(20))));
    fake.signal("quit", "");
    drop(child);
    wait_until("the line clears", || {
        pane.update(Some(Known::Codex), &pane_look(pid));
        pane.context().is_none()
    });
}

#[test]
fn ambiguous_open_rollouts_are_hidden_and_subagent_rollouts_are_excluded() {
    let fake = FakeCodex::new(ID, "gpt-5.4", false);
    let other = fake.rollout.with_file_name("rollout-2026-10-05T12-00-00-019a1234-5678-7000-8000-000000000002.jsonl");
    let content = CONTEXT.replace(ID, "019a1234-5678-7000-8000-000000000002");
    std::fs::write(&other, &content).expect("other rollout");
    write_executable(
        &fake.script,
        "#!/bin/sh\nexec 3>> \"$1\"\nexec 4>> \"$2\"\nwhile [ -d \"$3\" ] && [ ! -e \"$3/quit\" ]; do sleep 0.02; done\n",
    );
    let child = Running::spawn(
        std::process::Command::new(&fake.script)
            .args([&fake.rollout, &other, &fake.home])
            .env("CODEX_HOME", &fake.home),
    );
    let pid = child.pid();
    wait_until("both rollouts are open", || process::open_files(pid).contains(&other));
    assert_eq!(rollout_path(pid, SystemTime::UNIX_EPOCH), None);

    std::fs::write(&other, content.replace("\"cli\"", "{\"subagent\":{}}")).expect("subagent metadata");
    assert_eq!(rollout_path(pid, SystemTime::UNIX_EPOCH), Some(fake.rollout.clone()));
}

#[test]
fn a_codex_child_whose_home_cannot_be_read_does_not_hide_the_session() {
    let fake = FakeCodex::new(ID, "gpt-5.4", false);
    let child = fake.dir.path().join("child/codex");
    std::fs::create_dir_all(child.parent().expect("child folder")).expect("create child folder");
    write_executable(&child, "#!/bin/sh\nwhile [ -d \"$1\" ] && [ ! -e \"$1/quit\" ]; do /bin/sleep 0.02; done\n");
    write_executable(
        &fake.script,
        "#!/bin/sh\nexec 3>> \"$1\"\n/usr/bin/env -i \"$3\" \"$2\" &\nwhile [ -d \"$2\" ] && [ ! -e \"$2/quit\" ]; do sleep 0.02; done\n",
    );
    let agent = Running::spawn(
        std::process::Command::new(&fake.script)
            .args([&fake.rollout, &fake.home, &child])
            .env("CODEX_HOME", &fake.home),
    );
    let pid = agent.pid();
    wait_until("the Codex child runs without an environment", || {
        process::children(pid)
            .into_iter()
            .any(|c| process::args(c).get(1).is_some_and(|a| Path::new(a) == child) && home(c).is_none())
    });

    assert_eq!(rollout_path(pid, SystemTime::UNIX_EPOCH), Some(fake.rollout.clone()));
}

#[test]
fn a_session_served_by_the_daemon_is_found_by_folder_once_written_after_the_agent_started() {
    let fake = FakeCodex::new(ID, "gpt-5.4", false);
    let project = TempDir::new();
    let daemon = fake.serve(project.path());
    let agent = fake.agent(project.path());
    let since = SystemTime::now();
    assert_eq!(rollout_path(agent.pid(), since), None);

    let usage = format!("{}\n", CONTEXT.lines().last().expect("usage"));
    let mut pane = Pane::default();
    wait_until("the session is written after the agent started", || {
        fake.append(&usage);
        pane.update(Some(Known::Codex), &pane_look(agent.pid()));
        pane.context().is_some()
    });
    assert_eq!(pane.context().cloned(), Some(shown("gpt-5.4", Some(20))));
    drop(daemon);
    wait_until("the line clears", || {
        pane.update(Some(Known::Codex), &pane_look(agent.pid()));
        pane.context().is_none()
    });
}

#[test]
fn a_served_session_is_hidden_from_other_folders_and_while_another_agent_shares_its_folder() {
    let fake = FakeCodex::new(ID, "gpt-5.4", false);
    let project = TempDir::new();
    let _daemon = fake.serve(project.path());
    let elsewhere = fake.agent(fake.dir.path());
    let agent = fake.agent(project.path());
    let since = SystemTime::now();
    let usage = format!("{}\n", CONTEXT.lines().last().expect("usage"));
    wait_until("the session is written after the agent started", || {
        fake.append(&usage);
        rollout_path(agent.pid(), since).is_some()
    });

    assert_eq!(rollout_path(elsewhere.pid(), since), None);
    let second = fake.agent(project.path());
    assert_eq!(rollout_path(agent.pid(), since), None);
    drop(second);
    assert_eq!(rollout_path(agent.pid(), since), Some(fake.rollout.clone()));
}

mod counting_records {
    use super::*;
    use crate::agents::context::counting::{
        Ledger,
        tests::{since, sum},
    };

    #[test]
    fn cumulative_usage_adds_deltas_and_smaller_totals_start_a_new_baseline() {
        let records: Vec<_> = crate::test_util::fixture!("codex/0.160.0/usage.jsonl")
            .lines()
            .filter_map(|s| counted(&serde_json::from_str(s).expect("record")))
            .collect();
        let mut ledger = Ledger::default();
        ledger.apply("thread", since(), records.clone());
        assert_eq!(
            sum(ledger.take()).1,
            crate::agents::stats::Tokens { input: 180, out: 80, cache_read: 120, cache_write: 0 }
        );
        ledger.apply("thread", since(), records.clone());
        assert_eq!(sum(ledger.take()).1.out, 0);
        let mut first = Ledger::default();
        first.apply("thread", since(), records.into_iter().skip(1).take(1));
        assert_eq!(sum(first.take()).1.out, 50);
    }
}

#[test]
fn the_conversation_is_the_open_rollout_at_its_path_under_sessions() {
    let fake = FakeCodex::new(ID, "gpt-5.4", false);
    let child =
        Running::spawn(std::process::Command::new(&fake.script).args(fake.args()).env("CODEX_HOME", fake.dir.path()));
    let mut pane = Pane::default();
    wait_until("the fake agent opens its rollout", || {
        pane.update(Some(Known::Codex), &pane_look(child.pid()));
        pane.conversation_files().is_some()
    });

    let files = pane.conversation_files().expect("files");

    let relative = PathBuf::from(format!("sessions/2026/10/05/rollout-2026-10-05T12-00-00-{ID}.jsonl"));
    assert_eq!(files, vec![(fake.rollout.clone(), relative)]);
    fake.signal("quit", "");
}

#[test]
fn a_rollout_outside_a_sessions_folder_has_no_conversation_to_move() {
    let record = crate::agents::context::Record::Codex(PathBuf::from("/tmp/rollout.jsonl"));

    assert_eq!(record.conversation_files(), None);
}
