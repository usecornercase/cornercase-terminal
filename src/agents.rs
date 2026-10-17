pub(crate) mod activity;
pub(crate) mod awake;
pub mod claude;
pub(crate) mod codex;
pub(crate) mod context;
pub mod copilot;
pub(crate) mod cursor;
pub(crate) mod gemini;
pub(crate) mod grok;
mod known;
pub(crate) mod launch;
pub(crate) mod opencode;
pub mod stats;
pub(crate) mod tree;
pub(crate) mod usage;

pub use known::Known;

use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AgentState {
    pub(crate) kind: String,
    pub(crate) conversation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) mode: Option<String>,
}

pub(crate) const AUTO: &str = "auto";
pub(crate) const DEFAULT_TRUST_PROMPT: &str =
    "trust the files|trust this (folder|directory|workspace|repository)|do you trust|yes, proceed";
const TRUST_LINES: usize = 15;
const OTHERS: [(&str, &str); 8] = [
    ("amp", "amp"),
    ("droid", "droid"),
    ("pi", "pi"),
    ("qwen", "qwen"),
    ("kimi", "kimi"),
    ("cline", "cline"),
    ("goose", "goose"),
    ("aider", "aider"),
];
pub(in crate::agents) type ModeTable = &'static [(&'static str, &'static [&'static str])];

pub(crate) type Mode = (String, Vec<String>);

pub(crate) fn kinds(config: &Config) -> Vec<String> {
    let mut kinds: Vec<String> = Known::ALL
        .into_iter()
        .map(|kind| kind.name().to_string())
        .chain(OTHERS.iter().map(|(kind, _)| (*kind).to_string()))
        .collect();
    for kind in config.agent_commands.keys() {
        if !kinds.contains(kind) {
            kinds.push(kind.clone());
        }
    }
    kinds
}

pub(crate) fn command(config: &Config, kind: &str) -> String {
    config
        .agent_commands
        .get(kind)
        .cloned()
        .or_else(|| Known::of(kind).map(|kind| kind.command().to_string()))
        .or_else(|| OTHERS.iter().find(|(k, _)| *k == kind).map(|(_, bin)| (*bin).to_string()))
        .unwrap_or_else(|| kind.to_string())
}

fn executable(path: &Path) -> bool {
    path.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

fn found(program: &str, path: Option<&OsStr>) -> bool {
    if program.contains('/') {
        return executable(Path::new(program));
    }
    path.is_some_and(|path| std::env::split_paths(path).any(|dir| dir.is_absolute() && executable(&dir.join(program))))
}

pub(crate) fn installed(program: &str) -> bool {
    found(program, std::env::var_os("PATH").as_deref())
}

pub(crate) fn modes(config: &Config, kind: &str) -> Vec<Mode> {
    let mut modes: Vec<Mode> = Known::of(kind)
        .into_iter()
        .flat_map(|kind| kind.modes().iter())
        .map(|(name, args)| ((*name).to_string(), args.iter().map(|a| (*a).to_string()).collect()))
        .collect();
    for (name, args) in config.agent_modes.get(kind).into_iter().flatten() {
        match modes.iter_mut().find(|(n, _)| n == name) {
            Some(mode) => mode.1.clone_from(args),
            None => modes.push((name.clone(), args.clone())),
        }
    }
    modes.retain(|(_, args)| !args.is_empty());
    modes
}

pub(crate) fn args(config: &Config, kind: &str) -> Vec<String> {
    config.agent_args.get(kind).cloned().unwrap_or_default()
}

fn position_of(args: &[String], run: &[String]) -> Option<usize> {
    (!run.is_empty()).then(|| args.windows(run.len()).position(|w| w == run)).flatten()
}

fn longest_first(modes: &[Mode]) -> Vec<&Mode> {
    let mut sorted: Vec<&Mode> = modes.iter().collect();
    sorted.sort_by_key(|(_, args)| std::cmp::Reverse(args.len()));
    sorted
}

pub(crate) fn mode_of(kind: &str, args: &[String], modes: &[Mode]) -> Option<String> {
    let args = normalized(kind, args);
    longest_first(modes)
        .into_iter()
        .find(|(_, run)| position_of(&args, &normalized(kind, run)).is_some())
        .map(|(name, _)| name.clone())
}

fn normalized(kind: &str, args: &[String]) -> Vec<String> {
    Known::of(kind).map_or_else(|| args.to_vec(), |kind| kind.normalized(args))
}

pub(crate) fn extra_args(kind: &str, args: &[String], modes: &[Mode]) -> Vec<String> {
    let mut out = normalized(kind, args);
    for (_, run) in longest_first(modes) {
        let run = normalized(kind, run);
        while let Some(at) = position_of(&out, &run) {
            out.drain(at..at + run.len());
        }
    }
    out
}

fn mode_args(modes: &[Mode], name: Option<&str>) -> Vec<String> {
    name.and_then(|name| modes.iter().find(|(n, _)| n == name)).map(|(_, args)| args.clone()).unwrap_or_default()
}

pub(crate) fn with_mode(kind: &str, args: &[String], modes: &[Mode], name: Option<&str>) -> Vec<String> {
    [mode_args(modes, name), extra_args(kind, args, modes)].concat()
}

pub(crate) fn without_model(kind: &str, args: &[String]) -> Vec<String> {
    let Some(known) = Known::of(kind) else { return args.to_vec() };
    let flags: Vec<&str> = std::iter::once(known.model_flag()).chain(known.model_alias()).collect();
    let mut out = Vec::with_capacity(args.len());
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if flags.contains(&arg.as_str()) {
            args.next();
        } else if !flags.iter().any(|flag| arg.strip_prefix(flag).is_some_and(|rest| rest.starts_with('='))) {
            out.push(arg.clone());
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Effort {
    pub(crate) levels: &'static [&'static str],
    pub(crate) flags: &'static [&'static str],
    pub(crate) config: Option<&'static str>,
}

impl Effort {
    fn args(self, level: &str) -> Vec<String> {
        match (self.flags.first(), self.config) {
            (Some(flag), _) => vec![(*flag).to_string(), level.to_string()],
            (None, Some(key)) => vec!["-c".to_string(), format!("{key}={level}")],
            (None, None) => Vec::new(),
        }
    }

    fn keyed(self, text: &str) -> bool {
        self.config.is_some_and(|key| text.strip_prefix(key).is_some_and(|rest| rest.starts_with('=')))
    }
}

pub(crate) fn effort_levels(kind: &str) -> Option<&'static [&'static str]> {
    Known::of(kind).and_then(Known::effort).map(|effort| effort.levels)
}

pub(crate) fn without_effort(kind: &str, args: &[String]) -> Vec<String> {
    let Some(effort) = Known::of(kind).and_then(Known::effort) else { return args.to_vec() };
    let mut out = Vec::with_capacity(args.len());
    let mut args = args.iter().peekable();
    while let Some(arg) = args.next() {
        let flag = effort.flags.contains(&arg.as_str());
        let config = matches!(arg.as_str(), "-c" | "--config") && args.peek().is_some_and(|next| effort.keyed(next));
        if flag || config {
            args.next();
        } else if !effort.flags.iter().any(|flag| arg.strip_prefix(flag).is_some_and(|rest| rest.starts_with('=')))
            && !["--config=", "-c=", "-c"]
                .iter()
                .any(|prefix| arg.strip_prefix(prefix).is_some_and(|r| effort.keyed(r)))
        {
            out.push(arg.clone());
        }
    }
    out
}

pub(crate) fn names_effort(kind: &str, args: &[String]) -> bool {
    without_effort(kind, args).len() != args.len()
}

pub(crate) fn with_effort(kind: &str, args: Vec<String>, level: Option<&str>) -> Vec<String> {
    let (Some(effort), Some(level)) =
        (Known::of(kind).and_then(Known::effort), level.map(str::trim).filter(|l| !l.is_empty()))
    else {
        return args;
    };
    let mut out = without_effort(kind, &args);
    out.extend(effort.args(level));
    out
}

pub(crate) fn names_model(kind: &str, args: &[String]) -> bool {
    without_model(kind, args).len() != args.len()
}

pub(crate) fn with_model(kind: &str, args: Vec<String>, model: Option<&str>) -> Vec<String> {
    let (Some(known), Some(model)) = (Known::of(kind), model.map(str::trim).filter(|m| !m.is_empty())) else {
        return args;
    };
    let mut out = without_model(kind, &args);
    out.extend([known.model_flag().to_string(), model.to_string()]);
    out
}

pub(crate) fn with_extra(kind: &str, args: &[String], modes: &[Mode], extra: Vec<String>) -> Vec<String> {
    [mode_args(modes, mode_of(kind, args, modes).as_deref()), extra].concat()
}

pub(crate) fn is_dangerous(mode: &str) -> bool {
    mode.to_lowercase().contains("danger")
}

pub(crate) fn split_args(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    let mut quote: Option<char> = None;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"') | None, '\\') => current.get_or_insert_default().extend(chars.next()),
            (None, c) if c.is_whitespace() => out.extend(current.take()),
            (None, '"' | '\'') => {
                quote = Some(c);
                current.get_or_insert_default();
            }
            (_, c) => current.get_or_insert_default().push(c),
        }
    }
    out.extend(current);
    out
}

pub(crate) fn quote(arg: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "-_./:@%+=,".contains(c);
    if !arg.is_empty() && arg.chars().all(safe) {
        return arg.to_string();
    }
    format!("'{}'", arg.replace('\'', r"'\''"))
}

pub(crate) fn join_args(args: &[String]) -> String {
    args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ")
}

pub(crate) fn command_line(config: &Config, kind: &str) -> String {
    line(config, kind, args(config, kind))
}

fn line(config: &Config, kind: &str, args: Vec<String>) -> String {
    std::iter::once(command(config, kind)).chain(args).map(|a| quote(&a)).collect::<Vec<_>>().join(" ")
}

pub(crate) fn start_line(
    config: &Config,
    kind: &str,
    mode: Option<&str>,
    model: Option<&str>,
    effort: Option<&str>,
) -> Result<String, String> {
    let mut chosen = args(config, kind);
    if let Some(name) = mode {
        let table = modes(config, kind);
        if table.iter().all(|(known, _)| known != name) {
            let names: Vec<&str> = table.iter().map(|(known, _)| known.as_str()).collect();
            return Err(if names.is_empty() {
                format!("{kind} has no modes in cornercase; add some with agent_modes in config.json")
            } else {
                format!("{kind} has no mode `{name}`; its modes are `{}`", names.join("`, `"))
            });
        }
        chosen = with_mode(kind, &chosen, &table, Some(name));
    }
    if model.is_some_and(|m| !m.trim().is_empty()) && Known::of(kind).is_none() {
        let known = Known::ALL.map(Known::name).join(", ");
        return Err(format!("cornercase passes a model to {known} only; give {kind} its model in agent_args"));
    }
    let effort = effort.map(str::trim).filter(|level| !level.is_empty());
    if let Some(refusal) = effort.and_then(|level| effort_refusal(kind, level)) {
        return Err(refusal);
    }
    Ok(line(config, kind, with_effort(kind, with_model(kind, chosen, model), effort)))
}

pub(crate) fn effort_refusal(kind: &str, level: &str) -> Option<String> {
    match effort_levels(kind) {
        None => Some(format!("{kind} has no effort switch")),
        Some(levels) if !levels.contains(&level) => {
            Some(format!("{kind} has no effort `{level}`; its levels are `{}`", levels.join("`, `")))
        }
        Some(_) => None,
    }
}

pub(crate) fn resume_line(config: &Config, agent: &AgentState) -> Option<String> {
    let kind = agent.kind.as_str();
    let args = with_mode(kind, &args(config, kind), &modes(config, kind), agent.mode.as_deref());
    let conversation = agent.conversation.clone();
    let line = Known::of(kind)?.resume(args, conversation)?;
    Some(std::iter::once(command(config, kind)).chain(line).map(|a| quote(&a)).collect::<Vec<_>>().join(" "))
}

pub(crate) fn resolve(config: &Config, explicit: Option<&str>, running: Option<&str>) -> Option<String> {
    explicit
        .or_else(|| (config.agent != AUTO && !config.agent.trim().is_empty()).then_some(config.agent.as_str()))
        .or(running)
        .map(str::to_string)
}

fn basename(arg: &str) -> &str {
    Path::new(arg).file_name().and_then(|n| n.to_str()).unwrap_or(arg)
}

pub(crate) fn detect(config: &Config, argv: &[String]) -> Option<String> {
    let names: Vec<&str> = argv.iter().take(2).map(|a| basename(a)).collect();
    let script = argv.iter().skip(1).find(|arg| !arg.starts_with('-')).map(String::as_str);
    kinds(config).into_iter().find(|kind| {
        let bin = command(config, kind);
        names.contains(&basename(&bin))
            || Known::of(kind).is_some_and(|kind| script.is_some_and(|arg| kind.runs_as(arg)))
    })
}

#[derive(Debug, Default)]
pub(crate) struct TrustPrompt {
    compiled: Option<(String, Option<regex::Regex>)>,
}

impl TrustPrompt {
    pub(crate) fn regex(&mut self, pattern: &str) -> Option<&regex::Regex> {
        if self.compiled.as_ref().is_none_or(|(compiled, _)| compiled != pattern) {
            let regex = regex::RegexBuilder::new(pattern).case_insensitive(true).build().ok();
            self.compiled = Some((pattern.to_string(), regex));
        }
        self.compiled.as_ref().and_then(|(_, regex)| regex.as_ref())
    }
}

pub(crate) fn asks_trust(regex: &regex::Regex, screen: &str) -> bool {
    let lines: Vec<&str> = screen.lines().filter(|line| !line.trim().is_empty()).collect();
    regex.is_match(&lines[lines.len().saturating_sub(TRUST_LINES)..].join("\n"))
}

#[cfg(test)]
mod tests;
