mod link;

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{BOT_ENV, BOT_RUN_ENV, Bot, brief, folder};
use crate::agents::Known;
use crate::agents::headless::{self, Ask, Call, Ran, Tools, Watch};
use crate::error::{Error, Result};

const UNSET: [&str; 3] = [crate::control::PANE_ENV, crate::control::SERVER_ENV, crate::protocol::REMOTE_ENV];
const CONTEXT: [&str; 5] =
    ["prompt is too long", "context window", "context length", "maximum context", "context_length_exceeded"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Session {
    Chat,
    Setup,
    NewRun(u64),
    Resume { run: u64, session: String },
}

pub(crate) struct Input {
    pub(crate) bot: Bot,
    pub(crate) folder: PathBuf,
    pub(crate) text: String,
    pub(crate) session: Session,
    pub(crate) from_cornercase: bool,
    pub(crate) told: Option<String>,
    pub(crate) program: String,
    pub(crate) home: Option<PathBuf>,
    pub(crate) socket: Option<String>,
    pub(crate) bin: Option<PathBuf>,
    pub(crate) timeout: Duration,
}

pub(crate) fn path_with(first: &Path, path: Option<&OsStr>) -> Option<OsString> {
    let rest = path.map(std::env::split_paths).into_iter().flatten();
    std::env::join_paths(std::iter::once(first.to_path_buf()).chain(rest)).ok()
}

pub(crate) fn joined(texts: &[String]) -> String {
    texts.join("\n\n")
}

fn environment(bot: &Bot, session: &Session, first: Option<&Path>, path: Option<&OsStr>) -> headless::Env {
    let run = match session {
        Session::Chat | Session::Setup => None,
        Session::NewRun(run) | Session::Resume { run, .. } => Some(OsString::from(run.to_string())),
    };
    let mut env: headless::Env = vec![(BOT_ENV, Some(OsString::from(&bot.id))), (BOT_RUN_ENV, run)];
    if let Some(path) = first.and_then(|first| path_with(first, path)) {
        env.push(("PATH", Some(path)));
    }
    env.extend(UNSET.map(|key| (key, None)));
    env
}

pub(crate) fn ask(input: &Input, first: Option<&Path>, path: Option<&OsStr>) -> Ask {
    let bot = &input.bot;
    let (about, memory) = folder::notes(&input.folder);
    let notes = (about.as_str(), memory.as_str());
    let (prompt, conversation) = match &input.session {
        Session::NewRun(_) => (brief::run_opening(bot, &input.folder, notes, &input.text), None),
        Session::Resume { session, .. } => (input.text.clone(), Some(session.clone())),
        Session::Setup => (brief::setup_opening(bot, &input.folder, notes, input.told.as_deref()), None),
        Session::Chat => {
            let said = brief::said(input.from_cornercase, &input.text);
            let told = input.told.as_deref();
            match &bot.conversation {
                None => (brief::opening(bot, &input.folder, notes, told, &said), None),
                Some(session) => {
                    let text = if told.is_some() || input.from_cornercase { &said } else { &input.text };
                    (brief::resumed(told, text), Some(session.clone()))
                }
            }
        }
    };
    let effort = bot.effort.as_deref().filter(|level| crate::agents::effort_refusal(&bot.agent, level).is_none());
    Ask {
        prompt,
        model: bot.model.clone(),
        effort: effort.map(str::to_string),
        folder: input.folder.clone(),
        socket: input.socket.clone(),
        tools: Tools::Bot(bot.access),
        conversation,
        env: environment(bot, &input.session, first, path),
        protected: vec![folder::BOT_FILE, folder::CHAT_FILE],
        stops: super::stop_commands(),
    }
}

pub(crate) fn first_on_path(bin: Option<&Path>, binary: &Path) -> Option<PathBuf> {
    if !binary.is_absolute() {
        return None;
    }
    if let Some(folder) = bin {
        match std::path::absolute(folder).and_then(|folder| link::ensure(&folder, binary).map(|()| folder)) {
            Ok(folder) => return Some(folder),
            Err(error) => crate::log::warning!(
                "bots",
                "cannot keep the cornercase link, the binary's folder goes first",
                error = error
            ),
        }
    }
    binary.parent().map(Path::to_path_buf)
}

pub(crate) fn run(input: &Input, watch: Watch) -> Result<Ran> {
    let kind =
        Known::of(&input.bot.agent).ok_or_else(|| Error::Probe(format!("{} is not an agent", input.bot.agent)))?;
    let first = first_on_path(input.bin.as_deref(), &crate::process::program::own_binary());
    let ask = ask(input, first.as_deref(), std::env::var_os("PATH").as_deref());
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
