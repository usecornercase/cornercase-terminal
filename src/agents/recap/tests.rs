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
    assert!(error.to_string().contains("cannot write a recap"), "{error}");
}
