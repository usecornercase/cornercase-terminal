use std::fmt::Write as _;

use super::args::context::ContextArgs;
use super::{partial_unless, print_json, print_out};
use crate::agents::breakdown::{self, Breakdown, Slot};
use crate::client::{answer, ask};
use crate::control::{Command, ContextInfo, ContextRead};
use crate::error::Result;

const TOOLS_LISTED: usize = 5;

pub(super) fn run(args: &ContextArgs) -> Result<bool> {
    let read = ContextRead { pane: args.target.pane, tab: args.target.tab, cached: args.cached };
    let value = ask("context", Command::Context(read))?;
    let info: ContextInfo = answer(value.clone())?;
    if args.print.json {
        print_json(&value)?;
    } else {
        print_out(&text(&info))?;
    }
    partial_unless(info.error.is_none())
}

pub(crate) fn text(info: &ContextInfo) -> String {
    let mut text = heading(info);
    if let Some(breakdown) = &info.breakdown {
        body(&mut text, breakdown);
    }
    if let Some(error) = &info.error {
        let _ = writeln!(text, "context unavailable: {error}");
    }
    if let Some(notice) = &info.notice {
        let _ = writeln!(text, "{notice}");
    }
    text
}

fn heading(info: &ContextInfo) -> String {
    let title = info.title.as_deref().unwrap_or("agent");
    let mut line = info.model.as_ref().map_or_else(|| title.to_string(), |model| format!("{title} · {model}"));
    let status = match (info.loading, info.age) {
        (true, _) => "reading…".to_string(),
        (false, Some(age)) => breakdown::read_ago(age),
        (false, None) => String::new(),
    };
    if !status.is_empty() {
        let _ = write!(line, "  {status}");
    }
    line.push('\n');
    if info.breakdown.is_none() {
        match (info.percent, info.compacted) {
            (Some(percent), _) => {
                let _ = writeln!(line, "{percent}% of its window used");
            }
            (None, true) => line.push_str("compacted\n"),
            (None, false) => {}
        }
    }
    line
}

fn body(text: &mut String, b: &Breakdown) {
    let _ = writeln!(
        text,
        "{} of {} tokens · {}%",
        breakdown::tokens(b.total_tokens),
        breakdown::tokens(b.max_tokens),
        b.percent
    );
    let width = b.categories.iter().filter(|c| c.slot() != Slot::Deferred).map(|c| c.name.chars().count()).max();
    for category in b.categories.iter().filter(|c| c.slot() != Slot::Deferred && c.slot() != Slot::Other) {
        let _ = writeln!(
            text,
            "  {:<w$}  {:>7}  {:>6}",
            category.name,
            breakdown::tokens(category.tokens),
            breakdown::share(category.tokens, b.max_tokens),
            w = width.unwrap_or(0)
        );
    }
    let deferred: Vec<String> = b
        .categories
        .iter()
        .filter(|c| c.slot() == Slot::Deferred)
        .map(|c| format!("{} {}", c.name.replace(" (deferred)", ""), breakdown::tokens(c.tokens)))
        .collect();
    if !deferred.is_empty() {
        let _ = writeln!(text, "not loaded: {}", deferred.join(" · "));
    }
    sections(text, b);
    if b.conversation.is_none() {
        text.push_str("no conversation yet: this is what a new conversation starts with\n");
    }
}

fn sections(text: &mut String, b: &Breakdown) {
    list(text, "memory files", b.memory_files.iter().map(|f| (f.path.clone(), breakdown::tokens(f.tokens))));
    list(
        text,
        "mcp tools",
        b.mcp_servers
            .iter()
            .map(|s| (format!("{} ({} of {} loaded)", s.name, s.loaded, s.tools), breakdown::tokens(s.tokens))),
    );
    list(
        text,
        "mcp server instructions",
        b.mcp_instructions.iter().map(|i| (i.name.clone(), breakdown::tokens(i.tokens))),
    );
    if let Some(skills) = b.skills.as_ref().filter(|s| s.total > 0) {
        let count = if skills.included == skills.total {
            skills.total.to_string()
        } else {
            format!("{} of {}", skills.included, skills.total)
        };
        list(text, "skills", std::iter::once((count, breakdown::tokens(skills.tokens))));
    }
    if let Some(m) = &b.messages {
        let calls = breakdown::tokens(m.tool_calls + m.tool_results);
        list(text, "messages", std::iter::once(("tool calls and results".to_string(), calls)));
        let _ = writeln!(
            text,
            "  assistant {} · you {} · attachments {}",
            breakdown::tokens(m.assistant),
            breakdown::tokens(m.user),
            breakdown::tokens(m.attachments)
        );
        let top = m.top_tools(TOOLS_LISTED);
        if !top.is_empty() {
            let _ = writeln!(text, "  {}", top.join(" · "));
        }
    }
}

fn list(text: &mut String, title: &str, rows: impl Iterator<Item = (String, String)>) {
    let rows: Vec<(String, String)> = rows.collect();
    if rows.is_empty() {
        return;
    }
    let _ = writeln!(text, "{title}:");
    for (left, right) in rows {
        let _ = writeln!(text, "  {left}  {right}");
    }
}

#[cfg(test)]
mod tests;
