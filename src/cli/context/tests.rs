use super::*;
use crate::agents::breakdown::{Category, McpServer, MemoryFile};

fn category(id: &str, name: &str, tokens: u64, kind: &str) -> Category {
    Category { id: id.into(), name: name.into(), tokens, kind: kind.into() }
}

fn info() -> ContextInfo {
    let breakdown = Breakdown {
        conversation: Some("s1".into()),
        total_tokens: 251_853,
        max_tokens: 1_000_000,
        percent: 25,
        categories: vec![
            category("system_prompt", "System prompt", 2_681, "used"),
            category("mcp_tools_deferred", "MCP tools (deferred)", 199_872, "deferred"),
            category("free_space", "Free space", 715_147, "free"),
        ],
        memory_files: vec![MemoryFile { path: "~/code/AGENTS.md".into(), kind: "Project".into(), tokens: 8_476 }],
        mcp_servers: vec![McpServer { name: "github".into(), tools: 3, loaded: 0, tokens: 0 }],
        ..Breakdown::default()
    };
    ContextInfo {
        pane: 4,
        title: Some("Claude Code".into()),
        model: Some("Opus 5.5".into()),
        age: Some(5),
        breakdown: Some(breakdown),
        ..ContextInfo::default()
    }
}

#[test]
fn a_breakdown_is_printed_with_its_sections() {
    assert_eq!(
        text(&info()),
        "Claude Code · Opus 5.5  read just now\n\
         251.9k of 1M tokens · 25%\n\
         \u{20}\u{20}System prompt     2.7k    0.3%\n\
         \u{20}\u{20}Free space      715.1k   71.5%\n\
         not loaded: MCP tools 199.9k\n\
         memory files:\n\
         \u{20}\u{20}~/code/AGENTS.md  8.5k\n\
         mcp tools:\n\
         \u{20}\u{20}github (0 of 3 loaded)  0\n"
    );
}

#[test]
fn a_failed_read_says_why_beside_the_old_numbers() {
    let mut failed = info();
    failed.error = Some("claude exited without answering".into());
    assert!(text(&failed).ends_with("context unavailable: claude exited without answering\n"));
}

#[test]
fn another_agent_gets_its_percentage_and_the_notice() {
    let other = ContextInfo {
        title: Some("Codex".into()),
        model: Some("gpt-5".into()),
        percent: Some(42),
        notice: Some("Codex does not say what fills its context".into()),
        ..ContextInfo::default()
    };
    assert_eq!(text(&other), "Codex · gpt-5\n42% of its window used\nCodex does not say what fills its context\n");
}

#[test]
fn a_new_conversation_is_said() {
    let mut fresh = info();
    if let Some(b) = fresh.breakdown.as_mut() {
        b.conversation = None;
    }
    assert!(text(&fresh).ends_with("no conversation yet: this is what a new conversation starts with\n"));
}
