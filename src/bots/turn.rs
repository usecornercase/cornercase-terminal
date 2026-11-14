use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{BOT_ENV, Bot, brief, folder};
use crate::agents::Known;
use crate::agents::headless::{self, Ask, Call, Ran, Tools, Watch};
use crate::error::{Error, Result};

const UNSET: [&str; 3] = [crate::control::PANE_ENV, crate::control::SERVER_ENV, crate::protocol::REMOTE_ENV];
const CONTEXT: [&str; 5] =
    ["prompt is too long", "context window", "context length", "maximum context", "context_length_exceeded"];

pub(crate) struct Input {
    pub(crate) bot: Bot,
    pub(crate) folder: PathBuf,
    pub(crate) text: String,
    pub(crate) program: String,
    pub(crate) home: Option<PathBuf>,
    pub(crate) socket: Option<String>,
    pub(crate) timeout: Duration,
}

pub(crate) fn path_with(bin: &Path, path: Option<&OsStr>) -> Option<OsString> {
    let rest = path.map(std::env::split_paths).into_iter().flatten();
    std::env::join_paths(std::iter::once(bin.to_path_buf()).chain(rest)).ok()
}

pub(crate) fn joined(texts: &[String]) -> String {
    texts.join("\n\n")
}

fn environment(bot: &Bot, bin: Option<&Path>, path: Option<&OsStr>) -> headless::Env {
    let mut env: headless::Env = vec![(BOT_ENV, Some(OsString::from(&bot.id)))];
    if let Some(path) = bin.and_then(|bin| path_with(bin, path)) {
        env.push(("PATH", Some(path)));
    }
    env.extend(UNSET.map(|key| (key, None)));
    env
}

pub(crate) fn ask(input: &Input, bin: Option<&Path>, path: Option<&OsStr>) -> Ask {
    let bot = &input.bot;
    let prompt = if bot.conversation.is_none() {
        let (about, memory) = folder::notes(&input.folder);
        brief::opening(bot, &input.folder, (&about, &memory), &input.text)
    } else {
        input.text.clone()
    };
    let effort = bot.effort.as_deref().filter(|level| crate::agents::effort_refusal(&bot.agent, level).is_none());
    Ask {
        prompt,
        model: bot.model.clone(),
        effort: effort.map(str::to_string),
        folder: input.folder.clone(),
        socket: input.socket.clone(),
        tools: Tools::Bot(bot.access),
        conversation: bot.conversation.clone(),
        env: environment(bot, bin, path),
    }
}

pub(crate) fn run(input: &Input, watch: Watch) -> Result<Ran> {
    let kind =
        Known::of(&input.bot.agent).ok_or_else(|| Error::Probe(format!("{} is not an agent", input.bot.agent)))?;
    let bin = crate::process::program::own_binary().parent().map(Path::to_path_buf);
    let ask = ask(input, bin.as_deref(), std::env::var_os("PATH").as_deref());
    let call = Call { agent: kind, program: &input.program, home: input.home.as_deref(), timeout: input.timeout };
    headless::run_watched(&call, &ask, watch)
}

pub(crate) fn failure(agent: &str, error: &Error, bot: &str) -> String {
    let text = error.to_string();
    if text == "error_max_turns" {
        return format!("{agent} stopped after 40 steps, its limit for one message");
    }
    let lower = text.to_lowercase();
    if CONTEXT.iter().any(|words| lower.contains(words)) {
        return format!("{text}; start a new conversation: cornercase bot reset {bot}");
    }
    text
}

#[cfg(test)]
mod tests;
