use std::path::{Path, PathBuf};

use super::*;
use crate::agents::Known;
use crate::test_util::{TempDir, fake_program, fixture};

const TIMEOUT: Duration = Duration::from_secs(20);

fn ask(folder: &Path) -> Ask {
    Ask {
        prompt: "Sum it up".into(),
        model: Some("opus".into()),
        effort: Some("low".into()),
        folder: folder.to_path_buf(),
        socket: None,
        ..Ask::default()
    }
}

fn faked(dir: &TempDir, name: &str, out: &str) -> PathBuf {
    let program = fake_program(dir.path(), name);
    std::fs::write(program.with_extension("out"), out).expect("write the canned output");
    program
}

fn called(agent: Known, program: &str, timeout: Duration) -> Call<'_> {
    Call { agent, program, home: None, timeout }
}

fn ran(agent: Known, program: &Path, folder: &Path) -> Result<Ran> {
    let program = program.display().to_string();
    run(&called(agent, &program, TIMEOUT), &ask(folder), &Arc::new(AtomicUsize::new(0)))
}

fn assert_claude_argv(argv: &str) {
    for wanted in [
        "\"--allowedTools\", \"Read,Grep,Glob,Bash(cornercase search:*),Bash(git log:*),Bash(git show:*),Bash(git diff:*)\"",
        "\"--disallowedTools\", \"Edit,Write,NotebookEdit,WebFetch,WebSearch,Task\"",
        "\"--model\", \"opus\"",
        "\"--effort\", \"low\"",
    ] {
        assert!(argv.contains(wanted), "{wanted} in {argv}");
    }
    assert!(argv.trim_end().ends_with("\"--\", \"Sum it up\"]"), "{argv}");
}

#[test]
fn claude_runs_in_the_folder_with_read_only_tools_and_the_prompt_last() {
    let dir = TempDir::new();
    let folder = TempDir::new();
    let program = faked(&dir, "claude", fixture!("claude/2.1.318/recap-ok.jsonl"));
    let result = ran(Known::Claude, &program, folder.path()).expect("a recap");
    assert_eq!(result.text, "hello");
    assert_eq!(result.model.as_deref(), Some("claude-opus-5-5"));
    assert_eq!(result.tokens, Used { input: 4, output: 73, cache_read: 26364, cache_write: 5199 });
    assert_eq!(result.steps, 1);
    assert_claude_argv(&std::fs::read_to_string(program.with_extension("argv")).expect("argv"));
    let cwd = std::fs::read_to_string(program.with_extension("cwd")).expect("cwd");
    assert_eq!(Path::new(cwd.trim()), folder.path());
}

#[test]
fn codex_runs_read_only_in_the_folder() {
    let dir = TempDir::new();
    let folder = TempDir::new();
    let program = faked(&dir, "codex", fixture!("codex/0.164.0/recap-ok.jsonl"));
    let result = ran(Known::Codex, &program, folder.path()).expect("a recap");
    assert_eq!(result.text, "hello");
    assert_eq!(result.tokens, Used { input: 2606, output: 47, cache_read: 27008, cache_write: 0 });
    let argv = std::fs::read_to_string(program.with_extension("argv")).expect("argv");
    assert!(argv.contains("\"--sandbox\", \"read-only\""), "{argv}");
    assert!(argv.contains("\"--model\", \"opus\""), "{argv}");
    assert!(argv.contains("model_reasoning_effort"), "{argv}");
    assert!(argv.trim_end().ends_with("\"--\", \"Sum it up\"]"), "{argv}");
}

#[test]
fn the_progress_counts_the_tools_the_agent_used() {
    let dir = TempDir::new();
    let folder = TempDir::new();
    let program = faked(&dir, "claude", fixture!("claude/2.1.318/recap-ok.jsonl"));
    let progress = Arc::new(AtomicUsize::new(0));
    let command = program.display().to_string();
    run(&called(Known::Claude, &command, TIMEOUT), &ask(folder.path()), &progress).expect("a recap");
    assert_eq!(progress.load(Ordering::Relaxed), 1);
}

#[test]
fn an_agent_that_is_signed_out_fails_with_its_own_words() {
    let dir = TempDir::new();
    let folder = TempDir::new();
    let program = faked(&dir, "claude", fixture!("claude/2.1.318/recap-failed.jsonl"));
    let error = ran(Known::Claude, &program, folder.path()).expect_err("signed out");
    assert_eq!(error.to_string(), "Not logged in · Please run /login");
}

#[test]
fn codex_that_hit_its_limit_fails_with_its_own_words() {
    let dir = TempDir::new();
    let folder = TempDir::new();
    let program = faked(&dir, "codex", fixture!("codex/0.164.0/recap-failed.jsonl"));
    let error = ran(Known::Codex, &program, folder.path()).expect_err("limit");
    assert!(error.to_string().contains("usage limit"), "{error}");
}

#[test]
fn a_program_that_exits_without_a_result_fails() {
    let dir = TempDir::new();
    let folder = TempDir::new();
    let program = faked(&dir, "claude", "");
    let error = ran(Known::Claude, &program, folder.path()).expect_err("no answer");
    assert_eq!(error.to_string(), "claude exited without answering");
}

#[test]
fn a_program_that_is_not_there_fails() {
    let folder = TempDir::new();
    let error = ran(Known::Claude, Path::new("/nonexistent/claude"), folder.path()).expect_err("missing");
    assert!(error.to_string().starts_with("could not run /nonexistent/claude"), "{error}");
}

#[test]
fn a_run_that_never_ends_times_out() {
    let dir = TempDir::new();
    let folder = TempDir::new();
    let program = faked(&dir, "claude", "");
    std::fs::write(program.with_extension("hold"), "").expect("hold");
    let command = program.display().to_string();
    let progress = Arc::new(AtomicUsize::new(0));
    let call = called(Known::Claude, &command, Duration::from_millis(300));
    let error = run(&call, &ask(folder.path()), &progress).expect_err("timeout");
    std::fs::remove_file(program.with_extension("hold")).expect("release");
    assert_eq!(error.to_string(), "claude did not answer in time");
}

#[test]
fn an_agent_without_a_recap_is_refused() {
    let folder = TempDir::new();
    let error = ran(Known::Gemini, Path::new("gemini"), folder.path()).expect_err("no recap");
    assert!(error.to_string().contains("cannot run without its interface"), "{error}");
}

#[test]
fn claude_argv_is_exactly_what_recap_always_ran() {
    let dir = TempDir::new();
    let folder = TempDir::new();
    let program = faked(&dir, "claude", fixture!("claude/2.1.318/recap-ok.jsonl"));
    ran(Known::Claude, &program, folder.path()).expect("a recap");
    let argv = std::fs::read_to_string(program.with_extension("argv")).expect("argv");
    assert_eq!(
        argv.trim_end(),
        concat!(
            "[\"-p\", \"--setting-sources\", \"\", \"--no-session-persistence\", \"--strict-mcp-config\", ",
            "\"--output-format\", \"stream-json\", \"--verbose\", \"--max-turns\", \"40\", ",
            "\"--allowedTools\", \"Read,Grep,Glob,Bash(cornercase search:*),Bash(git log:*),Bash(git show:*),Bash(git diff:*)\", ",
            "\"--disallowedTools\", \"Edit,Write,NotebookEdit,WebFetch,WebSearch,Task\", ",
            "\"--model\", \"opus\", \"--effort\", \"low\", \"--\", \"Sum it up\"]"
        )
    );
}

#[test]
fn codex_argv_is_exactly_what_recap_always_ran() {
    let dir = TempDir::new();
    let folder = TempDir::new();
    let program = faked(&dir, "codex", fixture!("codex/0.164.0/recap-ok.jsonl"));
    ran(Known::Codex, &program, folder.path()).expect("a recap");
    let argv = std::fs::read_to_string(program.with_extension("argv")).expect("argv");
    let wanted = format!(
        concat!(
            "[\"exec\", \"--json\", \"--skip-git-repo-check\", \"--ephemeral\", \"--sandbox\", \"read-only\", ",
            "\"-C\", {:?}, \"--model\", \"opus\", \"-c\", \"model_reasoning_effort=low\", \"--\", \"Sum it up\"]"
        ),
        folder.path().display().to_string()
    );
    assert_eq!(argv.trim_end(), wanted);
}

mod bots {
    use std::ffi::OsString;
    use std::sync::Mutex;

    use super::*;
    use crate::agents::claude::headless::rule_path;
    use crate::agents::headless::{Access, Action, Env, Tools, Watch};
    use crate::agents::probe::Running;
    use crate::bots::BOT_ENV;

    fn bot_ask(folder: &Path, access: Access, conversation: Option<&str>) -> Ask {
        Ask {
            prompt: "hi".into(),
            model: Some("opus".into()),
            effort: None,
            protected: vec!["bot.json", "chat.jsonl"],
            stops: &["kill-server", "restart", "update"],
            tools: Tools::Bot(access),
            conversation: conversation.map(str::to_string),
            ..ask(folder)
        }
    }

    fn argv_of(agent: Known, name: &str, out: &str, ask: &Ask) -> (String, TempDir) {
        let dir = TempDir::new();
        let program = faked(&dir, name, out);
        let command = program.display().to_string();
        run(&called(agent, &command, TIMEOUT), ask, &Arc::new(AtomicUsize::new(0))).expect("a turn");
        (std::fs::read_to_string(program.with_extension("argv")).expect("argv"), dir)
    }

    fn claude_argv(access: Access, conversation: Option<&str>) -> String {
        let folder = TempDir::new();
        let ask = bot_ask(folder.path(), access, conversation);
        let (argv, _dir) = argv_of(Known::Claude, "claude", fixture!("claude/2.1.325/bot-new.jsonl"), &ask);
        argv.replace(&folder.path().display().to_string(), "<folder>")
    }

    #[test]
    fn a_claude_bot_with_cornercase_access_may_write_only_in_its_folder() {
        let folder = TempDir::new();
        let ask = bot_ask(folder.path(), Access::Cornercase, None);
        let (argv, _dir) = argv_of(Known::Claude, "claude", fixture!("claude/2.1.325/bot-new.jsonl"), &ask);
        let rule = rule_path(&folder.path().display().to_string(), cfg!(windows));
        for wanted in [
            "\"--setting-sources\", \"user\"".to_string(),
            "\"--permission-mode\", \"dontAsk\"".to_string(),
            format!("Edit({rule}/**)"),
            format!("Edit({rule}/bot.json)"),
            format!("Edit({rule}/chat.jsonl)"),
            "Bash(cornercase:*)".to_string(),
            "Bash(cornercase kill-server:*)".to_string(),
            "\"--model\", \"opus\"".to_string(),
        ] {
            assert!(argv.contains(&wanted), "{wanted} in {argv}");
        }
        for unwanted in ["--resume", "--no-session-persistence", "--strict-mcp-config", "bypassPermissions"] {
            assert!(!argv.contains(unwanted), "{unwanted} in {argv}");
        }
        assert!(argv.trim_end().ends_with("\"--\", \"hi\"]"), "{argv}");
    }

    #[test]
    fn a_claude_bot_with_full_access_bypasses_permissions_and_resumes_its_session() {
        let argv = claude_argv(Access::Full, Some("s-1"));
        assert!(argv.contains("\"--permission-mode\", \"bypassPermissions\""), "{argv}");
        assert!(argv.contains("\"--resume\", \"s-1\""), "{argv}");
        assert!(argv.contains("Bash(cornercase restart:*)"), "{argv}");
        assert!(!argv.contains("--allowedTools"), "{argv}");
    }

    #[test]
    fn a_codex_bot_runs_in_its_folder_and_resumes_before_the_prompt() {
        let folder = TempDir::new();
        let ask = bot_ask(folder.path(), Access::Cornercase, None);
        let (argv, _dir) = argv_of(Known::Codex, "codex", fixture!("codex/0.164.0/bot-new.jsonl"), &ask);
        let wanted = format!(
            "[\"exec\", \"--json\", \"--skip-git-repo-check\", \"--sandbox\", \"workspace-write\", \"-C\", {:?}, \"--model\", \"opus\", \"--\", \"hi\"]",
            folder.path().display().to_string()
        );
        assert_eq!(argv.trim_end(), wanted);
        let ask = bot_ask(folder.path(), Access::Full, Some("t-1"));
        let (argv, _dir) = argv_of(Known::Codex, "codex", fixture!("codex/0.164.0/bot-resumed.jsonl"), &ask);
        assert!(argv.contains("\"--dangerously-bypass-approvals-and-sandbox\""), "{argv}");
        assert!(argv.trim_end().ends_with("\"resume\", \"t-1\", \"--\", \"hi\"]"), "{argv}");
        assert!(!argv.contains("--ephemeral"), "{argv}");
    }

    fn with_env(folder: &Path, env: Env) -> (Vec<String>, TempDir) {
        let dir = TempDir::new();
        let program = faked(&dir, "claude", fixture!("claude/2.1.325/bot-new.jsonl"));
        std::fs::write(program.with_extension("vars"), "CORNERCASE_BOT CORNERCASE_SOCKET PATH").expect("names");
        let ask = Ask { env, socket: Some("/the/socket".into()), ..bot_ask(folder, Access::Full, None) };
        let command = program.display().to_string();
        run(&called(Known::Claude, &command, TIMEOUT), &ask, &Arc::new(AtomicUsize::new(0))).expect("a turn");
        let text = std::fs::read_to_string(program.with_extension("env")).expect("env");
        (text.lines().map(str::to_string).collect(), dir)
    }

    #[test]
    fn a_bot_turn_gets_its_id_the_socket_and_the_servers_binary_first_on_the_path() {
        let folder = TempDir::new();
        let bin = Path::new("/the/server/bin");
        let path = crate::bots::turn::path_with(bin, std::env::var_os("PATH").as_deref()).expect("a path");
        let (lines, _dir) = with_env(folder.path(), vec![(BOT_ENV, Some(OsString::from("prs"))), ("PATH", Some(path))]);
        assert_eq!(lines[0], "CORNERCASE_BOT=prs");
        assert_eq!(lines[1], "CORNERCASE_SOCKET=/the/socket");
        let path = lines[2].strip_prefix("PATH=").expect("the path");
        assert_eq!(std::env::split_paths(path).next().as_deref(), Some(bin));
    }

    #[test]
    fn a_variable_the_ask_removes_is_unset_for_the_agent() {
        let folder = TempDir::new();
        let (lines, _dir) = with_env(folder.path(), vec![("PATH", None)]);
        assert_eq!(lines[2], "PATH");
    }

    #[test]
    fn the_steps_reach_the_watcher_in_order_and_the_session_comes_out() {
        let dir = TempDir::new();
        let folder = TempDir::new();
        let program = faked(&dir, "claude", fixture!("claude/2.1.325/bot-new.jsonl"));
        let seen = Arc::new(Mutex::new(Vec::<Action>::new()));
        let sink = Arc::clone(&seen);
        let watch =
            Watch { steps: Some(Box::new(move |step| sink.lock().expect("lock").push(step))), ..Watch::default() };
        let command = program.display().to_string();
        let call = called(Known::Claude, &command, TIMEOUT);
        let result = run_watched(&call, &bot_ask(folder.path(), Access::Full, None), watch).expect("a turn");
        assert_eq!(result.session.as_deref(), Some("8b610e1d-23f2-4ef4-8666-258ef8b082dc"));
        assert_eq!(result.text, "Two are blocked.");
        let steps: Vec<_> =
            seen.lock().expect("lock").iter().map(|step| (step.tool.clone(), step.text.clone())).collect();
        assert_eq!(
            steps,
            [
                (String::from("Bash"), String::from("cornercase status --json")),
                (String::from("Read"), String::from("Read /work/bots/prs/bot.md")),
            ]
        );
    }

    fn held(dir: &TempDir, name: &str) -> PathBuf {
        let program = faked(dir, name, "");
        std::fs::write(program.with_extension("hold"), "").expect("hold");
        program
    }

    #[test]
    fn stopping_a_turn_ends_its_process() {
        let dir = TempDir::new();
        let folder = TempDir::new();
        let program = held(&dir, "claude");
        let running = Running::default();
        let (command, ask) = (program.display().to_string(), bot_ask(folder.path(), Access::Full, None));
        let handle = {
            let watch = Watch { running: Some(running.clone()), ..Watch::default() };
            let call_agent = Known::Claude;
            std::thread::spawn(move || run_watched(&called(call_agent, &command, TIMEOUT), &ask, watch))
        };
        crate::test_util::wait_until("the agent started", || program.with_extension("argv").exists());
        assert!(running.stop());
        let error = handle.join().expect("the run").expect_err("stopped");
        assert_eq!(error.to_string(), "claude exited without answering");
        std::fs::remove_file(program.with_extension("hold")).expect("release");
    }

    #[test]
    fn stopping_before_the_process_exists_still_ends_it() {
        let dir = TempDir::new();
        let folder = TempDir::new();
        let program = held(&dir, "claude");
        let running = Running::default();
        assert!(!running.stop());
        let watch = Watch { running: Some(running), ..Watch::default() };
        let command = program.display().to_string();
        let call = called(Known::Claude, &command, TIMEOUT);
        let error = run_watched(&call, &bot_ask(folder.path(), Access::Full, None), watch).expect_err("stopped");
        assert_eq!(error.to_string(), "claude exited without answering");
        std::fs::remove_file(program.with_extension("hold")).expect("release");
    }

    #[cfg(unix)]
    #[test]
    fn stopping_a_turn_ends_what_the_agent_started_too() {
        use crate::test_util::write_executable;
        let dir = TempDir::new();
        let folder = TempDir::new();
        let child = dir.path().join("child.pid");
        let script = format!("#!/bin/sh\nsleep 60 &\necho $! > '{}'\nwait\n", child.display());
        let program = dir.path().join("claude");
        write_executable(&program, &script);
        let running = Running::default();
        let (command, ask) = (program.display().to_string(), bot_ask(folder.path(), Access::Full, None));
        let watch = Watch { running: Some(running.clone()), ..Watch::default() };
        let handle = std::thread::spawn(move || run_watched(&called(Known::Claude, &command, TIMEOUT), &ask, watch));
        crate::test_util::wait_until("the child started", || {
            std::fs::read_to_string(&child).is_ok_and(|text| text.trim().parse::<i32>().is_ok())
        });
        let pid: i32 = std::fs::read_to_string(&child).expect("pid").trim().parse().expect("a pid");
        assert!(crate::process::running(pid, None));
        running.stop();
        handle.join().expect("the run").expect_err("stopped");
        crate::test_util::wait_until("the child ended", || !crate::process::running(pid, None));
    }

    #[cfg(unix)]
    #[test]
    fn a_turn_that_runs_out_of_time_has_what_the_agent_started_ended_too() {
        use crate::test_util::write_executable;
        let dir = TempDir::new();
        let folder = TempDir::new();
        let child = dir.path().join("child.pid");
        let script = format!("#!/bin/sh\nsleep 60 &\necho $! > '{}'\nwait\n", child.display());
        let program = dir.path().join("claude");
        write_executable(&program, &script);
        let command = program.display().to_string();
        let call = called(Known::Claude, &command, Duration::from_millis(600));
        let error =
            run_watched(&call, &bot_ask(folder.path(), Access::Full, None), Watch::default()).expect_err("slow");
        assert_eq!(error.to_string(), "claude did not answer in time");
        let pid: i32 = std::fs::read_to_string(&child).expect("pid").trim().parse().expect("a pid");
        crate::test_util::wait_until("the child ended", || !crate::process::running(pid, None));
    }

    #[test]
    fn a_turn_that_runs_out_of_time_has_its_process_ended() {
        let dir = TempDir::new();
        let folder = TempDir::new();
        let program = held(&dir, "claude");
        let command = program.display().to_string();
        let call = called(Known::Claude, &command, Duration::from_millis(300));
        let error =
            run_watched(&call, &bot_ask(folder.path(), Access::Full, None), Watch::default()).expect_err("slow");
        assert_eq!(error.to_string(), "claude did not answer in time");
        std::fs::remove_file(program.with_extension("hold")).expect("release");
    }
}
