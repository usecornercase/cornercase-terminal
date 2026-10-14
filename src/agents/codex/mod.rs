use std::path::PathBuf;
use std::time::SystemTime;

use crate::agents::activity::{Activity, BRAILLE, glyph};
use crate::agents::context::counting::Ledger;
use crate::agents::context::discovery::Job;
use crate::agents::context::{Context, Look, Record};

const ACTION_REQUIRED: [&str; 2] = ["[ ! ] Action Required", "[ . ] Action Required"];

pub(in crate::agents) const NAME: &str = "codex";
pub(in crate::agents) const TITLE: &str = "Codex";
pub(in crate::agents) const MODEL_FLAG: &str = "--model";
pub(in crate::agents) const MODEL_ALIAS: Option<&str> = Some("-m");
pub(in crate::agents) const EFFORT: Option<crate::agents::Effort> = Some(crate::agents::Effort {
    levels: &["minimal", "low", "medium", "high", "xhigh"],
    flags: &[],
    config: Some("model_reasoning_effort"),
});
pub(in crate::agents) const UNATTENDED: &[&str] = &["--dangerously-bypass-approvals-and-sandbox"];
pub(in crate::agents) const STATUS_BY_FOLDER: bool = false;
pub(in crate::agents) const RECORD_BY_FOLDER: bool = true;
pub(in crate::agents) const MODELS: &[&str] = &[];
pub(in crate::agents) const CATALOGUE: Option<fn() -> Vec<String>> = None;
pub(in crate::agents) const MODES: crate::agents::ModeTable = &[
    ("read only", &["--sandbox", "read-only"]),
    ("workspace write", &["--sandbox", "workspace-write"]),
    ("no sandbox, no approvals (dangerous)", &["--dangerously-bypass-approvals-and-sandbox"]),
];
pub(in crate::agents) const HOLDS_PROMPTS: bool = true;
pub(in crate::agents) const LOCAL_COMMAND: bool = false;
pub(in crate::agents) const INPUT_BOX: Option<bool> = None;
pub(in crate::agents) const SURVEY: bool = false;
pub(in crate::agents) const USAGE: Option<crate::agents::usage::Spec> = Some(crate::agents::usage::Spec {
    command: usage::command,
    requests: usage::requests,
    answer: usage::answer,
    no_limits: "no plan limits for this account",
});
pub(in crate::agents) const UNFOUND: &str =
    "Codex writes its rollout from its first turn on, and two Codex in one folder hide each other";

pub(in crate::agents) fn normalized(args: &[String]) -> Vec<String> {
    args.to_vec()
}

#[expect(clippy::unnecessary_wraps, reason = "Known dispatch shares an optional resume signature across agents")]
pub(in crate::agents) fn resume(args: Vec<String>, conversation: String) -> Option<Vec<String>> {
    Some([vec!["resume".into()], args, vec![conversation]].concat())
}

const NPM_SCRIPT: &str = "/codex/bin/codex.js";

pub(in crate::agents) fn runs_as(arg: &str) -> bool {
    arg.ends_with(NPM_SCRIPT)
}

fn activity(title: &str, turn: bool) -> Activity {
    if ACTION_REQUIRED.iter().any(|prefix| title.starts_with(prefix)) {
        Activity::Waiting
    } else if turn || glyph(title).is_some_and(|g| BRAILLE.contains(&g)) {
        Activity::Working
    } else {
        Activity::Idle
    }
}

#[derive(Debug, Default)]
pub(in crate::agents) struct Reader {
    rollout: Option<rollout::Rollout>,
    finding: Job<Option<PathBuf>>,
}

impl Reader {
    pub(in crate::agents) fn update(
        &mut self,
        look: &Look<'_>,
        since: SystemTime,
        shown: &mut Option<Context>,
        ledger: &mut Ledger,
    ) -> Option<SystemTime> {
        if let Some(found) = self.finding.take()
            && found.as_ref() != self.rollout.as_ref().map(|r| &r.path)
        {
            self.rollout = found.map(rollout::Rollout::new);
            *shown = None;
        }
        let pid = look.pid;
        self.finding.start(move || rollout::rollout_path(pid, since));
        if let Some(rollout) = &mut self.rollout {
            rollout.update();
            *shown = rollout.context();
            ledger.apply(&rollout.path.to_string_lossy(), since, std::mem::take(&mut rollout.counted));
            return rollout.prompted();
        }
        None
    }

    pub(in crate::agents) fn activity(&self, title: &str) -> Activity {
        activity(title, self.rollout.as_ref().is_some_and(rollout::Rollout::turn))
    }

    pub(in crate::agents) fn record(&self) -> Option<Record> {
        Some(Record::Codex(self.rollout.as_ref()?.path.clone()))
    }

    pub(in crate::agents) fn conversation(&self) -> Option<&str> {
        self.rollout.as_ref()?.id()
    }
}

pub(in crate::agents) mod message;
pub(in crate::agents) mod rollout;
pub(crate) mod usage;

#[cfg(test)]
pub(crate) mod fake;

#[cfg(test)]
mod tests;
