use std::time::Duration;

const FAILED: &str = "✗";
pub(crate) const MAX_FOLLOWED: usize = 20;
pub(crate) const BATCH: Duration = Duration::from_secs(30);
pub(crate) const BODY_CAP: usize = 2 * 1024;
pub(crate) const REPORTS_CAP: usize = 16 * 1024;
pub(crate) const RUN_STILL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Said<'a> {
    Message(&'a str),
    Screen(&'a str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Standing {
    pub(crate) ended: bool,
    pub(crate) still: Option<Duration>,
}

pub(crate) fn orchestrator_over(settled: bool, standing: Standing) -> bool {
    (settled && standing.ended) || standing.still.is_some_and(|still| still >= RUN_STILL)
}

pub(crate) fn started(agent: &str, place: &str) -> String {
    format!("▶ started {agent} in {place}")
}

pub(crate) fn finished(agent: &str, place: &str, failed: bool) -> String {
    if failed { format!("{FAILED} {agent} failed in {place}") } else { format!("✓ {agent} finished in {place}") }
}

pub(crate) fn failed(text: &str) -> bool {
    text.starts_with(FAILED)
}

pub(crate) fn waiting(agent: &str, place: &str) -> String {
    format!("! {agent} needs you in {place}")
}

pub(crate) fn dropped(agent: &str, place: &str) -> String {
    format!("no longer following {agent} in {place}: a bot follows at most {MAX_FOLLOWED} agents")
}

pub(crate) fn body(text: &str) -> String {
    let text = text.trim();
    let kept = crate::text::within_bytes(text, BODY_CAP);
    if kept.len() == text.len() { text.to_string() } else { format!("{}\n… (cut)", kept.trim_end()) }
}

pub(crate) fn report(place: &str, pane: u64, failed: bool, said: Said) -> String {
    let verb = if failed { "failed" } else { "finished" };
    let head = format!("The agent you started in {place} (pane {pane}) {verb}.");
    match said {
        Said::Message(text) => format!("{head} Its last message:\n\n{}", body(text)),
        Said::Screen(text) => format!("{head} Its screen ends with:\n\n{}", body(text)),
    }
}

pub(crate) fn batch(texts: &[String]) -> usize {
    let mut size = 0;
    let mut taken = 0;
    for text in texts {
        if taken > 0 && size + text.len() > REPORTS_CAP {
            break;
        }
        size += text.len();
        taken += 1;
    }
    taken
}

#[cfg(test)]
mod tests;
