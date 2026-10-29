use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::agents::activity::Activity;
use crate::agents::context::counting::Ledger;
use crate::agents::context::discovery::Follower;
use crate::agents::context::{Compaction, Context, Look, Record};

pub(in crate::agents) const NAME: &str = "vibe";
pub(in crate::agents) const TITLE: &str = "Vibe CLI";
pub(in crate::agents) const COMMAND: &str = "vibe";
pub(in crate::agents) const UNFOUND: &str = "Vibe CLI has no conversation for this pane yet";
pub(in crate::agents) const ACCOUNT: Option<crate::agents::AccountHome> = None;
pub(in crate::agents) const MODEL_FLAG: Option<&str> = None;
pub(in crate::agents) const MODEL_ALIAS: Option<&str> = None;
pub(in crate::agents) const MODELS: &[&str] = &[];
pub(in crate::agents) const CATALOGUE: Option<fn() -> Vec<String>> = None;
pub(in crate::agents) const EFFORT: Option<crate::agents::Effort> = None;
pub(in crate::agents) const COMPACT: Option<crate::agents::Compact> =
    Some(crate::agents::Compact { command: "/compact", keeps: true });
pub(in crate::agents) const ADD_DIR: Option<&str> = Some("--add-dir");
pub(in crate::agents) const UNATTENDED: &[&str] = &["--agent", "auto-approve"];
pub(in crate::agents) const MODES: crate::agents::ModeTable = &[
    ("ask", &["--agent", "ask"]),
    ("smart approve", &["--agent", "smart-approve"]),
    ("auto-approve (dangerous)", &["--agent", "auto-approve"]),
];
pub(in crate::agents) const USAGE: Option<crate::agents::usage::Spec> = None;
pub(in crate::agents) const INPUT_BOX: Option<bool> = None;
pub(in crate::agents) const SURVEY: bool = false;
pub(in crate::agents) const HOLDS_PROMPTS: bool = true;
pub(in crate::agents) const STATUS_BY_FOLDER: bool = false;
pub(in crate::agents) const RECORD_BY_FOLDER: bool = false;

#[expect(clippy::unnecessary_wraps, reason = "Known dispatch shares an optional resume signature across agents")]
pub(in crate::agents) fn resume(mut args: Vec<String>, conversation: String) -> Option<Vec<String>> {
    args.extend(["--resume".to_string(), conversation]);
    Some(args)
}

fn spelled(arg: &str) -> Vec<String> {
    let owned = |parts: &[&str]| parts.iter().map(|part| (*part).to_string()).collect();
    match arg {
        "--yolo" | "--auto-approve" => owned(&["--agent", "auto-approve"]),
        "--smart-approve" => owned(&["--agent", "smart-approve"]),
        _ => match arg.strip_prefix("--agent=") {
            Some(name) => owned(&["--agent", name]),
            None => owned(&[arg]),
        },
    }
}

pub(in crate::agents) fn normalized(args: &[String]) -> Vec<String> {
    args.iter().flat_map(|arg| spelled(arg)).collect()
}

pub(in crate::agents) fn local_command(text: &str) -> bool {
    let text = text.trim();
    text.starts_with(['!', '&']) || (text.starts_with('/') && !text.starts_with("/skill"))
}

pub(in crate::agents) fn runs_as(arg: &str) -> bool {
    Path::new(arg).file_name() == Some(OsStr::new("vibe-rs"))
}

#[derive(Debug)]
struct Held {
    path: PathBuf,
    id: String,
    compaction: Compaction,
    subagents: usize,
    resumable: bool,
}

#[derive(Debug, Default)]
pub(in crate::agents) struct Reader {
    following: Follower<session::Session>,
    held: Option<Held>,
}

impl Reader {
    pub(in crate::agents) fn update(
        &mut self,
        look: &Look<'_>,
        since: SystemTime,
        shown: &mut Option<Context>,
        ledger: &mut Ledger,
    ) -> Option<SystemTime> {
        let (pid, args) = (look.pid, look.args.clone());
        let vibe_dir = look.vibe_dir.map(Path::to_path_buf);
        let mut prompted = None;
        self.following.poll(
            move |previous| session::follow(pid, &args, previous, vibe_dir),
            |found| {
                self.held = found.map(|session| {
                    ledger.apply(&session.id, since, std::mem::take(&mut session.counted));
                    shown.clone_from(&session.context);
                    prompted = session.prompted;
                    Held {
                        path: session.path.clone(),
                        id: session.id.clone(),
                        compaction: session.compaction,
                        subagents: session.subagents,
                        resumable: session.resumable,
                    }
                });
                if self.held.is_none() {
                    *shown = None;
                }
            },
        );
        prompted
    }

    pub(in crate::agents) fn activity() -> Option<Activity> {
        None
    }

    pub(in crate::agents) fn compaction(&self) -> Compaction {
        self.held.as_ref().map(|held| held.compaction).unwrap_or_default()
    }

    pub(in crate::agents) fn subagents(&self) -> usize {
        self.held.as_ref().map_or(0, |held| held.subagents)
    }

    pub(in crate::agents) fn record(&self) -> Option<Record> {
        self.held.as_ref().map(|held| Record::Vibe(held.path.clone()))
    }

    pub(in crate::agents) fn conversation(&self) -> Option<&str> {
        let held = self.held.as_ref()?;
        held.resumable.then_some(held.id.as_str())
    }
}

pub(in crate::agents) mod message;
pub(in crate::agents) mod session;

#[cfg(test)]
pub(crate) mod fake;

#[cfg(test)]
mod tests;
