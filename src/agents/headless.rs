use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agents::Known;
use crate::agents::probe::{self, Heard, Limits, Running};
use crate::error::{Error, Result};

const GRACE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Used {
    pub(crate) input: u64,
    pub(crate) output: u64,
    pub(crate) cache_read: u64,
    pub(crate) cache_write: u64,
}

impl Used {
    pub(crate) fn total(self) -> u64 {
        self.input + self.output + self.cache_read + self.cache_write
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Access {
    Cornercase,
    Full,
}

impl Access {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Cornercase => "cornercase",
            Self::Full => "full",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "cornercase" => Some(Self::Cornercase),
            "full" => Some(Self::Full),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Tools {
    #[default]
    Recap,
    Bot(Access),
}

pub(crate) type Env = Vec<(&'static str, Option<OsString>)>;

#[derive(Debug, Clone, Default)]
pub(crate) struct Ask {
    pub(crate) prompt: String,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) folder: PathBuf,
    pub(crate) socket: Option<String>,
    pub(crate) tools: Tools,
    pub(crate) conversation: Option<String>,
    pub(crate) env: Env,
    pub(crate) protected: Vec<&'static str>,
    pub(crate) unreadable: Vec<PathBuf>,
    pub(crate) stops: Vec<&'static str>,
    pub(crate) repositories: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Action {
    pub(crate) tool: String,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Ran {
    pub(crate) text: String,
    pub(crate) model: Option<String>,
    pub(crate) tokens: Used,
    pub(crate) steps: usize,
    pub(crate) session: Option<String>,
    pub(crate) fresh: Vec<Action>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BotSupport {
    pub(crate) accesses: &'static [Access],
    pub(crate) why: Option<&'static str>,
    pub(crate) reads_anywhere: bool,
    pub(crate) reads_git: bool,
    pub(crate) effort_needs_model: bool,
}

pub(in crate::agents) type Ended = fn(&mut Ran) -> Option<Result<()>>;

pub(crate) struct Spec {
    pub(in crate::agents) command: fn(&str, &Ask) -> Command,
    pub(in crate::agents) read: fn(&str, &mut Ran) -> Option<Result<()>>,
    pub(in crate::agents) bot: Option<BotSupport>,
    pub(in crate::agents) recap: bool,
    pub(in crate::agents) stderr: bool,
    pub(in crate::agents) prepare: Option<fn(&Ask) -> Result<()>>,
    pub(in crate::agents) ended: Option<Ended>,
}

#[derive(Clone, Copy)]
pub(crate) struct Call<'a> {
    pub(crate) agent: Known,
    pub(crate) program: &'a str,
    pub(crate) home: Option<&'a Path>,
    pub(crate) timeout: Duration,
}

#[derive(Default)]
pub(crate) struct Watch {
    pub(crate) progress: Arc<AtomicUsize>,
    pub(crate) steps: Option<Box<dyn FnMut(Action) + Send>>,
    pub(crate) running: Option<Running>,
}

pub(crate) fn run(call: &Call<'_>, ask: &Ask, progress: &Arc<AtomicUsize>) -> Result<Ran> {
    run_watched(call, ask, Watch { progress: Arc::clone(progress), steps: None, running: None })
}

fn prepared(call: &Call<'_>, ask: &Ask, spec: &Spec) -> Command {
    let mut command = (spec.command)(call.program, ask);
    command.current_dir(&ask.folder);
    if let Some(socket) = &ask.socket {
        command.env(crate::paths::SOCKET_ENV, socket);
    }
    for (key, value) in &ask.env {
        match value {
            Some(value) => command.env(key, value),
            None => command.env_remove(key),
        };
    }
    if let (Some(home), Some(account)) = (call.home, call.agent.account()) {
        for key in account.remove {
            command.env_remove(key);
        }
        command.env(account.env, home);
    }
    command
}

fn refused(agent: Known, spec: &Spec, tools: Tools) -> Option<Error> {
    let name = agent.name();
    match tools {
        Tools::Recap if !spec.recap => Some(Error::Probe(format!("{name} cannot write a recap"))),
        Tools::Bot(access) if !spec.bot.is_some_and(|bot| bot.accesses.contains(&access)) => {
            Some(Error::Probe(format!("{name} cannot be a bot with {} access", access.name())))
        }
        _ => None,
    }
}

fn limits_of(call: &Call<'_>, ask: &Ask, spec: &Spec) -> Limits {
    let contain = matches!(ask.tools, Tools::Bot(Access::Cornercase));
    Limits {
        name: call.agent.name(),
        timeout: call.timeout,
        grace: GRACE,
        close_input: true,
        contain,
        stderr: spec.stderr,
    }
}

fn step_of(outcome: Option<Result<()>>, ran: &Ran, progress: &AtomicUsize) -> probe::Step<Ran> {
    match outcome {
        None => {
            progress.store(ran.steps, Ordering::Relaxed);
            probe::Step::Read
        }
        Some(Ok(())) => probe::Step::Answer(Ok(ran.clone())),
        Some(Err(error)) => probe::Step::Answer(Err(error)),
    }
}

pub(crate) fn run_watched(call: &Call<'_>, ask: &Ask, watch: Watch) -> Result<Ran> {
    let agent = call.agent;
    let spec =
        agent.headless().ok_or_else(|| Error::Probe(format!("{} cannot run without its interface", agent.name())))?;
    if let Some(error) = refused(agent, &spec, ask.tools) {
        return Err(error);
    }
    if let Some(prepare) = spec.prepare {
        prepare(ask)?;
    }
    let command = prepared(call, ask, &spec);
    let limits = limits_of(call, ask, &spec);
    let Watch { progress, mut steps, running } = watch;
    let (read, ended) = (spec.read, spec.ended);
    let running = running.unwrap_or_default();
    let stopped = running.clone();
    let mut ran = Ran::default();
    probe::run_heard(limits, command, "", &running, move |heard| {
        let outcome = match heard {
            Heard::Line(line) => read(line, &mut ran),
            Heard::End if stopped.stopped() => None,
            Heard::End => ended.and_then(|ended| ended(&mut ran)),
        };
        for action in ran.fresh.drain(..) {
            if let Some(steps) = steps.as_mut() {
                steps(action);
            }
        }
        step_of(outcome, &ran, &progress)
    })
}

pub(in crate::agents) const GH_READS: [&str; 5] =
    ["gh pr list", "gh pr view", "gh pr checks", "gh issue list", "gh issue view"];
const GIT_READS: [&str; 4] = ["log", "show", "diff", "status"];
const RULE_OVERHEAD: usize = 9;
const GIT_BUDGET: usize = 64 * 1024;
const WINDOWS_GIT_BUDGET: usize = 6 * 1024;

fn awkward(c: char, windows: bool) -> bool {
    c.is_control() || "'\"`$!,()*?".contains(c) || (c == '\\' && !windows)
}

fn plain(c: char) -> bool {
    c.is_alphanumeric() || "/._-+@:=%".contains(c)
}

fn spellings(folder: &Path, windows: bool) -> Vec<String> {
    let Some(path) = folder.to_str().filter(|path| !path.is_empty() && !path.chars().any(|c| awkward(c, windows)))
    else {
        return Vec::new();
    };
    let backslash = path.contains('\\');
    let mut all = Vec::new();
    if !backslash && path.chars().all(plain) {
        all.push(path.to_string());
    }
    all.push(format!("'{path}'"));
    if !backslash {
        all.push(format!("\"{path}\""));
    }
    all
}

fn rules_of(spellings: &[String]) -> Vec<String> {
    spellings.iter().flat_map(|spelling| GIT_READS.map(|sub| format!("git -C {spelling} {sub}"))).collect()
}

fn size(rules: &[String]) -> usize {
    rules.iter().map(|rule| rule.len() + RULE_OVERHEAD).sum()
}

pub(in crate::agents) fn git_budget(windows: bool) -> usize {
    if windows { WINDOWS_GIT_BUDGET } else { GIT_BUDGET }
}

pub(in crate::agents) fn git_reads(ask: &Ask, windows: bool, budget: usize) -> Vec<String> {
    let folders: Vec<Vec<String>> = ask.repositories.iter().map(|folder| spellings(folder, windows)).collect();
    let full: Vec<String> = folders.iter().flat_map(|spellings| rules_of(spellings)).collect();
    if size(&full) <= budget {
        return full;
    }
    let quoted: Vec<Vec<String>> = folders
        .iter()
        .filter_map(|all| all.iter().find(|spelling| spelling.starts_with('\'')).cloned())
        .map(|one| vec![one])
        .collect();
    let mut kept = Vec::new();
    let mut used = 0;
    for spellings in &quoted {
        let rules = rules_of(spellings);
        used += size(&rules);
        if used > budget {
            break;
        }
        kept.extend(rules);
    }
    crate::log::warning!(
        "bots",
        "too many projects for a bot's git rules, the rest get none",
        folders = quoted.len(),
        kept = kept.len() / GIT_READS.len()
    );
    kept
}

pub(in crate::agents) fn started(program: &str, args: Vec<String>) -> Command {
    let mut command = crate::process::program::command_without_cmd(program);
    command.args(args);
    command
}

pub(in crate::agents) fn owned(args: &[&str]) -> Vec<String> {
    args.iter().map(ToString::to_string).collect()
}

pub(in crate::agents) fn first_line(text: &str) -> &str {
    text.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or_default()
}

pub(in crate::agents) fn action(tool: &str, text: &str) -> Action {
    Action { tool: tool.to_string(), text: crate::text::truncate_right(&crate::text::one_line(text), 200) }
}

pub(in crate::agents) fn step(tool: &str, detail: &str) -> Action {
    if detail.is_empty() { action(tool, tool) } else { action(tool, &format!("{tool} {detail}")) }
}

pub(in crate::agents) fn shell(tool: &str, command: &str) -> Action {
    action(tool, first_line(command))
}

pub(in crate::agents) fn detail<'a>(input: &'a Value, keys: &[&str]) -> &'a str {
    keys.iter().find_map(|key| input[*key].as_str().filter(|text| !text.is_empty())).unwrap_or_default()
}

pub(in crate::agents) fn heard(ran: &mut Ran, action: Action) {
    ran.steps += 1;
    ran.fresh.push(action);
    ran.text.clear();
}

pub(in crate::agents) fn answered(ran: &mut Ran, title: &str) -> Result<()> {
    let text = ran.text.trim().to_string();
    if text.is_empty() {
        return Err(Error::Probe(format!("{title} answered nothing")));
    }
    ran.text = text;
    Ok(())
}

#[cfg(test)]
pub(in crate::agents) fn bot_ask(folder: &Path, tools: Tools, conversation: Option<&str>) -> Ask {
    Ask {
        prompt: "hi".into(),
        model: Some("opus".into()),
        folder: folder.to_path_buf(),
        tools,
        conversation: conversation.map(str::to_string),
        protected: vec!["bot.json", "chat.jsonl", "secrets"],
        unreadable: vec![PathBuf::from("/b.key")],
        stops: vec!["kill-server", "restart", "update"],
        ..Ask::default()
    }
}

#[cfg(test)]
pub(in crate::agents) fn args_of(command: &Command) -> Vec<String> {
    command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect()
}

#[cfg(test)]
pub(in crate::agents) fn plain_args(
    spec: fn(&str, &Ask) -> Command,
    tools: Tools,
    conversation: Option<&str>,
) -> Vec<String> {
    let model = Some(String::from("m"));
    let ask =
        Ask { prompt: "- hi".into(), model, tools, conversation: conversation.map(str::to_string), ..Ask::default() };
    args_of(&spec("agent", &ask))
}

#[cfg(test)]
pub(in crate::agents) fn steps_of(ran: &Ran) -> Vec<(&str, &str)> {
    ran.fresh.iter().map(|action| (action.tool.as_str(), action.text.as_str())).collect()
}

#[cfg(test)]
pub(in crate::agents) fn feed(read: fn(&str, &mut Ran) -> Option<Result<()>>, text: &str) -> (Ran, Option<Result<()>>) {
    let mut ran = Ran::default();
    let mut last = None;
    for line in text.lines() {
        last = read(line, &mut ran);
    }
    (ran, last)
}

#[cfg(test)]
mod tests;
