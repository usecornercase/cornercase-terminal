use crate::agents::activity::Activity;

pub(in crate::agents) const NAME: &str = "opencode";
pub(in crate::agents) const TITLE: &str = "opencode";
pub(in crate::agents) const MODES: crate::agents::ModeTable = &[];
pub(in crate::agents) const HOLDS_PROMPTS: bool = false;
pub(in crate::agents) const RUNS_AS: bool = false;
pub(in crate::agents) const LOCAL_COMMAND: bool = false;
pub(in crate::agents) const INPUT_BOX: Option<bool> = None;
pub(in crate::agents) const SURVEY: bool = false;
pub(in crate::agents) const USAGE: Option<crate::agents::usage::Spec> = None;
pub(in crate::agents) const RESUME: Option<Vec<String>> = None;
pub(in crate::agents) const UNFOUND: &str =
    "opencode has no conversation in its folder yet, or two opencode share that folder";

pub(in crate::agents) fn normalized(args: &[String]) -> Vec<String> {
    args.to_vec()
}
pub(crate) fn opencode(turn: bool) -> Activity {
    if turn { Activity::Working } else { Activity::Idle }
}

pub(in crate::agents) mod database;

#[cfg(test)]
pub(crate) mod fake;
