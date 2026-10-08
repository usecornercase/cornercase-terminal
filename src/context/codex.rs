use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

use super::{Context, TAIL, percent};
use crate::process;

pub(super) fn rollout_path(pid: i32, since: SystemTime) -> Option<PathBuf> {
    let mut pids = vec![pid];
    let mut paths = Vec::new();
    let mut at = 0;
    while at < pids.len() && at < 16 {
        let pid = pids[at];
        at += 1;
        if let Some(home) = home(pid) {
            paths.extend(rollouts(pid, &home));
        }
        for child in process::children(pid) {
            let args = process::args(child);
            let codex = args.iter().take(2).any(|arg| is_codex(arg) || arg.ends_with("/codex/bin/codex.js"));
            if codex && !pids.contains(&child) {
                pids.push(child);
            }
        }
    }
    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        return served(pid, since);
    }
    single(paths.into_iter().filter(|p| metadata(p).is_some()))
}

fn served(pid: i32, since: SystemTime) -> Option<PathBuf> {
    let codex_home = home(pid)?;
    let cwd = folder(pid)?;
    let daemon = daemon(&codex_home)?;
    let mut paths = rollouts(daemon, &codex_home);
    paths.sort();
    paths.dedup();
    let path = single(paths.into_iter().filter(|path| {
        std::fs::metadata(path).and_then(|m| m.modified()).is_ok_and(|modified| modified >= since)
            && metadata(path).is_some_and(|(_, dir)| dir.and_then(|d| d.canonicalize().ok()).as_ref() == Some(&cwd))
    }))?;
    let agents = process::all().into_iter().filter(|&other| {
        let args = process::args(other);
        args.first().is_some_and(|arg| is_codex(arg))
            && !args.iter().any(|arg| arg == "app-server")
            && folder(other).as_ref() == Some(&cwd)
            && home(other).as_ref() == Some(&codex_home)
    });
    single(agents).map(|_| path)
}

fn daemon(home: &Path) -> Option<i32> {
    let text = std::fs::read_to_string(home.join("app-server-daemon/daemon.pid")).ok()?;
    let pid = i32::try_from(serde_json::from_str::<Value>(&text).ok()?.get("pid")?.as_i64()?).ok()?;
    let args = process::args(pid);
    (args.iter().take(2).any(|arg| is_codex(arg)) && args.iter().any(|arg| arg == "app-server")).then_some(pid)
}

fn rollouts(pid: i32, home: &Path) -> Vec<PathBuf> {
    let sessions = home.join("sessions");
    process::open_files(pid)
        .into_iter()
        .filter(|path| path.starts_with(&sessions) && session_id(path).is_some())
        .collect()
}

fn single<T>(mut items: impl Iterator<Item = T>) -> Option<T> {
    let item = items.next()?;
    items.next().is_none().then_some(item)
}

fn is_codex(arg: &str) -> bool {
    Path::new(arg).file_name().is_some_and(|name| name == "codex")
}

fn folder(pid: i32) -> Option<PathBuf> {
    process::cwd(pid)?.canonicalize().ok()
}

fn home(pid: i32) -> Option<PathBuf> {
    let env = process::env(pid);
    let var = |name| env.iter().find(|(key, value)| key == name && !value.is_empty()).map(|(_, v)| PathBuf::from(v));
    let path = var("CODEX_HOME").or_else(|| var("HOME").map(|p| p.join(".codex")))?;
    let path = if path.is_absolute() { path } else { process::cwd(pid)?.join(path) };
    path.canonicalize().ok()
}

fn session_id(path: &Path) -> Option<&str> {
    let name = path.file_name()?.to_str()?.strip_prefix("rollout-")?.strip_suffix(".jsonl")?;
    let ids = name.get(20..)?;
    let id = ids.split('_').next()?;
    (id.len() == 36 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')).then_some(id)
}

fn metadata(path: &Path) -> Option<(String, Option<PathBuf>)> {
    let mut bytes = Vec::new();
    BufReader::new(File::open(path).ok()?.take(64 * 1024)).read_until(b'\n', &mut bytes).ok()?;
    let end = bytes.iter().position(|b| *b == b'\n')?;
    let entry: Value = serde_json::from_slice(&bytes[..end]).ok()?;
    let payload = entry.get("payload")?;
    let id = payload.get("id")?.as_str()?;
    (entry.get("type")?.as_str()? == "session_meta"
        && matches!(payload.get("source")?.as_str(), Some("cli" | "exec" | "vscode"))
        && session_id(path) == Some(id))
    .then(|| (id.to_string(), payload.get("cwd").and_then(Value::as_str).map(PathBuf::from)))
}

#[derive(Debug)]
pub(super) struct Rollout {
    pub path: PathBuf,
    read: u64,
    skipping: bool,
    stamp: Option<(u64, u64, SystemTime)>,
    id: Option<String>,
    model: Option<String>,
    percent: Option<u16>,
    turn: bool,
}

impl Rollout {
    pub fn new(path: PathBuf) -> Self {
        Self { path, read: 0, skipping: false, stamp: None, id: None, model: None, percent: None, turn: false }
    }

    pub fn context(&self) -> Option<Context> {
        Some(Context { model: self.model.clone()?, percent: self.percent })
    }

    pub fn turn(&self) -> bool {
        self.turn
    }

    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub fn update(&mut self) {
        if self.read_file().is_none() {
            *self = Self::new(std::mem::take(&mut self.path));
        }
    }

    fn read_file(&mut self) -> Option<()> {
        let info = std::fs::metadata(&self.path).ok()?;
        let stamp = (info.ino(), info.len(), info.modified().ok()?);
        if self.stamp == Some(stamp) && self.read == info.len() {
            return Some(());
        }
        if self.stamp.is_some_and(|s| s.0 != stamp.0 || stamp.1 < s.1 || (stamp.1 == s.1 && stamp.2 != s.2)) {
            *self = Self::new(self.path.clone());
        }
        if self.id.is_none() {
            self.id = Some(metadata(&self.path)?.0);
        }
        let start = if self.read == 0 { info.len().saturating_sub(TAIL) } else { self.read };
        let mut file = File::open(&self.path).ok()?;
        file.seek(SeekFrom::Start(start)).ok()?;
        let mut bytes = Vec::new();
        file.take((info.len() - start).min(TAIL)).read_to_end(&mut bytes).ok()?;
        let Some(end) = bytes.iter().rposition(|b| *b == b'\n').map(|i| i + 1) else {
            if bytes.len() as u64 == TAIL {
                self.read = start + TAIL;
                self.skipping = true;
                self.model = None;
                self.percent = None;
                self.stamp = Some(stamp);
            }
            return Some(());
        };
        let from = if self.skipping || (self.read == 0 && start > 0) {
            bytes.iter().position(|b| *b == b'\n').map_or(end, |i| i + 1)
        } else {
            0
        };
        for line in bytes[from..end].split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
            self.apply(line);
        }
        self.read = start + end as u64;
        self.skipping = false;
        self.stamp = Some(stamp);
        Some(())
    }

    fn set_model(&mut self, model: Option<&Value>) {
        let model = model.and_then(Value::as_str).filter(|m| !m.trim().is_empty()).map(str::to_string);
        if model != self.model {
            self.model = model;
            self.percent = None;
        }
    }

    fn apply(&mut self, line: &[u8]) {
        let Ok(entry) = serde_json::from_slice::<Value>(line) else {
            self.percent = None;
            return;
        };
        let payload = &entry["payload"];
        match entry["type"].as_str() {
            Some("turn_context") => self.set_model(payload.get("model")),
            Some("compacted") => self.percent = None,
            Some("event_msg") => match payload["type"].as_str() {
                Some("thread_settings_applied") => {
                    if payload.get("thread_id").is_none_or(|id| id.as_str() == self.id.as_deref()) {
                        self.set_model(payload["thread_settings"].get("model"));
                    }
                }
                Some("token_count") => {
                    if payload.get("info").is_some_and(Value::is_null) {
                        return;
                    }
                    let info = &payload["info"];
                    self.percent = info["last_token_usage"]["total_tokens"]
                        .as_u64()
                        .zip(info["model_context_window"].as_u64().filter(|window| *window > 0))
                        .map(|(tokens, window)| percent(tokens, window));
                }
                Some("context_compacted") => self.percent = None,
                Some("task_started") => self.turn = true,
                Some("task_complete" | "turn_aborted") => self.turn = false,
                _ => {}
            },
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use rstest::rstest;

    use super::*;
    use crate::context::Pane;
    use crate::test_util::{FakeCodex, Sleeper, TempDir, wait_until, write_executable};

    const ID: &str = "019a1234-5678-7000-8000-000000000001";
    const CONTEXT: &str = include_str!("../../tests/fixtures/codex/0.160.0/context.jsonl");
    const MODEL_CHANGE: &str = include_str!("../../tests/fixtures/codex/0.160.0/model-change.jsonl");
    const COMPACTED: &str = include_str!("../../tests/fixtures/codex/0.160.0/compacted.jsonl");
    const UNAVAILABLE: &str = include_str!("../../tests/fixtures/codex/0.160.0/unavailable.jsonl");
    const MALFORMED: &str = include_str!("../../tests/fixtures/codex/0.160.0/malformed.jsonl");
    const VSCODE: &str = include_str!("../../tests/fixtures/codex/0.160.0/vscode-context.jsonl");
    const STARTED: &str = include_str!("../../tests/fixtures/codex/0.160.0/turn-started.jsonl");
    const COMPLETE: &str = include_str!("../../tests/fixtures/codex/0.160.0/turn-complete.jsonl");
    const ABORTED: &str = include_str!("../../tests/fixtures/codex/0.160.0/turn-aborted.jsonl");

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

        let found = file.append(
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":null,\"rate_limits\":{}}}\n",
        );

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
        let file =
            Written::new(&format!("{CONTEXT}{}\n{}\n", filler.repeat(1100), CONTEXT.lines().last().expect("usage")));

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
        let compacted =
            format!("{{\"type\":\"compacted\",\"payload\":{{\"message\":\"{}\"}}}}\n", "x".repeat(2_100_000));

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
        let mut child = std::process::Command::new(&fake.script)
            .args(fake.args())
            .env("CODEX_HOME", fake.dir.path())
            .spawn()
            .expect("spawn fake codex");
        let pid = i32::try_from(child.id()).expect("pid");
        let mut pane = Pane::default();
        wait_until("the fake agent opens its rollout", || {
            pane.update_codex(pid);
            pane.context().is_some()
        });
        assert_eq!(pane.context().cloned(), Some(shown("gpt-5.4", Some(20))));
        fake.signal("quit", "");
        child.wait().expect("agent exits");
        wait_until("the line clears", || {
            pane.update_codex(pid);
            pane.context().is_none()
        });
    }

    #[test]
    fn ambiguous_open_rollouts_are_hidden_and_subagent_rollouts_are_excluded() {
        let fake = FakeCodex::new(ID, "gpt-5.4", false);
        let other =
            fake.rollout.with_file_name("rollout-2026-10-05T12-00-00-019a1234-5678-7000-8000-000000000002.jsonl");
        let content = CONTEXT.replace(ID, "019a1234-5678-7000-8000-000000000002");
        std::fs::write(&other, &content).expect("other rollout");
        write_executable(
            &fake.script,
            "#!/bin/sh\nexec 3>> \"$1\"\nexec 4>> \"$2\"\nwhile [ ! -e \"$3/quit\" ]; do sleep 0.02; done\n",
        );
        let mut child = std::process::Command::new(&fake.script)
            .args([&fake.rollout, &other, &fake.home])
            .env("CODEX_HOME", &fake.home)
            .spawn()
            .expect("spawn fake codex");
        let pid = i32::try_from(child.id()).expect("pid");
        wait_until("both rollouts are open", || process::open_files(pid).contains(&other));
        assert_eq!(rollout_path(pid, SystemTime::UNIX_EPOCH), None);

        std::fs::write(&other, content.replace("\"cli\"", "{\"subagent\":{}}")).expect("subagent metadata");
        assert_eq!(rollout_path(pid, SystemTime::UNIX_EPOCH), Some(fake.rollout.clone()));
        fake.signal("quit", "");
        child.wait().expect("fake agent exits");
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
        let mut agent = std::process::Command::new(&fake.script)
            .args([&fake.rollout, &fake.home, &child])
            .env("CODEX_HOME", &fake.home)
            .spawn()
            .expect("spawn fake codex");
        let pid = i32::try_from(agent.id()).expect("pid");
        wait_until("the Codex child runs without an environment", || {
            process::children(pid)
                .into_iter()
                .any(|c| process::args(c).get(1).is_some_and(|a| Path::new(a) == child) && home(c).is_none())
        });

        assert_eq!(rollout_path(pid, SystemTime::UNIX_EPOCH), Some(fake.rollout.clone()));
        fake.signal("quit", "");
        agent.wait().expect("fake agent exits");
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
            pane.update_codex(agent.pid());
            pane.context().is_some()
        });
        assert_eq!(pane.context().cloned(), Some(shown("gpt-5.4", Some(20))));
        drop(daemon);
        wait_until("the line clears", || {
            pane.update_codex(agent.pid());
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
}
