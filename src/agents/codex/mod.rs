use crate::agents::activity::{Activity, BRAILLE, glyph};

const ACTION_REQUIRED: [&str; 2] = ["[ ! ] Action Required", "[ . ] Action Required"];

pub(in crate::agents) const NAME: &str = "codex";
pub(in crate::agents) const TITLE: &str = "Codex";
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

pub(crate) fn codex(title: &str, turn: bool) -> Activity {
    if ACTION_REQUIRED.iter().any(|prefix| title.starts_with(prefix)) {
        Activity::Waiting
    } else if turn || glyph(title).is_some_and(|g| BRAILLE.contains(&g)) {
        Activity::Working
    } else {
        Activity::Idle
    }
}

pub(in crate::agents) mod message;
pub(in crate::agents) mod rollout;
pub(crate) mod usage;

#[cfg(test)]
pub(crate) mod fake;

#[cfg(test)]
mod tests;
