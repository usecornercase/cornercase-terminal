use crate::agents::activity::{Activity, glyph};

pub(in crate::agents) const NAME: &str = "gemini";
pub(in crate::agents) const TITLE: &str = "Gemini CLI";
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

pub(crate) fn gemini(title: &str) -> Option<Activity> {
    match glyph(title.trim())? {
        '✋' => Some(Activity::Waiting),
        '✦' | '⏲' => Some(Activity::Working),
        '◇' => Some(Activity::Idle),
        _ => None,
    }
}

pub(in crate::agents) mod session;

#[cfg(test)]
pub(crate) mod fake;

#[cfg(test)]
mod tests;
