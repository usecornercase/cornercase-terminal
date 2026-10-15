use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::agents::activity::Activity;
use crate::agents::context::counting::Ledger;
use crate::agents::context::discovery::Job;
use crate::agents::context::{Context, Look, Record};

pub(in crate::agents) const NAME: &str = "cursor";
pub(in crate::agents) const TITLE: &str = "Cursor CLI";
pub(in crate::agents) const COMMAND: &str = "cursor-agent";
pub(in crate::agents) const MODEL_FLAG: &str = "--model";
pub(in crate::agents) const MODEL_ALIAS: Option<&str> = None;
pub(in crate::agents) const EFFORT: Option<crate::agents::Effort> = None;
pub(in crate::agents) const UNATTENDED: &[&str] = &["--force", "--trust"];
pub(in crate::agents) const STATUS_BY_FOLDER: bool = false;
pub(in crate::agents) const RECORD_BY_FOLDER: bool = false;
pub(in crate::agents) const MODELS: &[&str] = &[];
pub(in crate::agents) const CATALOGUE: Option<fn() -> Vec<String>> = None;
pub(in crate::agents) const MODES: crate::agents::ModeTable = &[
    ("plan", &["--mode", "plan"]),
    ("ask", &["--mode", "ask"]),
    ("auto review", &["--auto-review"]),
    ("run everything (dangerous)", &["--force"]),
];
pub(in crate::agents) const HOLDS_PROMPTS: bool = true;
pub(in crate::agents) const INPUT_BOX: Option<bool> = None;
pub(in crate::agents) const SURVEY: bool = false;
pub(in crate::agents) const USAGE: Option<crate::agents::usage::Spec> = None;
pub(in crate::agents) const UNFOUND: &str = "Cursor CLI has no conversation for this pane yet";

#[expect(clippy::unnecessary_wraps, reason = "Known dispatch shares an optional resume signature across agents")]
pub(in crate::agents) fn resume(args: Vec<String>, conversation: String) -> Option<Vec<String>> {
    Some([args, vec!["--resume".into(), conversation]].concat())
}

pub(in crate::agents) fn normalized(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for arg in args {
        match arg.as_str() {
            "-f" | "--yolo" | "--force" => out.push("--force".into()),
            "--plan" => out.extend(["--mode".into(), "plan".into()]),
            _ => match arg.strip_prefix("--mode=") {
                Some(mode) => out.extend(["--mode".into(), mode.to_string()]),
                None => out.push(arg.clone()),
            },
        }
    }
    out
}

pub(in crate::agents) fn local_command(text: &str) -> bool {
    text.trim_start().starts_with(['/', '!'])
}

pub(in crate::agents) fn runs_as(arg: &str) -> bool {
    let mut parts = Path::new(arg).components().rev().filter_map(|c| c.as_os_str().to_str());
    let (file, _version, versions, install) = (parts.next(), parts.next(), parts.next(), parts.next());
    file == Some("index.js") && versions == Some("versions") && install == Some("cursor-agent")
}

const WAITING: [char; 2] = ['❓', '🔐'];
const WORKING: [char; 5] = ['⏳', '🧭', '⌨', '📋', '📝'];

fn indicated(title: &str) -> Option<Activity> {
    let (_, state) = title.rsplit_once(" - ")?;
    let sign = state.trim_start().chars().next()?;
    if WAITING.contains(&sign) {
        Some(Activity::Waiting)
    } else if WORKING.contains(&sign) {
        Some(Activity::Working)
    } else if sign == '✅' {
        Some(Activity::Idle)
    } else {
        None
    }
}

#[derive(Debug)]
struct Held {
    path: PathBuf,
    id: String,
    turn_open: bool,
    resumable: bool,
}

#[derive(Debug, Default)]
pub(in crate::agents) struct Reader {
    session: Option<Box<store::Session>>,
    held: Option<Held>,
    reading: Job<Option<store::Session>>,
}

impl Reader {
    pub(in crate::agents) fn update(
        &mut self,
        look: &Look<'_>,
        since: SystemTime,
        shown: &mut Option<Context>,
        ledger: &mut Ledger,
    ) -> Option<SystemTime> {
        let mut prompted = None;
        if let Some(found) = self.reading.take() {
            if let Some(mut session) = found {
                ledger.apply(&session.id, since, std::mem::take(&mut session.counted));
                shown.clone_from(&session.context);
                prompted = session.prompted;
                self.held = Some(Held {
                    path: session.path.clone(),
                    id: session.id.clone(),
                    turn_open: session.turn_open,
                    resumable: session.resumable,
                });
                self.session = Some(Box::new(session));
            } else {
                *shown = None;
                self.session = None;
                self.held = None;
            }
        }
        if !self.reading.running() {
            let pid = look.pid;
            let previous = self.session.take().map(|session| *session);
            self.reading.start(move || store::follow(pid, since, previous));
        }
        prompted
    }

    pub(in crate::agents) fn activity(&self, title: &str) -> Activity {
        let open = self.held.as_ref().is_some_and(|held| held.turn_open);
        indicated(title).unwrap_or(if open { Activity::Working } else { Activity::Idle })
    }

    pub(in crate::agents) fn record(&self) -> Option<Record> {
        self.held.as_ref().map(|held| Record::Cursor(held.path.clone()))
    }

    pub(in crate::agents) fn conversation(&self) -> Option<&str> {
        let held = self.held.as_ref()?;
        held.resumable.then_some(held.id.as_str())
    }
}

mod proto;
pub(in crate::agents) mod store;

#[cfg(test)]
pub(crate) mod fake;

#[cfg(test)]
mod tests;
