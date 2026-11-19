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
fn an_agent_without_a_spec_is_refused() {
    let folder = TempDir::new();
    let error = ran(Known::Grok, Path::new("grok"), folder.path()).expect_err("no spec");
    assert!(error.to_string().contains("cannot run without its interface"), "{error}");
}

#[test]
fn an_agent_that_cannot_write_a_recap_is_refused_before_anything_starts() {
    let dir = TempDir::new();
    let folder = TempDir::new();
    let program = faked(&dir, "gemini", fixture!("gemini/0.67.0/bot-new.jsonl"));
    let error = ran(Known::Gemini, &program, folder.path()).expect_err("no recap");
    assert_eq!(error.to_string(), "gemini cannot write a recap");
    assert!(!program.with_extension("argv").exists());
}

#[test]
fn an_access_the_agent_lacks_is_refused_before_anything_starts() {
    use crate::agents::headless::{Access, Tools};
    let dir = TempDir::new();
    let folder = TempDir::new();
    let program = faked(&dir, "opencode", fixture!("opencode/1.18.35/bot-new.jsonl"));
    let ask = Ask { tools: Tools::Bot(Access::Cornercase), ..ask(folder.path()) };
    let command = program.display().to_string();
    let error = run(&called(Known::Opencode, &command, TIMEOUT), &ask, &Arc::new(AtomicUsize::new(0)))
        .expect_err("no cornercase access");
    assert_eq!(error.to_string(), "opencode cannot be a bot with cornercase access");
    assert!(!program.with_extension("argv").exists());
}

#[test]
fn an_agent_that_does_not_ask_for_it_keeps_its_standard_error_to_itself() {
    let dir = TempDir::new();
    let folder = TempDir::new();
    let program = faked(&dir, "claude", "");
    std::fs::write(program.with_extension("err"), "Error: nope\n").expect("write the canned error");
    std::fs::write(program.with_extension("code"), "1").expect("write the exit code");
    let error = ran(Known::Claude, &program, folder.path()).expect_err("no answer");
    assert_eq!(error.to_string(), "claude exited without answering");
}

fn recap_argv(agent: Known, out: &str, vars: &str) -> (String, String, TempDir) {
    let dir = TempDir::new();
    let folder = TempDir::new();
    let program = faked(&dir, agent.name(), out);
    std::fs::write(program.with_extension("vars"), vars).expect("names");
    ran(agent, &program, folder.path()).expect("a recap");
    let argv = std::fs::read_to_string(program.with_extension("argv")).expect("argv");
    let env = std::fs::read_to_string(program.with_extension("env")).unwrap_or_default();
    (argv.trim_end().replace(&format!("{:?}", folder.path().display().to_string()), "<folder>"), env, folder)
}

#[test]
fn claude_argv_is_exactly_what_recap_always_ran() {
    let (argv, _, _folder) = recap_argv(Known::Claude, fixture!("claude/2.1.318/recap-ok.jsonl"), "");
    assert_eq!(
        argv,
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
    let (argv, _, _folder) = recap_argv(Known::Codex, fixture!("codex/0.164.0/recap-ok.jsonl"), "");
    assert_eq!(
        argv,
        concat!(
            "[\"exec\", \"--json\", \"--skip-git-repo-check\", \"--ephemeral\", \"--sandbox\", \"read-only\", ",
            "\"-C\", <folder>, \"--model\", \"opus\", \"-c\", \"model_reasoning_effort=low\", \"--\", \"Sum it up\"]"
        )
    );
}

#[test]
fn copilot_argv_for_a_recap_allows_no_tool() {
    let (argv, _, _folder) = recap_argv(Known::Copilot, fixture!("copilot/1.0.98/bot-resumed.jsonl"), "");
    assert_eq!(
        argv,
        concat!(
            "[\"--output-format\", \"json\", \"--no-auto-update\", \"--no-ask-user\", ",
            "\"--disallow-temp-dir\", \"--disable-builtin-mcps\", \"--model\", \"opus\", ",
            "\"--reasoning-effort\", \"low\", \"--prompt=Sum it up\"]"
        )
    );
}

#[test]
fn opencode_argv_for_a_recap_has_no_bypass() {
    let out = fixture!("opencode/1.18.35/bot-resumed.jsonl");
    let (argv, env, _folder) = recap_argv(Known::Opencode, out, "OPENCODE_PERMISSION");
    assert_eq!(argv, "[\"run\", \"--format\", \"json\", \"--model\", \"opus\", \"--\", \"Sum it up\"]");
    let rules = env.trim_end().strip_prefix("OPENCODE_PERMISSION=").expect("the read-only rules");
    let rules: Value = serde_json::from_str(rules).expect("json");
    assert_eq!((&rules["*"], &rules["bash"], &rules["read"]), (&"deny".into(), &"deny".into(), &"allow".into()));
}

#[rstest::rstest]
#[case::copilot(Known::Copilot, fixture!("copilot/1.0.98/bot-resumed.jsonl"))]
#[case::opencode(Known::Opencode, fixture!("opencode/1.18.35/bot-resumed.jsonl"))]
fn a_recap_is_the_agents_last_answer(#[case] agent: Known, #[case] out: &str) {
    let dir = TempDir::new();
    let folder = TempDir::new();
    let program = faked(&dir, agent.name(), out);
    let recap = ran(agent, &program, folder.path()).expect("a recap");
    assert!(!recap.text.is_empty() && out.contains(&recap.text), "{}", recap.text);
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
        crate::agents::headless::bot_ask(folder, Tools::Bot(access), conversation)
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
            "Bash(gh pr list:*)".to_string(),
            "Bash(git *--output*)".to_string(),
            format!("Edit({rule}/-*)"),
            "\"--model\", \"opus\"".to_string(),
        ] {
            assert!(argv.contains(&wanted), "{wanted} in {argv}");
        }
        let unwanted = ["--resume", "--no-session-persistence", "--strict-mcp-config", "bypassPermissions"];
        for unwanted in unwanted.into_iter().chain(["Bash(git log:*)", "Bash(git diff:*)", "Bash(git status:*)"]) {
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
        let args = args_of(&crate::agents::codex::headless::command("codex", &ask));
        let wanted = [
            "exec",
            "--json",
            "--skip-git-repo-check",
            "--sandbox",
            "workspace-write",
            "-C",
            &folder.path().display().to_string(),
            "--model",
            "opus",
            "--",
            "hi",
        ];
        assert_eq!(args, wanted);
        let ask = bot_ask(folder.path(), Access::Full, Some("t-1"));
        let (resumed, _dir) = argv_of(Known::Codex, "codex", fixture!("codex/0.164.0/bot-resumed.jsonl"), &ask);
        assert!(resumed.contains("\"--dangerously-bypass-approvals-and-sandbox\""), "{resumed}");
        assert!(resumed.trim_end().ends_with("\"resume\", \"t-1\", \"--\", \"hi\"]"), "{resumed}");
        assert!(!resumed.contains("--ephemeral"), "{resumed}");
    }

    #[rstest::rstest]
    #[case::copilot(Known::Copilot, fixture!("copilot/1.0.98/bot-failed.txt"))]
    #[case::cursor(Known::Cursor, fixture!("cursor/2026.10.22-27dcf5f/bot-failed.txt"))]
    fn an_agent_that_fails_on_its_standard_error_says_why(#[case] agent: Known, #[case] said: &str) {
        let dir = TempDir::new();
        let folder = TempDir::new();
        let program = faked(&dir, agent.name(), "");
        std::fs::write(program.with_extension("err"), said).expect("write the canned error");
        std::fs::write(program.with_extension("code"), "1").expect("write the exit code");
        let ask = bot_ask(folder.path(), Access::Full, None);
        let command = program.display().to_string();
        let error = run(&called(agent, &command, TIMEOUT), &ask, &Arc::new(AtomicUsize::new(0)));
        let words = crate::text::truncate_right(said.lines().next().expect("a line").trim(), 200);
        assert_eq!(
            error.expect_err("no answer").to_string(),
            format!("{} exited without answering: {words}", agent.name())
        );
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
    fn a_bot_turn_gets_its_id_the_socket_and_the_folder_put_first_on_its_path() {
        let folder = TempDir::new();
        let first = Path::new("/the/server/bin");
        let path = crate::bots::turn::path_with(first, std::env::var_os("PATH").as_deref()).expect("a path");
        let (lines, _dir) = with_env(folder.path(), vec![(BOT_ENV, Some(OsString::from("prs"))), ("PATH", Some(path))]);
        assert_eq!(lines[0], "CORNERCASE_BOT=prs");
        assert_eq!(lines[1], "CORNERCASE_SOCKET=/the/socket");
        let path = lines[2].strip_prefix("PATH=").expect("the path");
        assert_eq!(std::env::split_paths(path).next().as_deref(), Some(first));
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
    fn a_stopped_opencode_turn_is_not_an_answer_even_with_text_already_said() {
        let dir = TempDir::new();
        let folder = TempDir::new();
        let out = concat!(
            r#"{"type":"tool_use","sessionID":"s","part":{"tool":"read","state":{"input":{"filePath":"a"}}}}"#,
            "\n",
            r#"{"type":"text","sessionID":"s","part":{"type":"text","text":"Half a thought"}}"#,
            "\n"
        );
        let program = faked(&dir, "opencode", out);
        std::fs::write(program.with_extension("hold"), "").expect("hold");
        let (running, seen) = (Running::default(), Arc::new(AtomicUsize::new(0)));
        let (command, ask) = (program.display().to_string(), bot_ask(folder.path(), Access::Full, None));
        let handle = {
            let sink = Arc::clone(&seen);
            let steps: Box<dyn FnMut(Action) + Send> = Box::new(move |_| _ = sink.fetch_add(1, Ordering::Relaxed));
            let watch = Watch { running: Some(running.clone()), steps: Some(steps), ..Watch::default() };
            std::thread::spawn(move || run_watched(&called(Known::Opencode, &command, TIMEOUT), &ask, watch))
        };
        crate::test_util::wait_until("opencode spoke", || seen.load(Ordering::Relaxed) > 0);
        assert!(running.stop());
        let error = handle.join().expect("the run").expect_err("stopped");
        assert_eq!(error.to_string(), "opencode exited without answering");
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

    #[rstest::rstest]
    #[case::gemini(
        Known::Gemini,
        fixture!("gemini/0.67.0/bot-new.jsonl"),
        fixture!("gemini/0.67.0/bot-resumed.jsonl"),
        "\"--resume\", \"{}\"",
        "run_shell_command"
    )]
    #[case::opencode(
        Known::Opencode,
        fixture!("opencode/1.18.35/bot-new.jsonl"),
        fixture!("opencode/1.18.35/bot-resumed.jsonl"),
        "\"--session\", \"{}\"",
        "bash"
    )]
    #[case::cursor(
        Known::Cursor,
        fixture!("cursor/2026.10.22-27dcf5f/bot-new.jsonl"),
        fixture!("cursor/2026.10.22-27dcf5f/bot-resumed.jsonl"),
        "\"--resume\", \"{}\"",
        "shell"
    )]
    #[case::copilot(
        Known::Copilot,
        fixture!("copilot/1.0.98/bot-new.jsonl"),
        fixture!("copilot/1.0.98/bot-resumed.jsonl"),
        "\"--resume={}\"",
        "bash"
    )]
    fn a_full_access_bot_starts_a_session_and_continues_it(
        #[case] agent: Known,
        #[case] new: &str,
        #[case] resumed: &str,
        #[case] resume: &str,
        #[case] first_tool: &str,
    ) {
        let dir = TempDir::new();
        let folder = TempDir::new();
        let program = faked(&dir, agent.name(), new);
        let seen = Arc::new(Mutex::new(Vec::<Action>::new()));
        let sink = Arc::clone(&seen);
        let watch =
            Watch { steps: Some(Box::new(move |step| sink.lock().expect("lock").push(step))), ..Watch::default() };
        let command = program.display().to_string();
        let call = called(agent, &command, TIMEOUT);
        let first = run_watched(&call, &bot_ask(folder.path(), Access::Full, None), watch).expect("a first turn");
        let session = first.session.clone().expect("a session");
        assert_eq!(seen.lock().expect("lock")[0].tool, first_tool);
        assert!(first.steps > 0 && !first.text.is_empty());
        std::fs::write(program.with_extension("out"), resumed).expect("canned resume");
        let ask = bot_ask(folder.path(), Access::Full, Some(&session));
        let second = run_watched(&call, &ask, Watch::default()).expect("a resumed turn");
        assert_eq!(second.session.as_deref(), Some(session.as_str()));
        let argvs = std::fs::read_to_string(program.with_extension("argv")).expect("argv");
        assert!(!argvs.lines().next().expect("first").contains(&resume.replace("{}", &session)));
        assert!(argvs.lines().last().expect("second").contains(&resume.replace("{}", &session)), "{argvs}");
    }

    fn opencode_turn(out: &str) -> Result<Ran> {
        let dir = TempDir::new();
        let folder = TempDir::new();
        let program = faked(&dir, "opencode", out);
        let command = program.display().to_string();
        let ask = bot_ask(folder.path(), Access::Full, Some("ses_e2a1b2c3d4ffeAbCdEfGhIjKlM"));
        run_watched(&called(Known::Opencode, &command, TIMEOUT), &ask, Watch::default())
    }

    #[test]
    fn an_opencode_bot_that_compacts_first_answers_with_its_real_answer() {
        let ran = opencode_turn(fixture!("opencode/1.18.35/bot-compacted.jsonl")).expect("a turn");
        assert_eq!(ran.text, "The keeper's name was Silas.");
    }

    #[test]
    fn an_opencode_run_that_says_nothing_exited_without_answering() {
        let out = r#"{"type":"step_finish","sessionID":"s","part":{"reason":"stop","tokens":{}}}"#;
        let error = opencode_turn(out).expect_err("no answer");
        assert_eq!(error.to_string(), "opencode exited without answering");
    }

    #[cfg(not(windows))]
    #[test]
    fn a_gemini_bot_with_cornercase_access_writes_its_rules_and_runs_with_them() {
        let folder = TempDir::new();
        let ask = bot_ask(folder.path(), Access::Cornercase, None);
        let (argv, _dir) = argv_of(Known::Gemini, "gemini", fixture!("gemini/0.67.0/bot-new.jsonl"), &ask);
        let rules = folder.path().join(".gemini").join("cornercase-policy.toml");
        let text = std::fs::read_to_string(&rules).expect("the rules file");
        assert!(text.contains(r"\.[gG][eE][mM][iI][nN][iI]"), "{text}");
        assert!(text.contains(r"\.[eE][nN][vV]"), "{text}");
        assert!(argv.contains(&format!("{:?}", rules.display().to_string())), "{argv}");
        assert!(argv.contains("\"--approval-mode\", \"default\""), "{argv}");
    }

    #[cfg(not(windows))]
    #[test]
    fn a_copilot_bot_with_cornercase_access_runs_with_its_rules() {
        let folder = TempDir::new();
        let ask = bot_ask(folder.path(), Access::Cornercase, None);
        let (argv, _dir) = argv_of(Known::Copilot, "copilot", fixture!("copilot/1.0.98/bot-new.jsonl"), &ask);
        assert!(argv.contains("\"--allow-tool\", \"shell(cornercase:*)\""), "{argv}");
        assert!(argv.contains("\"--deny-tool\", \"shell(cornercase restart:*)\""), "{argv}");
        assert!(argv.contains(&format!("write({}/bot.json)", folder.path().display())), "{argv}");
    }

    #[cfg(windows)]
    #[rstest::rstest]
    #[case::gemini(Known::Gemini)]
    #[case::copilot(Known::Copilot)]
    fn gemini_and_copilot_have_full_access_only_on_windows(#[case] agent: Known) {
        let dir = TempDir::new();
        let folder = TempDir::new();
        let program = faked(&dir, agent.name(), "");
        let ask = bot_ask(folder.path(), Access::Cornercase, None);
        let command = program.display().to_string();
        let error = run(&called(agent, &command, TIMEOUT), &ask, &Arc::new(AtomicUsize::new(0))).expect_err("refused");
        assert!(error.to_string().contains("cannot be a bot with cornercase access"), "{error}");
        assert!(!program.with_extension("argv").exists());
    }
}

mod repositories {
    use super::*;

    const BIG: usize = 1 << 20;

    fn folders(paths: &[&str]) -> Ask {
        Ask { repositories: paths.iter().map(PathBuf::from).collect(), ..Ask::default() }
    }

    #[rstest::rstest]
    #[case::plain("/w/shop", &["/w/shop", "'/w/shop'", "\"/w/shop\""])]
    #[case::a_space("/w/my shop", &["'/w/my shop'", "\"/w/my shop\""])]
    #[case::a_dollar("/w/a$b", &[])]
    #[case::a_quote("/w/it's", &[])]
    #[case::a_comma("/w/a,b", &[])]
    #[case::a_parenthesis("/w/a(b)", &[])]
    #[case::a_star("/w/a*b", &[])]
    #[case::a_newline("/w/a\nb", &[])]
    #[case::nothing("", &[])]
    fn a_folder_is_written_as_the_agent_may_type_it(#[case] path: &str, #[case] wanted: &[&str]) {
        assert_eq!(spellings(Path::new(path), false), wanted);
        assert_eq!(spellings(Path::new(path), true), wanted);
    }

    #[rstest::rstest]
    #[case::on_windows(true, &["'C:\\w\\shop'"])]
    #[case::elsewhere(false, &[])]
    fn a_path_with_backslashes_has_one_spelling_on_windows_only(#[case] windows: bool, #[case] wanted: &[&str]) {
        assert_eq!(spellings(Path::new("C:\\w\\shop"), windows), wanted);
    }

    #[cfg(unix)]
    #[test]
    fn a_path_that_is_not_utf8_has_no_spelling() {
        use std::os::unix::ffi::OsStrExt;
        assert_eq!(spellings(Path::new(std::ffi::OsStr::from_bytes(b"/w/\xff")), false), Vec::<String>::new());
    }

    #[test]
    fn each_spelling_gets_the_four_reads_in_order() {
        let rules = git_reads(&folders(&["/a", "/b c"]), false, BIG);
        assert_eq!(rules.len(), 4 * 3 + 4 * 2);
        assert_eq!(&rules[..4], ["git -C /a log", "git -C /a show", "git -C /a diff", "git -C /a status"]);
        assert_eq!(rules[4], "git -C '/a' log");
        assert_eq!(rules[12], "git -C '/b c' log");
    }

    #[test]
    fn no_folder_gives_no_rule() {
        assert_eq!(git_reads(&Ask::default(), false, BIG), Vec::<String>::new());
        assert_eq!(git_reads(&folders(&["/w/a$b"]), false, BIG), Vec::<String>::new());
    }

    #[test]
    fn past_its_budget_the_rules_keep_the_single_quoted_spelling_and_then_fewer_folders() {
        let paths: Vec<String> = (0..40).map(|n| format!("/w/{}/{n}", "x".repeat(80))).collect();
        let ask = folders(&paths.iter().map(String::as_str).collect::<Vec<_>>());
        let full = git_reads(&ask, false, BIG);
        assert_eq!(full.len(), 40 * 12);
        let budget = size(&full) / 2;
        let some = git_reads(&ask, false, budget);
        assert_eq!(some.len(), 40 * 4);
        assert!(some.iter().all(|rule| rule.starts_with("git -C '")), "{some:?}");
        let few = git_reads(&ask, false, size(&some) / 2);
        assert!(!few.is_empty() && few.len() < some.len(), "{}", few.len());
        assert!(size(&few) <= size(&some) / 2);
        assert!(size(&git_reads(&ask, true, git_budget(true))) <= git_budget(true));
    }
}
