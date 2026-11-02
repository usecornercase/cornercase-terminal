use rstest::rstest;
use serde_json::{Value, json};

use super::*;
use crate::agents::breakdown::Slot;
use crate::test_util::TempDir;

const CONTEXT: &str = crate::test_util::fixture!("claude/2.1.316/context-usage.jsonl");
const FRESH: &str = crate::test_util::fixture!("claude/2.1.316/context-usage-fresh.jsonl");
const PENDING: &str = crate::test_util::fixture!("claude/2.1.316/mcp-status.jsonl");
const UNKNOWN: &str = crate::test_util::fixture!("claude/2.1.316/resume-unknown.jsonl");

fn answered(line: &str) -> Breakdown {
    match next(line, Duration::ZERO) {
        Step::Answer(result) => result.expect("a breakdown"),
        _ => panic!("no answer to {line}"),
    }
}

fn target(conversation: Option<&str>, model: Option<&str>) -> Target {
    Target {
        pid: 0,
        conversation: conversation.map(str::to_string),
        folder: std::env::temp_dir(),
        model: model.map(str::to_string),
        account: None,
    }
}

fn args_of(command: &Command) -> Vec<String> {
    command.get_args().map(|a| a.to_string_lossy().into_owned()).collect()
}

#[test]
fn the_recorded_answer_is_read() {
    let breakdown = answered(CONTEXT);
    assert_eq!((breakdown.total_tokens, breakdown.max_tokens, breakdown.percent), (251_853, 1_000_000, 25));
    assert_eq!(breakdown.model.as_deref(), Some("claude-opus-5-5"));
    assert_eq!(breakdown.auto_compact_at, Some(967_000));
    assert_eq!(breakdown.memory_files.len(), 4);
    assert_eq!(breakdown.memory_files[1].kind, "Project");
    assert_eq!(breakdown.skills.as_ref().map(|s| (s.total, s.included, s.tokens)), Some((34, 34, 6_002)));
}

#[test]
fn the_categories_are_placed_by_kind() {
    let breakdown = answered(CONTEXT);
    assert_eq!(breakdown.categories.len(), 11);
    assert_eq!(breakdown.categories[4].slot(), Slot::Deferred);
    assert_eq!(breakdown.categories[9].slot(), Slot::Buffer);
    assert_eq!(breakdown.categories[10].slot(), Slot::Free);
}

#[test]
fn mcp_tools_are_grouped_by_server_and_instructions_stay_apart() {
    let breakdown = answered(CONTEXT);
    let docs = breakdown.mcp_servers.iter().find(|s| s.name == "claude_ai_Docs").expect("the docs server");
    assert_eq!((docs.tools, docs.loaded), (8, 3));
    let loaded: u64 = breakdown.mcp_servers.iter().map(|s| s.loaded).sum();
    let total: u64 = breakdown.mcp_servers.iter().map(|s| s.tools).sum();
    assert_eq!((loaded, total), (6, 23));
    assert_eq!(breakdown.mcp_instructions.len(), 7);
    assert!(breakdown.mcp_instructions.iter().any(|i| i.name == "claude.ai Docs"));
}

#[test]
fn the_messages_are_split() {
    let messages = answered(CONTEXT).messages.expect("messages");
    assert_eq!((messages.tool_calls, messages.tool_results, messages.attachments), (6_958, 90_974, 137_118));
    assert_eq!((messages.assistant, messages.user), (20_663, 418));
    assert_eq!(messages.tools[0], ToolUse { name: "Bash".into(), calls: 4_841, results: 81_916 });
    assert_eq!(messages.attachment_kinds[0].name, "hook_success");
}

#[test]
fn a_new_conversation_has_no_api_usage() {
    let breakdown = answered(FRESH);
    assert_eq!(breakdown.total_tokens, 45_518);
}

#[test]
fn an_unknown_kind_is_passed_through() {
    let body = json!({"categories": [{"id": "x", "name": "X", "tokens": 1, "kind": "weird"}], "totalTokens": 1, "maxTokens": 2});
    let breakdown = parse(body).expect("a breakdown");
    assert_eq!(breakdown.categories[0].kind, "weird");
    assert_eq!(breakdown.categories[0].slot(), Slot::Other);
    assert!(breakdown.memory_files.is_empty() && breakdown.skills.is_none() && breakdown.messages.is_none());
}

#[rstest]
#[case::categories(json!({"totalTokens": 1, "maxTokens": 2}))]
#[case::total(json!({"categories": [], "maxTokens": 2}))]
#[case::wrong_type(json!({"categories": 5, "totalTokens": 1, "maxTokens": 2}))]
fn an_answer_without_what_the_dialog_needs_fails(#[case] body: Value) {
    let error = parse(body).expect_err("unexpected");
    assert!(error.to_string().starts_with("unexpected answer: "), "{error}");
}

#[test]
fn the_percentage_is_clamped() {
    let body = json!({"categories": [], "totalTokens": 1, "maxTokens": 2, "percentage": 140.4});
    assert_eq!(parse(body).expect("a breakdown").percent, 100);
}

#[test]
fn a_refusal_is_the_text_of_the_error() {
    let line = r#"{"type":"control_response","response":{"subtype":"error","request_id":"context","error":"nope"}}"#;
    match next(line, Duration::ZERO) {
        Step::Answer(Err(error)) => assert_eq!(error.to_string(), "nope"),
        _ => panic!("no refusal"),
    }
}

#[test]
fn an_unknown_conversation_is_claudes_own_error() {
    match next(UNKNOWN, Duration::ZERO) {
        Step::Answer(Err(error)) => {
            assert!(error.to_string().starts_with("No conversation found with session ID"), "{error}");
        }
        _ => panic!("no error"),
    }
}

#[rstest]
#[case::pending_before_the_cap(PENDING, Duration::from_secs(1), MCP_REQUEST)]
#[case::pending_after_the_cap(PENDING, Duration::from_secs(9), CONTEXT_REQUEST)]
#[case::settled(
    r#"{"type":"control_response","response":{"subtype":"success","request_id":"mcp","response":{"mcpServers":[]}}}"#,
    Duration::ZERO,
    CONTEXT_REQUEST
)]
#[case::without_mcp_status(
    r#"{"type":"control_response","response":{"subtype":"error","request_id":"mcp","error":"unknown"}}"#,
    Duration::ZERO,
    CONTEXT_REQUEST
)]
fn the_context_is_asked_once_the_servers_settle(#[case] line: &str, #[case] elapsed: Duration, #[case] asks: &str) {
    match next(line, elapsed) {
        Step::Write(text) => assert_eq!(text, asks),
        _ => panic!("nothing written"),
    }
}

#[test]
fn other_lines_are_waited_through() {
    assert!(matches!(next(r#"{"type":"system"}"#, Duration::ZERO), Step::Read));
    assert!(matches!(next("not json", Duration::ZERO), Step::Read));
}

#[test]
fn a_conversation_is_resumed_with_hooks_off() {
    let args = args_of(&build("claude", &target(Some("s1"), Some("claude-opus-5-5")), &[], Vec::new()));
    let joined = args.join(" ");
    assert!(joined.contains("--resume s1"), "{joined}");
    assert!(joined.contains("--model claude-opus-5-5"), "{joined}");
    assert!(args.contains(&SETTINGS.to_string()), "{joined}");
    assert!(!args.iter().any(|a| a == "--setting-sources"), "{joined}");
}

#[test]
fn a_new_conversation_is_not_resumed() {
    let args = args_of(&build("claude", &target(None, None), &[], Vec::new()));
    assert!(!args.iter().any(|a| a == "--resume" || a == "--model"), "{args:?}");
}

#[test]
fn only_what_exists_is_repeated_from_the_pane() {
    let dir = TempDir::new();
    let folder = dir.path().display().to_string();
    let args: Vec<String> = [
        "--add-dir",
        &folder,
        "/nonexistent/dir",
        "--mcp-config",
        "{\"mcpServers\":{}}",
        "/nonexistent.json",
        "--strict-mcp-config",
        "--system-prompt",
        "x",
    ]
    .map(String::from)
    .to_vec();
    let built = args_of(&build("claude", &target(None, None), &args, Vec::new()));
    let tail = built[ARGS.len() + 1..].to_vec();
    assert_eq!(tail, ["--add-dir", &folder, "--mcp-config", "{\"mcpServers\":{}}", "--strict-mcp-config"]);
}

#[test]
fn the_panes_environment_is_kept_without_the_session_variables() {
    let own = vec![
        ("PATH".to_string(), "/bin".to_string()),
        ("CLAUDECODE".to_string(), "1".to_string()),
        ("CLAUDE_CONFIG_DIR".to_string(), "/acc".to_string()),
    ];
    let built = build("claude", &target(None, None), &[], own);
    let envs: Vec<_> = built.get_envs().collect();
    assert!(envs.contains(&("PATH".as_ref(), Some("/bin".as_ref()))), "{envs:?}");
    assert!(envs.contains(&("CLAUDE_CONFIG_DIR".as_ref(), Some("/acc".as_ref()))), "{envs:?}");
    assert!(envs.iter().all(|(key, _)| *key != "CLAUDECODE"), "{envs:?}");
}

#[test]
fn without_the_panes_environment_the_account_names_the_folder() {
    let mut with = target(None, None);
    with.account = Some("/accounts/work".into());
    let built = build("claude", &with, &[], Vec::new());
    let envs: Vec<_> = built.get_envs().collect();
    assert!(envs.contains(&("CLAUDE_CONFIG_DIR".as_ref(), Some("/accounts/work".as_ref()))), "{envs:?}");
    assert!(envs.contains(&("CLAUDECODE".as_ref(), None)), "{envs:?}");
    let plain = build("claude", &target(None, None), &[], Vec::new());
    assert!(plain.get_envs().all(|(key, _)| key != "CLAUDE_CONFIG_DIR"));
}

#[test]
fn a_folder_that_is_gone_is_said() {
    let mut gone = target(None, None);
    gone.folder = "/nonexistent/folder".into();
    let error = probe(&gone, "claude", Duration::from_secs(1)).expect_err("a gone folder");
    assert!(error.to_string().starts_with("the folder Claude Code runs in is gone"), "{error}");
}
