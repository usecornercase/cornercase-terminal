use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::agents::headless::{
    Access, Action, Ask, READ_ONLY, Ran, Tools, Used, answered, detail, heard, owned, shell, started, step,
};
use crate::error::{Error, Result};
use crate::persist::{self, Kind};
use crate::{agents, os};

const BASE: [&str; 3] = ["--skip-trust", "--output-format", "stream-json"];
const TOOL_DETAILS: [&str; 6] = ["file_path", "dir_path", "pattern", "query", "url", "prompt"];
const ESCAPED: &str = r#"'"file_path":"[^"]*[%\\]'"#;
const WRITES: &str = r#"["write_file", "replace"]"#;
const POLICY_DIR: &str = ".gemini";
const POLICY_FILE: &str = "cornercase-policy.toml";
const DOTENV: &str = ".env";

fn policy_file(folder: &Path) -> PathBuf {
    folder.join(POLICY_DIR).join(POLICY_FILE)
}

fn mode_args(ask: &Ask) -> Vec<String> {
    match ask.tools {
        Tools::Bot(Access::Cornercase) => {
            let policy = policy_file(&ask.folder).display().to_string();
            owned(&["--approval-mode", "default", "--policy", &policy])
        }
        Tools::Bot(Access::Full) => owned(&["--approval-mode", "yolo"]),
        Tools::Recap => owned(&["--approval-mode", "default"]),
    }
}

pub(in crate::agents) fn command(program: &str, ask: &Ask) -> Command {
    let mut args = owned(&BASE);
    args.extend(mode_args(ask));
    let mut args = agents::with_model("gemini", args, ask.model.as_deref());
    if let Some(session) = &ask.conversation {
        args.extend(["--resume".to_string(), session.clone()]);
    }
    args.push(format!("--prompt={}", ask.prompt));
    started(program, args)
}

fn quoted(words: &[String]) -> String {
    let words: Vec<String> = words.iter().map(|word| format!("\"{word}\"")).collect();
    format!("[{}]", words.join(", "))
}

fn escaped(name: &str) -> String {
    name.chars().fold(String::new(), |mut text, c| {
        if c.is_ascii_alphabetic() {
            text.extend(['[', c.to_ascii_lowercase(), c.to_ascii_uppercase(), ']']);
        } else {
            if ".^$*+?()[]{}|\\".contains(c) {
                text.push('\\');
            }
            text.push(c);
        }
        text
    })
}

fn allowed_prefixes() -> Vec<String> {
    std::iter::once("cornercase").chain(READ_ONLY).map(String::from).collect()
}

fn stopped_prefixes(ask: &Ask) -> Vec<String> {
    ask.stops.iter().map(|name| format!("cornercase {name}")).collect()
}

fn protected_pattern(ask: &Ask) -> String {
    let protected = ask.protected.iter().map(|name| escaped(name));
    let names: Vec<String> = [POLICY_DIR, DOTENV].iter().map(|name| escaped(name)).chain(protected).collect();
    format!("'\"file_path\":\"[^\"]*({})'", names.join("|"))
}

pub(in crate::agents) fn policy(ask: &Ask) -> String {
    let rules = [
        format!(
            "[[rule]]\ntoolName = \"run_shell_command\"\ncommandPrefix = {}\ndecision = \"allow\"\npriority = 900\n",
            quoted(&allowed_prefixes())
        ),
        format!(
            "[[rule]]\ntoolName = \"run_shell_command\"\ncommandPrefix = {}\ndecision = \"deny\"\npriority = 950\n",
            quoted(&stopped_prefixes(ask))
        ),
        String::from(
            "[[rule]]\ntoolName = [\"write_file\", \"replace\", \"web_fetch\"]\ndecision = \"allow\"\npriority = 900\n",
        ),
        format!(
            "[[rule]]\ntoolName = {WRITES}\nargsPattern = {}\ndecision = \"deny\"\npriority = 950\n",
            protected_pattern(ask)
        ),
        format!("[[rule]]\ntoolName = {WRITES}\nargsPattern = {ESCAPED}\ndecision = \"deny\"\npriority = 950\n"),
    ];
    rules.join("\n")
}

pub(in crate::agents) fn prepare(ask: &Ask) -> Result<()> {
    if ask.tools != Tools::Bot(Access::Cornercase) {
        return Ok(());
    }
    let written = os::fs::create_private_dir_all(&ask.folder.join(POLICY_DIR))
        .and_then(|()| persist::write_bytes(&policy_file(&ask.folder), policy(ask).as_bytes(), Kind::Private));
    written.map(|_| ()).map_err(|e| Error::Probe(format!("could not write {}'s rules: {e}", super::TITLE)))
}

fn tool_action(entry: &Value) -> Action {
    let name = entry["tool_name"].as_str().unwrap_or("tool");
    let input = &entry["parameters"];
    match name {
        "run_shell_command" => shell(name, input["command"].as_str().unwrap_or_default()),
        _ => step(name, detail(input, &TOOL_DETAILS)),
    }
}

fn main_model(stats: &Value) -> Option<String> {
    let models = stats["models"].as_object()?;
    let total = |entry: &Value| entry["total_tokens"].as_u64().unwrap_or_default();
    models.iter().max_by_key(|(_, entry)| total(entry)).map(|(name, _)| name.clone())
}

fn finished(entry: &Value, ran: &mut Ran) -> Result<()> {
    if entry["status"] != "success" {
        let message = entry["error"]["message"].as_str().filter(|message| !message.is_empty());
        return Err(Error::Probe(message.unwrap_or("Gemini CLI failed").to_string()));
    }
    let stats = &entry["stats"];
    let count = |key: &str| stats[key].as_u64().unwrap_or_default();
    ran.tokens =
        Used { input: count("input"), output: count("output_tokens"), cache_read: count("cached"), cache_write: 0 };
    if let Some(model) = main_model(stats) {
        ran.model = Some(model);
    }
    answered(ran, super::TITLE)
}

fn begun(entry: &Value, ran: &mut Ran) {
    ran.session = entry["session_id"].as_str().map(str::to_string);
    ran.model = entry["model"].as_str().filter(|model| *model != "auto").map(str::to_string);
}

pub(in crate::agents) fn read(line: &str, ran: &mut Ran) -> Option<Result<()>> {
    let entry: Value = serde_json::from_str(line).ok()?;
    match entry["type"].as_str()? {
        "init" => begun(&entry, ran),
        "message" if entry["role"] == "assistant" => ran.text.push_str(entry["content"].as_str().unwrap_or_default()),
        "tool_use" => heard(ran, tool_action(&entry)),
        "result" => return Some(finished(&entry, ran)),
        _ => {}
    }
    None
}

#[cfg(test)]
mod tests;
