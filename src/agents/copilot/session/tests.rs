use std::process::{Command, Stdio};
use std::time::UNIX_EPOCH;

use super::*;
use crate::agents::context::counting::{Ledger, tests::sum};
use crate::agents::copilot::fake::{FakeCopilot, ID};
use crate::test_util::{Running, TempDir, exited_pid, fixture, wait_until};

const TURN: &str = fixture!("copilot/1.0.98/turn.jsonl");
const PERMISSION: &str = fixture!("copilot/1.0.98/permission.jsonl");

const OTHER: &str = "55555555-5555-4555-8555-555555555555";

fn cwd() -> TempDir {
    TempDir::new()
}

fn running(fake: &FakeCopilot, args: &[&str]) -> Running {
    let process = fake.spawn(&fake.script, args);
    wait_until("copilot holds its session", || fake.hold(ID, process.pid()).exists());
    process
}

fn seen(fake: &FakeCopilot, process: &Running) -> Session {
    look(process.pid(), &[]).unwrap_or_else(|| panic!("a session for {}", fake.script.display()))
}

#[test]
fn a_held_session_is_the_panes() {
    let dir = cwd();
    let fake = FakeCopilot::new(dir.path());
    let process = running(&fake, &[]);

    let session = seen(&fake, &process);

    assert_eq!(
        (session.id.as_str(), session.path, session.activity, session.context),
        (ID, fake.session(ID).canonicalize().expect("session folder"), Some(Activity::Idle), None)
    );
}

#[test]
fn the_presence_file_says_whether_it_works() {
    let dir = cwd();
    let fake = FakeCopilot::new(dir.path());
    let process = running(&fake, &[]);

    let idle = seen(&fake, &process).activity;
    fake.working(true);
    let working = seen(&fake, &process).activity;
    std::fs::remove_file(fake.home.join("open-sessions-state.json")).expect("remove the presence file");
    let unknown = seen(&fake, &process).activity;

    assert_eq!((idle, working, unknown), (Some(Activity::Idle), Some(Activity::Working), None));
}

#[test]
fn a_write_in_progress_keeps_the_last_known_state() {
    let dir = cwd();
    let fake = FakeCopilot::new(dir.path());
    let process = running(&fake, &[]);
    fake.working(true);
    let first = seen(&fake, &process);
    std::fs::write(fake.home.join("open-sessions-state.json"), "{\"a\":").expect("half a file");

    let again = follow(process.pid(), &[], Some(first)).expect("a session");

    assert_eq!(again.activity, Some(Activity::Working));
}

#[test]
fn a_permission_or_a_background_shell_changes_what_working_means() {
    let dir = cwd();
    let fake = FakeCopilot::new(dir.path());
    let process = running(&fake, &[]);
    fake.started("gpt-4.1");
    fake.working(true);
    let mut session = seen(&fake, &process);
    fake.permission(true);
    session = follow(process.pid(), &[], Some(session)).expect("a session");
    let waiting = session.activity;
    fake.permission(false);
    fake.turn(false);
    fake.shell_started("0");
    session = follow(process.pid(), &[], Some(session)).expect("a session");
    let shell = session.activity;
    fake.shell_done("0");
    session = follow(process.pid(), &[], Some(session)).expect("a session");

    assert_eq!(
        (waiting, shell, session.activity),
        (Some(Activity::Waiting), Some(Activity::Shell), Some(Activity::Working))
    );
}

#[test]
fn a_lock_left_by_a_dead_process_is_ignored() {
    let dir = cwd();
    let fake = FakeCopilot::new(dir.path());
    let dead = exited_pid();
    std::fs::write(fake.session(ID).join(format!("inuse.{dead}.lock")), dead.to_string()).expect("stale lock");
    std::fs::write(fake.hold(ID, dead), "").expect("stale hold");
    let other = fake.spawn(Path::new("/bin/sleep"), &["30"]);

    assert!(look(other.pid(), &[]).is_none());
}

#[test]
fn a_live_process_that_does_not_hold_the_file_is_not_its_owner() {
    let dir = cwd();
    let fake = FakeCopilot::new(dir.path());
    let other = fake.spawn(Path::new("/bin/sleep"), &["30"]);
    std::fs::write(fake.hold(ID, other.pid()), "").expect("a file nobody holds");

    assert!(look(other.pid(), &[]).is_none());
}

#[test]
fn two_copilot_in_one_home_each_follow_their_own_session() {
    let dir = cwd();
    let fake = FakeCopilot::new(dir.path());
    let first = running(&fake, &[]);
    std::fs::create_dir_all(fake.session(OTHER)).expect("second session");
    let argument = format!("--resume={OTHER}");
    let second = fake.spawn(&fake.script, &[&argument]);
    fake.prompted();
    wait_until("the second holds its session", || fake.hold(OTHER, second.pid()).exists());

    let ids = (seen(&fake, &first).id, look(second.pid(), &[]).map(|session| session.id));

    assert_eq!(ids, (ID.to_string(), Some(OTHER.to_string())));
}

#[test]
fn clearing_moves_to_the_new_session_and_forgets_the_old_events() {
    let dir = cwd();
    let fake = FakeCopilot::new(dir.path());
    let process = running(&fake, &[]);
    fake.started("gpt-4.1");
    fake.prompt("before");
    let before = seen(&fake, &process);
    fake.clear(OTHER);
    wait_until("the new session is held", || fake.hold(OTHER, process.pid()).exists());

    let after = follow(process.pid(), &[], Some(before)).expect("a session");

    assert_eq!((after.id.as_str(), after.resumable, after.context), (OTHER, false, None));
}

#[test]
fn a_process_holding_two_sessions_shows_none() {
    let dir = cwd();
    let fake = FakeCopilot::new(dir.path());
    std::fs::create_dir_all(fake.session(OTHER)).expect("second session");
    let script = format!(
        "exec 3>\"$1/session-state/{ID}/inuse.$$.hold\"; exec 4>\"$1/session-state/{OTHER}/inuse.$$.hold\"; sleep 30; :"
    );
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", &script, "sh"])
        .arg(&fake.home)
        .env("COPILOT_HOME", &fake.home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let process = Running::spawn(&mut command);
    wait_until("both files are held", || {
        std::fs::read_dir(fake.session(OTHER)).is_ok_and(|mut entries| {
            entries.any(|e| e.is_ok_and(|e| e.file_name().to_string_lossy().starts_with("inuse.")))
        })
    });

    assert!(look(process.pid(), &[]).is_none());
}

#[test]
fn a_resumed_process_is_followed_from_its_arguments_until_it_takes_the_lock() {
    let dir = cwd();
    let fake = FakeCopilot::new(dir.path());
    let argument = format!("--resume={ID}");
    let process = fake.spawn(&fake.script, &[&argument]);
    wait_until("the fake runs with its home", || {
        process::env(process.pid()).iter().any(|(key, _)| key == "COPILOT_HOME")
    });
    let args = vec!["copilot".to_string(), argument];

    let before = look(process.pid(), &args).expect("a session from the arguments");
    fake.prompted();
    wait_until("the lock is taken", || fake.hold(ID, process.pid()).exists());
    let after = look(process.pid(), &args).expect("a held session");

    assert_eq!((before.id.as_str(), before.activity, after.id.as_str()), (ID, Some(Activity::Idle), ID));
}

#[test]
fn a_session_another_live_process_holds_is_not_taken_from_the_arguments() {
    let dir = cwd();
    let fake = FakeCopilot::new(dir.path());
    let holder = fake.spawn(Path::new("/bin/sleep"), &["30"]);
    std::fs::write(fake.session(ID).join(format!("inuse.{}.lock", holder.pid())), "").expect("a live lock");
    let args = vec!["copilot".to_string(), format!("--resume={ID}")];
    let process = fake.spawn(Path::new("/bin/sleep"), &["30"]);

    assert!(look(process.pid(), &args).is_none());
}

#[test]
fn an_argument_that_names_no_session_folder_finds_nothing() {
    let dir = cwd();
    let fake = FakeCopilot::new(dir.path());
    let process = fake.spawn(Path::new("/bin/sleep"), &["30"]);
    let args = vec!["copilot".to_string(), "--resume=nothing-here".to_string()];

    assert!(look(process.pid(), &args).is_none());
}

#[test]
fn the_session_is_found_through_the_npm_loader() {
    let dir = cwd();
    let fake = FakeCopilot::new(dir.path());
    let loader = fake.loader();
    let process = fake.spawn(&loader, &[]);
    wait_until("the child holds the session", || {
        std::fs::read_dir(fake.session(ID))
            .is_ok_and(|entries| entries.flatten().any(|e| e.file_name().to_string_lossy().ends_with(".hold")))
    });
    let args = vec!["sh".to_string(), loader.to_string_lossy().into_owned()];

    let session = look(process.pid(), &args).expect("the child's session");

    assert_eq!(session.id, ID);
}

#[test]
fn a_missing_events_file_shows_nothing_and_a_new_one_is_read() {
    let dir = cwd();
    let fake = FakeCopilot::new(dir.path());
    let process = running(&fake, &[]);
    let empty = seen(&fake, &process);
    fake.started("gpt-4.1");
    fake.prompt("hello");
    fake.usage(32000, 64000, "gpt-4.1");

    let read = follow(process.pid(), &[], Some(empty)).expect("a session");

    assert_eq!(
        (read.context, read.resumable, read.prompted.is_some()),
        (Some(Context { model: "gpt-4.1".into(), percent: Some(50) }), true, true)
    );
}

#[test]
fn a_replaced_file_starts_over() {
    let dir = cwd();
    let fake = FakeCopilot::new(dir.path());
    let process = running(&fake, &[]);
    fake.started("gpt-4.1");
    let first = seen(&fake, &process);
    std::fs::remove_file(fake.events(ID)).expect("remove the events");
    fake.started("gpt-5");

    let again = follow(process.pid(), &[], Some(first)).expect("a session");

    assert_eq!(again.context.map(|context| context.model), Some("gpt-5".to_string()));
}

#[test]
fn a_long_file_is_seeded_from_behind_its_tail() {
    let dir = cwd();
    let fake = FakeCopilot::new(dir.path());
    let process = running(&fake, &[]);
    let mut text = String::from(TURN);
    let filler = "{\"type\":\"tool.execution_start\",\"data\":{\"toolCallId\":\"x\",\"toolName\":\"view\"},\"timestamp\":\"2026-10-17T17:24:00.000Z\"}\n";
    text.push_str(&filler.repeat(14_000));
    std::fs::write(fake.events(ID), text).expect("a long file");

    let session = seen(&fake, &process);

    assert_eq!(
        (session.context, session.resumable),
        (Some(Context { model: "gpt-4.1".into(), percent: Some(19) }), true)
    );
}

#[test]
fn the_seed_stops_at_the_back_limit() {
    let dir = cwd();
    let fake = FakeCopilot::new(dir.path());
    let process = running(&fake, &[]);
    let filler = "{\"type\":\"tool.execution_start\",\"data\":{\"toolCallId\":\"x\",\"toolName\":\"view\"},\"timestamp\":\"2026-10-17T17:24:00.000Z\"}\n";
    std::fs::write(fake.events(ID), format!("{TURN}{}", filler.repeat(14_000))).expect("a long file");
    limit_back(Some(4096));

    let session = seen(&fake, &process);
    limit_back(None);

    assert_eq!(session.context, None);
}

#[test]
fn usage_counts_once_across_polls_and_the_prompt_too() {
    let dir = cwd();
    let fake = FakeCopilot::new(dir.path());
    let process = running(&fake, &[]);
    let at = UNIX_EPOCH;
    fake.started("gpt-4.1");
    fake.prompt("hello");
    fake.usage(12345, 64000, "gpt-4.1");
    let mut ledger = Ledger::default();
    let mut session = seen(&fake, &process);
    ledger.apply(&session.id, at, std::mem::take(&mut session.counted));
    session = follow(process.pid(), &[], Some(session)).expect("a session");
    ledger.apply(&session.id, at, std::mem::take(&mut session.counted));

    let (prompts, tokens) = sum(ledger.take());

    assert_eq!((prompts, tokens.out), (1, 10));
}

#[test]
fn the_pending_permission_of_a_long_session_is_read_from_its_tail() {
    let dir = cwd();
    let fake = FakeCopilot::new(dir.path());
    let process = running(&fake, &[]);
    fake.working(true);
    std::fs::write(fake.events(ID), PERMISSION).expect("events");

    assert_eq!(seen(&fake, &process).activity, Some(Activity::Waiting));
}

#[test]
fn no_process_of_its_own_means_no_session() {
    let process = Running::spawn(Command::new("/bin/sleep").arg("30"));

    assert!(look(process.pid(), &[]).is_none());
}

#[test]
fn a_question_a_plan_review_and_a_permission_each_wait_and_ctrl_c_clears_them() {
    let dir = cwd();
    let fake = FakeCopilot::new(dir.path());
    let process = running(&fake, &[]);
    fake.working(true);
    let mut session = seen(&fake, &process);
    let mut states = Vec::new();
    let steps: [&dyn Fn(); 7] = [
        &|| fake.ask(true),
        &|| fake.ask(false),
        &|| fake.plan_review(),
        &|| fake.abort(),
        &|| fake.permission(true),
        &|| fake.abort(),
        &|| fake.turn(true),
    ];
    for step in steps {
        step();
        session = follow(process.pid(), &[], Some(session)).expect("a session");
        states.push(session.activity);
    }

    let (waiting, working) = (Some(Activity::Waiting), Some(Activity::Working));
    assert_eq!(states, [waiting, working, waiting, working, waiting, working, working]);
}

#[test]
fn a_sub_agent_adds_tokens_and_a_model_change_hides_the_percentage() {
    let dir = cwd();
    let fake = FakeCopilot::new(dir.path());
    let process = running(&fake, &[]);
    fake.started("gpt-4.1");
    fake.prompt("hello");
    fake.usage(32000, 64000, "gpt-4.1");
    fake.subagent_usage(50000, 64000);
    let mut session = seen(&fake, &process);
    let shown = session.context.clone();
    let tokens = session.counted.len();
    fake.model_change("claude-sonnet-4.5");
    session = follow(process.pid(), &[], Some(session)).expect("a session");

    assert_eq!(
        (shown, tokens, session.context),
        (
            Some(Context { model: "gpt-4.1".into(), percent: Some(50) }),
            3,
            Some(Context { model: "claude-sonnet-4.5".into(), percent: None })
        )
    );
}

#[test]
fn a_sub_agents_newer_record_does_not_hide_the_main_percentage_after_a_seed() {
    let dir = cwd();
    let fake = FakeCopilot::new(dir.path());
    let process = running(&fake, &[]);
    let filler = "{\"type\":\"tool.execution_start\",\"data\":{\"toolCallId\":\"x\",\"toolName\":\"view\"},\"timestamp\":\"2026-10-17T17:24:00.000Z\"}\n";
    let sub = "{\"type\":\"session.usage_record\",\"data\":{\"usage\":{\"model\":\"gpt-4.1\",\"inputTokens\":60000,\"maxPromptTokens\":64000,\"initiator\":\"sub-agent\"}},\"id\":\"s\",\"timestamp\":\"2026-10-17T17:23:30.000Z\"}\n";
    std::fs::write(fake.events(ID), format!("{TURN}{sub}{}", filler.repeat(14_000))).expect("a long file");

    let session = seen(&fake, &process);

    assert_eq!(session.context, Some(Context { model: "gpt-4.1".into(), percent: Some(19) }));
}
