use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentState {
    pub kind: String,
    pub conversation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

pub const AUTO: &str = "auto";
pub const CLAUDE: &str = "claude";
pub const CODEX: &str = "codex";
pub const GEMINI: &str = "gemini";
pub const OPENCODE: &str = "opencode";
pub const DEFAULT_TRUST_PROMPT: &str =
    "trust the files|trust this (folder|directory|workspace|repository)|do you trust|yes, proceed";
const TRUST_LINES: usize = 15;
const INPUT_MARK: char = '❯';
const RULE: char = '─';
const RULE_MIN: usize = 8;
const SURVEY_OPTIONS: [&str; 2] =
    ["1: Bad    2: Fine   3: Good   0: Dismiss", "1: Bad    2: Fine   3: Good   4: Unsure 0: Dismiss"];
const SURVEY_BULLET: &str = "● ";
const SURVEY_INDENT: &str = "  ";
const SURVEY_GAP: usize = 3;
const SURVEY_QUESTION_LINES: usize = 4;
const KNOWN: [(&str, &str); 14] = [
    (CLAUDE, "claude"),
    (CODEX, "codex"),
    (GEMINI, "gemini"),
    (OPENCODE, "opencode"),
    ("cursor", "cursor-agent"),
    ("copilot", "copilot"),
    ("amp", "amp"),
    ("droid", "droid"),
    ("pi", "pi"),
    ("qwen", "qwen"),
    ("kimi", "kimi"),
    ("cline", "cline"),
    ("goose", "goose"),
    ("aider", "aider"),
];
type ModeTable = &'static [(&'static str, &'static [&'static str])];

const DEFAULT_MODES: [(&str, ModeTable); 3] = [
    (
        "claude",
        &[
            ("accept edits", &["--permission-mode", "acceptEdits"]),
            ("auto", &["--permission-mode", "auto"]),
            ("plan", &["--permission-mode", "plan"]),
            ("skip permissions (dangerous)", &["--dangerously-skip-permissions"]),
        ],
    ),
    (
        "codex",
        &[
            ("read only", &["--sandbox", "read-only"]),
            ("workspace write", &["--sandbox", "workspace-write"]),
            ("no sandbox, no approvals (dangerous)", &["--dangerously-bypass-approvals-and-sandbox"]),
        ],
    ),
    (
        GEMINI,
        &[
            ("auto edit", &["--approval-mode", "auto_edit"]),
            ("plan", &["--approval-mode", "plan"]),
            ("yolo (dangerous)", &["--yolo"]),
        ],
    ),
];

pub type Mode = (String, Vec<String>);

pub fn kinds(config: &Config) -> Vec<String> {
    let mut kinds: Vec<String> = KNOWN.iter().map(|(kind, _)| (*kind).to_string()).collect();
    for kind in config.agent_commands.keys() {
        if !kinds.contains(kind) {
            kinds.push(kind.clone());
        }
    }
    kinds
}

pub fn command(config: &Config, kind: &str) -> String {
    config
        .agent_commands
        .get(kind)
        .cloned()
        .or_else(|| KNOWN.iter().find(|(k, _)| *k == kind).map(|(_, bin)| (*bin).to_string()))
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

pub fn installed(program: &str) -> bool {
    found(program, std::env::var_os("PATH").as_deref())
}

pub fn modes(config: &Config, kind: &str) -> Vec<Mode> {
    let mut modes: Vec<Mode> = DEFAULT_MODES
        .iter()
        .filter(|(k, _)| *k == kind)
        .flat_map(|(_, modes)| modes.iter())
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

pub fn args(config: &Config, kind: &str) -> Vec<String> {
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

pub fn mode_of(kind: &str, args: &[String], modes: &[Mode]) -> Option<String> {
    let args = normalized(kind, args);
    longest_first(modes)
        .into_iter()
        .find(|(_, run)| position_of(&args, &normalized(kind, run)).is_some())
        .map(|(name, _)| name.clone())
}

fn normalized(kind: &str, args: &[String]) -> Vec<String> {
    if kind != GEMINI {
        return args.to_vec();
    }
    args.iter()
        .flat_map(|arg| match arg.as_str() {
            "-y" | "--yolo" => vec!["--approval-mode".into(), "yolo".into()],
            _ => arg
                .split_once('=')
                .filter(|(flag, _)| *flag == "--approval-mode")
                .map_or_else(|| vec![arg.clone()], |(flag, value)| vec![flag.into(), value.into()]),
        })
        .collect()
}

pub fn extra_args(kind: &str, args: &[String], modes: &[Mode]) -> Vec<String> {
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

pub fn with_mode(kind: &str, args: &[String], modes: &[Mode], name: Option<&str>) -> Vec<String> {
    [mode_args(modes, name), extra_args(kind, args, modes)].concat()
}

pub fn with_extra(kind: &str, args: &[String], modes: &[Mode], extra: Vec<String>) -> Vec<String> {
    [mode_args(modes, mode_of(kind, args, modes).as_deref()), extra].concat()
}

pub fn is_dangerous(mode: &str) -> bool {
    mode.to_lowercase().contains("danger")
}

pub fn split_args(text: &str) -> Vec<String> {
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

pub fn quote(arg: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "-_./:@%+=,".contains(c);
    if !arg.is_empty() && arg.chars().all(safe) {
        return arg.to_string();
    }
    format!("'{}'", arg.replace('\'', r"'\''"))
}

pub fn join_args(args: &[String]) -> String {
    args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ")
}

pub fn command_line(config: &Config, kind: &str) -> String {
    std::iter::once(quote(&command(config, kind)))
        .chain(args(config, kind).iter().map(|a| quote(a)))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn resume_line(config: &Config, agent: &AgentState) -> Option<String> {
    let kind = agent.kind.as_str();
    let args = with_mode(kind, &args(config, kind), &modes(config, kind), agent.mode.as_deref());
    let conversation = agent.conversation.clone();
    let line = match kind {
        CLAUDE | GEMINI => [args, vec!["--resume".into(), conversation]].concat(),
        CODEX => [vec!["resume".into()], args, vec![conversation]].concat(),
        _ => return None,
    };
    Some(std::iter::once(command(config, kind)).chain(line).map(|a| quote(&a)).collect::<Vec<_>>().join(" "))
}

pub fn resolve(config: &Config, explicit: Option<&str>, running: Option<&str>) -> Option<String> {
    explicit
        .or_else(|| (config.agent != AUTO && !config.agent.trim().is_empty()).then_some(config.agent.as_str()))
        .or(running)
        .map(str::to_string)
}

fn basename(arg: &str) -> &str {
    Path::new(arg).file_name().and_then(|n| n.to_str()).unwrap_or(arg)
}

pub fn detect(config: &Config, argv: &[String]) -> Option<String> {
    let names: Vec<&str> = argv.iter().take(2).map(|a| basename(a)).collect();
    kinds(config).into_iter().find(|kind| {
        let bin = command(config, kind);
        names.contains(&basename(&bin))
            || (kind == CODEX && argv.get(1).is_some_and(|arg| arg.ends_with("/codex/bin/codex.js")))
    })
}

#[derive(Debug, Default)]
pub struct TrustPrompt {
    compiled: Option<(String, Option<regex::Regex>)>,
}

impl TrustPrompt {
    pub fn regex(&mut self, pattern: &str) -> Option<&regex::Regex> {
        if self.compiled.as_ref().is_none_or(|(compiled, _)| compiled != pattern) {
            let regex = regex::RegexBuilder::new(pattern).case_insensitive(true).build().ok();
            self.compiled = Some((pattern.to_string(), regex));
        }
        self.compiled.as_ref().and_then(|(_, regex)| regex.as_ref())
    }
}

pub fn asks_trust(regex: &regex::Regex, screen: &str) -> bool {
    let lines: Vec<&str> = screen.lines().filter(|line| !line.trim().is_empty()).collect();
    regex.is_match(&lines[lines.len().saturating_sub(TRUST_LINES)..].join("\n"))
}

pub fn input_box(agent: &str, screen: &str) -> Option<bool> {
    (agent == CLAUDE).then(|| box_rule(&screen.lines().collect::<Vec<_>>()).is_some())
}

pub fn survey(agent: &str, screen: &str) -> bool {
    let lines: Vec<&str> = screen.lines().collect();
    let Some(rule) = box_rule(&lines).filter(|_| agent == CLAUDE) else { return false };
    let above = &lines[..rule];
    let Some(gap) = above.iter().rev().take(SURVEY_GAP).position(|line| survey_options(line)) else { return false };
    let question = &above[..above.len() - 1 - gap];
    question
        .iter()
        .rev()
        .take(SURVEY_QUESTION_LINES)
        .find(|line| !survey_indented(line))
        .is_some_and(|line| line.starts_with(SURVEY_BULLET))
}

fn box_rule(lines: &[&str]) -> Option<usize> {
    let rule = |line: &str| {
        let line = line.trim();
        line.chars().count() >= RULE_MIN && line.chars().all(|c| c == RULE)
    };
    lines.windows(2).rposition(|pair| rule(pair[0]) && pair[1].starts_with(INPUT_MARK))
}

fn survey_options(line: &str) -> bool {
    line.strip_prefix(SURVEY_INDENT).is_some_and(|rest| SURVEY_OPTIONS.contains(&rest.trim_end()))
}

fn survey_indented(line: &str) -> bool {
    line.strip_prefix(SURVEY_INDENT).is_some_and(|rest| rest.starts_with(|c: char| !c.is_whitespace()))
}

#[cfg(test)]
mod tests;
