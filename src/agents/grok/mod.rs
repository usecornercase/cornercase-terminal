use std::path::PathBuf;
use std::time::SystemTime;

use crate::agents::activity::Activity;
use crate::agents::context::counting::Ledger;
use crate::agents::context::discovery::Job;
use crate::agents::context::{Context, Look, Record};

pub(in crate::agents) const NAME: &str = "grok";
pub(in crate::agents) const TITLE: &str = "Grok Build";
pub(in crate::agents) const MODES: crate::agents::ModeTable = &[
    ("accept edits", &["--permission-mode", "acceptEdits"]),
    ("auto", &["--permission-mode", "auto"]),
    ("always approve (dangerous)", &["--always-approve"]),
];
pub(in crate::agents) const HOLDS_PROMPTS: bool = true;
pub(in crate::agents) const RUNS_AS: bool = false;
pub(in crate::agents) const INPUT_BOX: Option<bool> = None;
pub(in crate::agents) const SURVEY: bool = false;
pub(in crate::agents) const USAGE: Option<crate::agents::usage::Spec> = None;
pub(in crate::agents) const UNFOUND: &str = "Grok Build has no conversation for this pane yet";

#[expect(clippy::unnecessary_wraps, reason = "Known dispatch shares an optional resume signature across agents")]
pub(in crate::agents) fn resume(args: Vec<String>, conversation: String) -> Option<Vec<String>> {
    Some([args, vec!["--resume".into(), conversation]].concat())
}

pub(in crate::agents) fn normalized(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--yolo" || arg == "--always-approve" {
            out.push("--always-approve".into());
        } else if let Some(value) = arg.strip_prefix("--permission-mode=") {
            push_permission(&mut out, value);
        } else if arg == "--permission-mode" {
            match args.next() {
                Some(value) => push_permission(&mut out, value),
                None => out.push(arg.clone()),
            }
        } else {
            out.push(arg.clone());
        }
    }
    out
}

fn push_permission(out: &mut Vec<String>, value: &str) {
    if value == "bypassPermissions" {
        out.push("--always-approve".into());
    } else {
        out.push("--permission-mode".into());
        out.push(value.to_string());
    }
}

pub(in crate::agents) fn local_command(text: &str) -> bool {
    text.trim_start().starts_with(['/', '!'])
}

#[derive(Debug)]
struct Held {
    path: PathBuf,
    id: String,
    activity: Activity,
    turn_open: bool,
    resumable: bool,
}

#[derive(Debug, Default)]
pub(in crate::agents) struct Reader {
    session: Option<Box<session::Session>>,
    held: Option<Held>,
    reading: Job<Option<session::Session>>,
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
                for (id, records) in std::mem::take(&mut session.extras) {
                    ledger.apply(&id, since, records);
                }
                shown.clone_from(&session.context);
                prompted = session.prompted;
                self.held = Some(Held {
                    path: session.path.clone(),
                    id: session.id.clone(),
                    activity: session.activity,
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
            self.reading.start(move || session::follow(pid, previous));
        }
        prompted
    }

    pub(in crate::agents) fn activity(&self, title: &str) -> Option<Activity> {
        let held = self.held.as_ref()?;
        if held.turn_open && title.contains("Action Required") { Some(Activity::Waiting) } else { Some(held.activity) }
    }

    pub(in crate::agents) fn record(&self) -> Option<Record> {
        self.held.as_ref().map(|held| Record::Grok(held.path.clone()))
    }

    pub(in crate::agents) fn conversation(&self) -> Option<&str> {
        let held = self.held.as_ref()?;
        held.resumable.then_some(held.id.as_str())
    }
}

pub(in crate::agents) mod session;

#[cfg(test)]
pub(crate) mod fake;

#[cfg(test)]
mod tests;
