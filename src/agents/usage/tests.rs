use rstest::rstest;

use super::*;
use crate::accounts::Account;
use crate::agents::claude::usage as claude;
use crate::agents::claude::usage::tests as claude_tests;
use crate::agents::codex::usage::tests as codex_tests;
use crate::test_util::{TempDir, write_executable};

#[rstest]
#[case::minutes(13 * 60, "resets in 13m")]
#[case::rounds_up(30, "resets in 1m")]
#[case::hours(2 * 3_600 + 13 * 60, "resets in 2h 13m")]
#[case::days(3 * 86_400 + 4 * 3_600, "resets in 3d 4h")]
#[case::past(-5, "resets in 0m")]
fn reset_times_are_relative(#[case] secs: i64, #[case] expected: &str) {
    assert_eq!(until(secs), expected);
}

fn fake(name: &str, script: &str) -> (TempDir, String) {
    let dir = TempDir::new();
    let path = dir.path().join(name);
    write_executable(&path, &format!("#!/bin/sh\n{script}\n"));
    let command = path.display().to_string();
    (dir, command)
}

#[rstest]
#[case::claude(Known::Claude, 2, claude_tests::RECORDED, claude_tests::report)]
#[case::codex(Known::Codex, 3, codex_tests::RECORDED, codex_tests::report)]
fn the_probe_answers_after_reading_the_requests(
    #[case] agent: Known,
    #[case] requests: usize,
    #[case] recorded: &str,
    #[case] report: fn(&str) -> Report,
) {
    let reads = "read -r line\n".repeat(requests);
    let script = format!("{reads}echo '{{\"type\":\"system\"}}'\necho '{recorded}'\nexec sleep 30");
    let (_dir, command) = fake(agent.name(), &script);
    assert_eq!(probe(agent, &command, None, TIMEOUT).expect("a report"), report(recorded));
}

#[test]
fn the_codex_probe_runs_the_app_server() {
    let (_dir, command) = fake(
        "codex",
        "read -r a\nread -r b\nread -r c\necho \"{\\\"id\\\":2,\\\"error\\\":{\\\"message\\\":\\\"$1\\\"}}\"",
    );
    let error = probe(Known::Codex, &command, None, TIMEOUT).expect_err("the arguments as an error");
    assert_eq!(error.to_string(), "app-server");
}

#[test]
fn a_codex_that_takes_no_params_reads_its_limits() {
    let script = format!(
        "read -r a\nread -r b\nread -r c\ncase \"$c\" in *'\"params\":{{\"'*) echo '{}' ;; *) echo '{}' ;; esac\nexec sleep 30",
        codex_tests::PARAMS_REFUSED,
        codex_tests::RECORDED,
    );
    let (_dir, command) = fake(Known::Codex.name(), &script);
    let report = probe(Known::Codex, &command, None, TIMEOUT).expect("a report");
    assert_eq!(report, codex_tests::report(codex_tests::RECORDED));
}

#[rstest]
#[case::hangs(Known::Claude, "exec sleep 30", Duration::from_millis(300), "claude did not answer in time")]
#[case::exits(Known::Claude, "exit 1", TIMEOUT, "claude exited without answering")]
#[case::codex_hangs(Known::Codex, "exec sleep 30", Duration::from_millis(300), "codex did not answer in time")]
fn a_probe_that_gets_no_answer_fails(
    #[case] agent: Known,
    #[case] script: &str,
    #[case] timeout: Duration,
    #[case] expected: &str,
) {
    let (_dir, command) = fake(agent.name(), script);
    let error = probe(agent, &command, None, timeout).expect_err("no report");
    assert_eq!(error.to_string(), expected);
}

#[test]
fn an_agent_gone_before_the_requests_fails_at_once() {
    let gone = Some(Err(std::io::ErrorKind::BrokenPipe.into()));
    let error = await_answer(gone, None::<&'static [u8]>, claude::answer, "claude", Duration::from_secs(2))
        .expect_err("no report");
    assert_eq!(error.to_string(), "claude exited without answering");
}

#[test]
fn a_missing_agent_fails() {
    let error = probe(Known::Codex, "/nonexistent/codex", None, TIMEOUT).expect_err("no report");
    assert!(error.to_string().starts_with("could not run /nonexistent/codex"), "{error}");
}

#[test]
fn each_answer_counts_once_and_a_running_probe_is_not_restarted() {
    let mut state = State::default();
    assert_eq!(state.start(vec![(Known::Claude, None)]), [(Known::Claude, None)]);
    assert_eq!(state.probe(Known::Claude).answers(), 0);
    assert_eq!(state.start(vec![(Known::Claude, None)]), []);
    state.answered((Known::Claude, None), Ok(Report::default()), Instant::now());
    assert_eq!(state.probe(Known::Claude).answers(), 1);
    assert_eq!(state.start(vec![(Known::Claude, None)]), [(Known::Claude, None)]);
    state.answered((Known::Claude, None), Err(Error::Usage("no".into())), Instant::now());
    assert_eq!(state.probe(Known::Claude).answers(), 2);
    assert_eq!(state.probe(Known::Codex).answers(), 0);
}

fn account(agent: Known, name: &str) -> Account {
    Account { kind: agent, name: name.into(), home: format!("/accounts/{name}").into() }
}

#[test]
fn each_account_has_a_probe_of_its_own() {
    let mut state = State::default();
    let work = account(Known::Claude, "work");
    let home = account(Known::Claude, "home");
    let all = vec![(Known::Claude, None), (Known::Claude, Some(work.clone())), (Known::Claude, Some(home.clone()))];
    assert_eq!(state.start(all.clone()), all);
    state.answered(key(Known::Claude, Some(&work)), Ok(Report { limited: true, ..Report::default() }), Instant::now());
    assert_eq!(state.probe_of(Known::Claude, Some("work")).answers(), 1);
    assert!(state.probe_of(Known::Claude, Some("home")).loading());
    assert!(state.probe(Known::Claude).loading());
    assert_eq!(state.start(all), [(Known::Claude, Some(work))]);
    let shown: Vec<_> = state.shown().map(|(agent, name, _)| (agent, name.map(str::to_string))).collect();
    assert_eq!(
        shown,
        [(Known::Claude, None), (Known::Claude, Some("work".into())), (Known::Claude, Some("home".into()))]
    );
    assert_eq!(state.probe_of(Known::Codex, Some("work")).answers(), 0);
}

#[test]
fn without_accounts_the_probes_are_the_agents_in_their_order() {
    let mut state = State::default();
    assert_eq!(
        state.start(vec![(Known::Claude, None), (Known::Codex, None)]),
        [(Known::Claude, None), (Known::Codex, None)]
    );
    let shown: Vec<_> = state.shown().map(|(agent, name, _)| (agent, name.is_some())).collect();
    assert_eq!(shown, [(Known::Claude, false), (Known::Codex, false)]);
}

#[rstest]
#[case::claude(Known::Claude, 2, "CLAUDE_CONFIG_DIR", claude_tests::RECORDED)]
#[case::codex(Known::Codex, 3, "CODEX_HOME", codex_tests::RECORDED)]
fn an_account_probe_runs_with_its_folder(
    #[case] agent: Known,
    #[case] requests: usize,
    #[case] variable: &str,
    #[case] recorded: &str,
) {
    let reads = "read -r line\n".repeat(requests);
    let script = format!("printf '%s' \"${variable}\" > \"$0.env\"\n{reads}echo '{recorded}'\nexec sleep 30");
    let (dir, command) = fake(agent.name(), &script);
    let folder = dir.path().join("work");
    probe(agent, &command, Some(&folder), TIMEOUT).expect("a report");
    let seen = std::fs::read_to_string(format!("{command}.env")).expect("the probe wrote its environment");
    assert_eq!(seen, folder.display().to_string());
}

#[test]
fn a_claude_account_probe_drops_the_secure_storage_folder() {
    let spec = Known::Claude.usage().expect("claude has a probe");
    let built = command(Known::Claude, &spec, "claude", Some(Path::new("/accounts/work")));
    let envs: Vec<_> = built.get_envs().collect();
    assert!(envs.contains(&("CLAUDE_SECURESTORAGE_CONFIG_DIR".as_ref(), None)), "{envs:?}");
    assert!(envs.contains(&("CLAUDE_CONFIG_DIR".as_ref(), Some("/accounts/work".as_ref()))), "{envs:?}");
    let system = command(Known::Claude, &spec, "claude", None);
    assert!(system.get_envs().all(|(key, _)| key != "CLAUDE_CONFIG_DIR"));
}

#[rstest]
#[case::normal(10, "normal")]
#[case::warning(80, "warning")]
#[case::critical(95, "critical")]
fn severities_have_names(#[case] percent: u16, #[case] name: &str) {
    assert_eq!(Severity::of(percent).name(), name);
}
