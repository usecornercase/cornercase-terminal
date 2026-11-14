use std::process::Command;

use serde_json::Value;

use crate::agents::headless::{Access, Action, Ask, Ran, Tools, Used};
use crate::error::{Error, Result};
use crate::{agents, process};

const ARGS: [&str; 6] = ["exec", "--json", "--skip-git-repo-check", "--ephemeral", "--sandbox", "read-only"];

const BOT_ARGS: [&str; 3] = ["exec", "--json", "--skip-git-repo-check"];

fn bot_args(access: Access) -> Vec<String> {
    let mut args: Vec<String> = BOT_ARGS.iter().map(ToString::to_string).collect();
    match access {
        Access::Cornercase => args.extend(["--sandbox", "workspace-write"].map(String::from)),
        Access::Full => args.push(String::from("--dangerously-bypass-approvals-and-sandbox")),
    }
    args
}

fn recap_args() -> Vec<String> {
    ARGS.iter().map(ToString::to_string).collect()
}

pub(in crate::agents) fn command(program: &str, ask: &Ask) -> Command {
    let mut args = match ask.tools {
        Tools::Recap => recap_args(),
        Tools::Bot(access) => bot_args(access),
    };
    args.extend(["-C".to_string(), ask.folder.display().to_string()]);
    let args = agents::with_model("codex", args, ask.model.as_deref());
    let mut args = agents::with_effort("codex", args, ask.effort.as_deref());
    if let Some(session) = &ask.conversation {
        args.extend(["resume".to_string(), session.clone()]);
    }
    args.extend(["--".to_string(), ask.prompt.clone()]);
    let mut command = process::program::command(program);
    command.args(args);
    command
}

fn unwrap_shell(command: &str) -> String {
    let words: Vec<&str> = command.splitn(3, ' ').collect();
    let shell = words.first().and_then(|program| program.rsplit(['/', '\\']).next()).unwrap_or_default();
    let wrapper =
        matches!(shell, "sh" | "bash" | "zsh" | "dash" | "fish") && matches!(words.get(1), Some(&("-lc" | "-c")));
    let inner = words.get(2).copied().filter(|_| wrapper).unwrap_or(command);
    let inner = inner.trim();
    for quote in ['\'', '"'] {
        if let Some(unquoted) = inner.strip_prefix(quote).and_then(|rest| rest.strip_suffix(quote)) {
            return unquoted.to_string();
        }
    }
    inner.to_string()
}

fn action(command: &str) -> Action {
    let text = crate::text::truncate_right(&crate::text::one_line(&unwrap_shell(command)), 200);
    Action { tool: String::from("shell"), text }
}

fn tokens(usage: &Value) -> Used {
    let count = |key: &str| usage[key].as_u64().unwrap_or_default();
    let cached = count("cached_input_tokens");
    Used {
        input: count("input_tokens").saturating_sub(cached),
        output: count("output_tokens"),
        cache_read: cached,
        cache_write: count("cache_write_input_tokens"),
    }
}

fn completed(entry: &Value, ran: &mut Ran) -> Result<()> {
    if ran.text.trim().is_empty() {
        return Err(Error::Probe(String::from("Codex answered nothing")));
    }
    ran.tokens = tokens(&entry["usage"]);
    Ok(())
}

fn failed(entry: &Value) -> Error {
    let message = entry["error"]["message"].as_str().or_else(|| entry["message"].as_str());
    Error::Probe(message.unwrap_or("Codex failed").to_string())
}

pub(in crate::agents) fn read(line: &str, ran: &mut Ran) -> Option<Result<()>> {
    let entry: Value = serde_json::from_str(line).ok()?;
    match entry["type"].as_str()? {
        "thread.started" => {
            ran.session = entry["thread_id"].as_str().map(str::to_string);
            None
        }
        "item.started" if entry["item"]["type"] == "command_execution" => {
            ran.steps += 1;
            ran.fresh.push(action(entry["item"]["command"].as_str().unwrap_or_default()));
            None
        }
        "item.completed" if entry["item"]["type"] == "agent_message" => {
            ran.text = entry["item"]["text"].as_str().unwrap_or_default().trim().to_string();
            None
        }
        "turn.completed" => Some(completed(&entry, ran)),
        "turn.failed" => Some(Err(failed(&entry))),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
