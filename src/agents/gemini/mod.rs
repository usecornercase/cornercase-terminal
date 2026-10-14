use std::path::PathBuf;
use std::time::SystemTime;

use crate::agents::activity::{Activity, glyph};
use crate::agents::context::counting::Ledger;
use crate::agents::context::discovery::Job;
use crate::agents::context::{Context, Look, Record};

pub(in crate::agents) const NAME: &str = "gemini";
pub(in crate::agents) const TITLE: &str = "Gemini CLI";
pub(in crate::agents) const MODEL_FLAG: &str = "--model";
pub(in crate::agents) const UNATTENDED: &[&str] = &["--yolo", "--skip-trust"];
pub(in crate::agents) const MODELS: &[&str] = &[];
pub(in crate::agents) const CATALOGUE: Option<fn() -> Vec<String>> = None;
pub(in crate::agents) const MODES: crate::agents::ModeTable = &[
    ("auto edit", &["--approval-mode", "auto_edit"]),
    ("plan", &["--approval-mode", "plan"]),
    ("yolo (dangerous)", &["--yolo"]),
];
pub(in crate::agents) const HOLDS_PROMPTS: bool = true;
pub(in crate::agents) const RUNS_AS: bool = false;
pub(in crate::agents) const INPUT_BOX: Option<bool> = None;
pub(in crate::agents) const SURVEY: bool = false;
pub(in crate::agents) const USAGE: Option<crate::agents::usage::Spec> = None;
pub(in crate::agents) const UNFOUND: &str =
    "Gemini has no conversation in its folder yet, or two Gemini share that home and folder";

#[expect(clippy::unnecessary_wraps, reason = "Known dispatch shares an optional resume signature across agents")]
pub(in crate::agents) fn resume(args: Vec<String>, conversation: String) -> Option<Vec<String>> {
    Some([args, vec!["--resume".into(), conversation]].concat())
}

pub(in crate::agents) fn normalized(args: &[String]) -> Vec<String> {
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

pub(in crate::agents) fn local_command(text: &str) -> bool {
    text.trim_start().starts_with(['/', '?', '!'])
}

fn from_title(title: &str) -> Option<Activity> {
    match glyph(title.trim())? {
        '✋' => Some(Activity::Waiting),
        '✦' | '⏲' => Some(Activity::Working),
        '◇' => Some(Activity::Idle),
        _ => None,
    }
}

#[derive(Debug, Default)]
pub(in crate::agents) struct Reader {
    session: Option<session::Session>,
    reading: Job<Option<session::Session>>,
    record: Option<(PathBuf, Option<String>)>,
}

impl Reader {
    pub(in crate::agents) fn update(
        &mut self,
        look: &Look<'_>,
        since: SystemTime,
        shown: &mut Option<Context>,
        ledger: &mut Ledger,
    ) -> Option<SystemTime> {
        let screen = (look.screen)();
        let mut prompted = None;
        if let Some(mut found) = self.reading.take() {
            if let Some(session) = &mut found {
                ledger.apply(&session.path.to_string_lossy(), since, std::mem::take(&mut session.counted));
            }
            *shown = found.as_ref().and_then(|session| session.context.clone());
            prompted = found.as_ref().and_then(|session| session.prompted);
            self.record = found.as_ref().map(|s| (s.path.clone(), s.id().map(str::to_string)));
            self.session = found;
        }
        if !self.reading.running() {
            let pid = look.pid;
            let previous = self.session.take();
            let footer = session::footer_model(&screen);
            self.reading.start(move || session::look(pid, since, previous, footer));
        }
        prompted
    }

    pub(in crate::agents) fn activity(title: &str) -> Option<Activity> {
        from_title(title)
    }

    pub(in crate::agents) fn record(&self) -> Option<Record> {
        Some(Record::Gemini(self.record.as_ref()?.0.clone()))
    }

    pub(in crate::agents) fn conversation(&self) -> Option<&str> {
        self.record.as_ref()?.1.as_deref()
    }
}

pub(in crate::agents) mod session;

#[cfg(test)]
pub(crate) mod fake;

#[cfg(test)]
mod tests;
