use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::agents::activity::Activity;
use crate::agents::context::counting::Ledger;
use crate::agents::context::discovery::Follower;
use crate::agents::context::{Context, Look, Record};

pub(in crate::agents) const NAME: &str = "copilot";
pub(in crate::agents) const TITLE: &str = "GitHub Copilot CLI";
pub(in crate::agents) const UNFOUND: &str = "GitHub Copilot CLI has no conversation for this pane yet";
pub(in crate::agents) const COMMAND: &str = "copilot";
pub(in crate::agents) const MODEL_FLAG: &str = "--model";
pub(in crate::agents) const MODEL_ALIAS: Option<&str> = None;
pub(in crate::agents) const EFFORT: Option<crate::agents::Effort> = Some(crate::agents::Effort {
    levels: &["none", "minimal", "low", "medium", "high", "xhigh", "max"],
    flags: &["--reasoning-effort"],
    config: None,
});
pub(in crate::agents) const ADD_DIR: Option<&str> = Some("--add-dir");
pub(in crate::agents) const UNATTENDED: &[&str] = &["--allow-all", "--no-ask-user"];
pub(in crate::agents) const MODELS: &[&str] = &["auto"];
pub(in crate::agents) const CATALOGUE: Option<fn() -> Vec<String>> = None;
pub(in crate::agents) const MODES: crate::agents::ModeTable = &[
    ("plan", &["--mode", "plan"]),
    ("autopilot", &["--mode", "autopilot"]),
    ("allow all (dangerous)", &["--allow-all"]),
];
pub(in crate::agents) const USAGE: Option<crate::agents::usage::Spec> = None;
pub(in crate::agents) const INPUT_BOX: Option<bool> = None;
pub(in crate::agents) const SURVEY: bool = false;
pub(in crate::agents) const HOLDS_PROMPTS: bool = true;
pub(in crate::agents) const STATUS_BY_FOLDER: bool = false;
pub(in crate::agents) const RECORD_BY_FOLDER: bool = false;

pub const SESSION_ENV: [&str; 6] = [
    "COPILOT_CLI",
    "COPILOT_AGENT_SESSION_ID",
    "COPILOT_LOADER_PID",
    "COPILOT_SUPERVISED",
    "COPILOT_CLI_RESOLVED_DIST_DIR",
    "COPILOT_CLI_BINARY_VERSION",
];

#[expect(clippy::unnecessary_wraps, reason = "Known dispatch shares an optional resume signature across agents")]
pub(in crate::agents) fn resume(args: Vec<String>, mut conversation: String) -> Option<Vec<String>> {
    conversation.insert_str(0, "--resume=");
    Some([args, vec![conversation]].concat())
}

fn spelled(arg: &str) -> Vec<String> {
    let owned = |parts: &[&str]| parts.iter().map(|part| (*part).to_string()).collect();
    match arg {
        "--yolo" | "--allow-all" => owned(&["--allow-all"]),
        "--plan" => owned(&["--mode", "plan"]),
        "--autopilot" => owned(&["--mode", "autopilot"]),
        _ => match arg.strip_prefix("--mode=") {
            Some(mode) => owned(&["--mode", mode]),
            None => owned(&[arg]),
        },
    }
}

pub(in crate::agents) fn normalized(args: &[String]) -> Vec<String> {
    args.iter().flat_map(|arg| spelled(arg)).collect()
}

pub(in crate::agents) fn local_command(text: &str) -> bool {
    let text = text.trim();
    text == "?" || text.starts_with(['/', '!'])
}

pub(in crate::agents) fn runs_as(arg: &str) -> bool {
    let mut parts = Path::new(arg).iter().rev().filter_map(OsStr::to_str);
    let (file, package, scope) = (parts.next(), parts.next(), parts.next());
    file == Some("npm-loader.js") && package == Some("copilot") && scope == Some("@github")
}

#[derive(Debug)]
struct Held {
    path: PathBuf,
    id: String,
    activity: Option<Activity>,
    resumable: bool,
    subagents: usize,
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
        let mut prompted = None;
        self.following.poll(
            move |previous| session::follow(pid, &args, previous),
            |found| {
                self.held = found.map(|session| {
                    ledger.apply(&session.id, since, std::mem::take(&mut session.counted));
                    shown.clone_from(&session.context);
                    prompted = session.prompted;
                    Held {
                        path: session.path.clone(),
                        id: session.id.clone(),
                        activity: session.activity,
                        resumable: session.resumable,
                        subagents: session.subagents,
                    }
                });
                if self.held.is_none() {
                    *shown = None;
                }
            },
        );
        prompted
    }

    pub(in crate::agents) fn activity(&self) -> Option<Activity> {
        self.held.as_ref()?.activity
    }

    pub(in crate::agents) fn subagents(&self) -> usize {
        self.held.as_ref().map_or(0, |held| held.subagents)
    }

    pub(in crate::agents) fn record(&self) -> Option<Record> {
        self.held.as_ref().map(|held| Record::Copilot(held.path.clone()))
    }

    pub(in crate::agents) fn conversation(&self) -> Option<&str> {
        let held = self.held.as_ref()?;
        held.resumable.then_some(held.id.as_str())
    }
}

mod events;
pub(in crate::agents) mod message;
pub(in crate::agents) mod session;

#[cfg(test)]
pub(crate) mod fake;

#[cfg(test)]
mod tests;
