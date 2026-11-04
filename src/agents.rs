pub(crate) mod activity;
pub(crate) mod awake;
pub(crate) mod breakdown;
pub mod claude;
pub(crate) mod closing;
pub(crate) mod codex;
pub(crate) mod context;
pub mod copilot;
pub(crate) mod cursor;
pub(crate) mod gemini;
pub(crate) mod grok;
mod known;
pub(crate) mod launch;
pub(crate) mod opencode;
pub(crate) mod probe;
pub mod stats;
pub(crate) mod tree;
pub(crate) mod usage;
pub(crate) mod vibe;

pub use known::Known;

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use self::launch::Refusal;
use crate::config::Config;
use crate::{paths, process};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AgentState {
    pub(crate) kind: String,
    pub(crate) conversation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) account: Option<String>,
}

pub(crate) const AUTO: &str = "auto";
pub(crate) const DEFAULT_TRUST_PROMPT: &str =
    "trust the files|trust this (folder|directory|workspace|repository)|do you trust|yes, proceed";
const TRUST_LINES: usize = 15;
const POWERSHELL_BARE: &str = r"-_./\+=";
const POWERSHELL_START: &str = r"-_./\";
const POWERSHELL_QUOTES: [char; 5] = ['\'', '\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'];
const BATCH_EXTENSIONS: [&str; 2] = ["cmd", "bat"];
const BATCH_SPECIAL: [char; 8] = ['&', '|', '<', '>', '^', '%', '!', '"'];
pub(crate) const STOP_PARSING: &str = "--%";
const CMD_NAME_ENDS: [char; 3] = ['=', ',', ';'];
const CMD: &str = "cmd";
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaneShell {
    PowerShell,
    Cmd,
    Other,
}

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

pub(crate) fn installed(program: &str) -> bool {
    process::program::locate(program).is_some()
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
    let flags: Vec<&str> = known.model_flag().into_iter().chain(known.model_alias()).collect();
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
pub(crate) struct AccountHome {
    pub(crate) env: &'static str,
    pub(crate) default: &'static [&'static str],
    pub(crate) subdir: Option<&'static str>,
    pub(crate) links: &'static [&'static str],
    pub(crate) remove: &'static [&'static str],
    pub(crate) login: &'static [&'static str],
    pub(crate) status: Option<&'static [&'static str]>,
    pub(crate) tokens: &'static [&'static str],
}

impl AccountHome {
    pub(crate) fn default_home(self, var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
        let absolute = |name: &str| var(name).map(PathBuf::from).filter(|path| path.is_absolute());
        absolute(self.env)
            .or_else(|| absolute(paths::HOME_VAR).map(|home| self.default.iter().fold(home, |p, s| p.join(s))))
    }

    pub(crate) fn config(self, home: &Path) -> PathBuf {
        self.subdir.map_or_else(|| home.to_path_buf(), |subdir| home.join(subdir))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Compact {
    pub(crate) command: &'static str,
    pub(crate) keeps: bool,
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
    let (Some(model), Some(flag)) =
        (model.map(str::trim).filter(|m| !m.is_empty()), Known::of(kind).and_then(Known::model_flag))
    else {
        return args;
    };
    let mut out = without_model(kind, &args);
    out.extend([flag.to_string(), model.to_string()]);
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

pub(crate) fn typed_line(words: impl IntoIterator<Item = String>) -> String {
    if cfg!(windows) {
        return powershell_line(words);
    }
    words.into_iter().map(|word| quote(&word)).collect::<Vec<_>>().join(" ")
}

fn powershell_line(words: impl IntoIterator<Item = String>) -> String {
    let mut words = words.into_iter();
    let program = words.next().map(|program| {
        let named = !program.starts_with('-');
        if named && powershell_bare(&program) { program } else { format!("& {}", powershell_quoted(&program)) }
    });
    let words = words.map(|word| if powershell_bare(&word) { word } else { powershell_quoted(&word) });
    program.into_iter().chain(words).collect::<Vec<_>>().join(" ")
}

fn powershell_bare(word: &str) -> bool {
    word.starts_with(|c: char| c.is_ascii_alphanumeric() || POWERSHELL_START.contains(c))
        && word != "--"
        && word.chars().all(|c| c.is_ascii_alphanumeric() || POWERSHELL_BARE.contains(c))
}

pub(crate) fn powershell_quoted(word: &str) -> String {
    let mut quoted = String::with_capacity(word.len() + 2);
    quoted.push('\'');
    for c in word.chars() {
        if POWERSHELL_QUOTES.contains(&c) {
            quoted.push(c);
        }
        quoted.push(c);
    }
    quoted.push('\'');
    quoted
}

impl PaneShell {
    pub(crate) fn of(shell: Option<i32>) -> Self {
        Self::named(shell.and_then(process::name).as_deref())
    }

    pub(crate) fn named(name: Option<&str>) -> Self {
        match name {
            None => Self::PowerShell,
            Some(name) if process::program::is_powershell(name) => Self::PowerShell,
            Some(name) if process::same_file_name(process::program_name(name), CMD) => Self::Cmd,
            Some(_) => Self::Other,
        }
    }
}

pub(crate) fn typed_on_windows(
    line: &str,
    shell: PaneShell,
    runs: impl Fn(&str) -> Option<PathBuf>,
) -> Result<String, Refusal> {
    let Some(words) = typed_words(line) else { return Ok(line.to_string()) };
    let refusal = match shell {
        PaneShell::PowerShell => split_word(&words),
        PaneShell::Cmd => cmd_word(&words),
        PaneShell::Other => None,
    };
    if let Some(refusal) = refusal {
        return Err(refusal);
    }
    let typed = match shell {
        PaneShell::PowerShell => line.to_string(),
        PaneShell::Cmd => words.join(" "),
        PaneShell::Other => join_args(&words),
    };
    without_cmd(typed, words, shell == PaneShell::PowerShell, runs)
}

fn refusal(reason: &'static str, message: &str) -> Refusal {
    Refusal { reason, message: format!("not started: {message}") }
}

fn split_word(words: &[String]) -> Option<Refusal> {
    if words.iter().any(|word| word == STOP_PARSING) {
        return Some(refusal(
            "the stop-parsing token",
            "the argument `--%` makes Windows PowerShell 5.1 pass the words after it without their quotes, so a word \
             with a space could reach the agent as options of its own; remove it",
        ));
    }
    let word = words.iter().find(|word| splits(word))?;
    let why = if word.contains('"') { "holds a `\"`" } else { "holds whitespace and ends with `\\`" };
    Some(refusal(
        "a word PowerShell would split",
        &format!(
            "the argument `{word}` {why}, which Windows PowerShell 5.1 passes on in pieces when it starts a program, \
             so part of it could reach the agent as an option of its own; change the argument"
        ),
    ))
}

pub(crate) fn splits(word: &str) -> bool {
    word.contains('"') || (word.ends_with('\\') && word.contains(char::is_whitespace))
}

fn cmd_word(words: &[String]) -> Option<Refusal> {
    if let Some((word, special)) = special_in(words) {
        return Some(refusal(
            "a character cmd.exe reads",
            &format!(
                "the pane's shell is cmd.exe, which would read the `{special}` in `{word}` as its own; start the \
                 agent in a PowerShell pane"
            ),
        ));
    }
    if let Some(word) = words.iter().find(|word| word.is_empty() || word.contains(char::is_whitespace)) {
        return Some(refusal(
            "a word cmd.exe would split",
            &format!(
                "the pane's shell is cmd.exe, which does not read the quotes around `{word}`, so it would reach the \
                 agent in pieces or not at all; start the agent in a PowerShell pane"
            ),
        ));
    }
    let program = words.first().filter(|program| program.contains(CMD_NAME_ENDS))?;
    Some(refusal(
        "a program name cmd.exe would cut",
        &format!(
            "the pane's shell is cmd.exe, which ends a program's name at `=`, `,` or `;`, so `{program}` would not \
             start; start the agent in a PowerShell pane"
        ),
    ))
}

fn special(words: &[String]) -> Option<char> {
    special_in(words).map(|(_, special)| special)
}

fn special_in(words: &[String]) -> Option<(&str, char)> {
    words.iter().find_map(|word| word.chars().find(|c| BATCH_SPECIAL.contains(c)).map(|c| (word.as_str(), c)))
}

fn without_cmd(
    line: String,
    words: Vec<String>,
    powershell: bool,
    runs: impl Fn(&str) -> Option<PathBuf>,
) -> Result<String, Refusal> {
    let Some(program) = words.first() else { return Ok(line) };
    let target = runs(program).unwrap_or_else(|| PathBuf::from(program));
    if !is_batch(&target) {
        return Ok(line);
    }
    if let Some(shim) = powershell.then(|| shim_words(&target, &runs)).flatten() {
        return Ok(powershell_line(shim.into_iter().chain(words.into_iter().skip(1))));
    }
    let Some(special) = special(&words) else {
        return Ok(line);
    };
    Err(refusal(
        "a batch file",
        &format!(
            "`{}` is a batch file that is not an npm shim cornercase can read, so it runs through cmd.exe, and cmd.exe \
             would read the `{special}` in the line as its own; install the agent as an .exe or with npm, or keep its \
             .ps1",
            target.display()
        ),
    ))
}

fn is_batch(program: &Path) -> bool {
    program
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| BATCH_EXTENSIONS.iter().any(|batch| extension.eq_ignore_ascii_case(batch)))
}

fn shim_words(shim: &Path, runs: impl Fn(&str) -> Option<PathBuf>) -> Option<Vec<String>> {
    let words = process::program::shim::target(shim, runs)?;
    words.into_iter().map(|word| word.into_os_string().into_string().ok()).collect()
}

fn typed_words(line: &str) -> Option<Vec<String>> {
    let mut rest = line.strip_prefix("& ").unwrap_or(line);
    let mut words = Vec::new();
    while !rest.is_empty() {
        let (word, after) = typed_word(rest)?;
        words.push(word);
        rest = after;
    }
    Some(words)
}

fn typed_word(text: &str) -> Option<(String, &str)> {
    let Some(quoted) = text.strip_prefix('\'') else {
        let (word, rest) = text.split_once(' ').unwrap_or((text, ""));
        return Some((word.to_string(), rest));
    };
    let mut word = String::new();
    let mut chars = quoted.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        if POWERSHELL_QUOTES.contains(&c) && chars.next_if(|(_, next)| *next == c).is_none() {
            let after = &quoted[at + c.len_utf8()..];
            return Some((word, if after.is_empty() { after } else { after.strip_prefix(' ')? }));
        }
        word.push(c);
    }
    None
}

pub(crate) fn join_args(args: &[String]) -> String {
    args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ")
}

pub(crate) fn command_line(config: &Config, kind: &str) -> String {
    line(config, kind, args(config, kind))
}

fn line(config: &Config, kind: &str, args: Vec<String>) -> String {
    typed_line(std::iter::once(command(config, kind)).chain(args))
}

pub(crate) fn fresh_line(config: &Config, kind: &str, mode: Option<&str>) -> String {
    start_line(config, kind, mode, None, None).unwrap_or_else(|_| command_line(config, kind))
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
        let known: Vec<&str> = Known::ALL.into_iter().filter(|k| k.model_flag().is_some()).map(Known::name).collect();
        let known = known.join(", ");
        return Err(format!("cornercase passes a model to {known} only; give {kind} its model in agent_args"));
    }
    if let Some(refusal) = model_refusal(kind, model) {
        return Err(refusal);
    }
    let effort = effort.map(str::trim).filter(|level| !level.is_empty());
    if let Some(refusal) = effort.and_then(|level| effort_refusal(kind, level)) {
        return Err(refusal);
    }
    Ok(line(config, kind, with_effort(kind, with_model(kind, chosen, model), effort)))
}

pub(crate) fn model_refusal(kind: &str, model: Option<&str>) -> Option<String> {
    model.map(str::trim).filter(|model| !model.is_empty())?;
    let known = Known::of(kind)?;
    known.model_flag().is_none().then(|| format!("{kind} has no model flag; set its model in its own configuration"))
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
    Some(typed_line(std::iter::once(command(config, kind)).chain(line)))
}

pub(crate) fn resolve(config: &Config, explicit: Option<&str>, running: Option<&str>) -> Option<String> {
    explicit
        .or_else(|| (config.agent != AUTO && !config.agent.trim().is_empty()).then_some(config.agent.as_str()))
        .or(running)
        .map(str::to_string)
}

fn basename(arg: &str) -> &str {
    process::program_name(Path::new(arg).file_name().and_then(|n| n.to_str()).unwrap_or(arg))
}

fn named(part: Option<&str>, name: &str) -> bool {
    part.is_some_and(|part| process::same_file_name(part, name))
}

fn is_program(arg: &str, name: &str) -> bool {
    process::same_file_name(basename(arg), name)
}

pub(crate) fn detect(config: &Config, argv: &[String]) -> Option<String> {
    let names: Vec<&str> = argv.iter().take(2).map(|a| basename(a)).collect();
    let script = argv.iter().skip(1).find(|arg| !arg.starts_with('-'));
    let scripts: Vec<&str> = argv.first().into_iter().chain(script).map(String::as_str).collect();
    kinds(config).into_iter().find(|kind| {
        let bin = command(config, kind);
        names.iter().any(|name| process::same_file_name(name, basename(&bin)))
            || Known::of(kind).is_some_and(|kind| scripts.iter().any(|arg| kind.runs_as(arg)))
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
