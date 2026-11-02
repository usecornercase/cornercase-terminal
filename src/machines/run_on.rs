use ratatui::style::Style;
use ratatui::text::Span;

use super::HERE;

pub(crate) const NEEDS_REMOTE: &str = "needs a git remote";
const GAP: &str = "   ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) name: String,
    pub(crate) icon: char,
    pub(crate) link: u64,
    pub(crate) why: Option<String>,
}

impl Target {
    pub(crate) fn enabled(&self) -> bool {
        self.why.is_none()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RunOn {
    pub(crate) targets: Vec<Target>,
    pub(crate) chosen: Option<usize>,
    pub(crate) blocked: Option<&'static str>,
    pub(crate) awaiting: Option<u64>,
}

impl RunOn {
    pub(crate) fn can_choose(&self, at: usize) -> bool {
        self.blocked.is_none() && self.targets.get(at).is_some_and(Target::enabled)
    }

    pub(crate) fn choose(&mut self, at: Option<usize>) -> bool {
        if at.is_some_and(|at| !self.can_choose(at)) {
            return false;
        }
        self.chosen = at;
        true
    }

    pub(crate) fn machine(&self) -> Option<&Target> {
        self.targets.get(self.chosen?)
    }

    pub(crate) fn spans(&self, dim: Style) -> Vec<Span<'static>> {
        let mark = |chosen: bool| if chosen { '●' } else { '○' };
        let mut spans = vec![Span::raw(format!("{} {HERE}", mark(self.chosen.is_none())))];
        for (at, target) in self.targets.iter().enumerate() {
            let text = format!("{GAP}{} {} {}", mark(self.chosen == Some(at)), target.icon, target.name);
            match (&target.why, self.blocked) {
                (Some(why), None) => spans.push(Span::styled(format!("{text} · {why}"), dim)),
                (_, Some(_)) => spans.push(Span::styled(text, dim)),
                (None, None) => spans.push(Span::raw(text)),
            }
        }
        if let Some(blocked) = self.blocked {
            spans.push(Span::styled(format!(" — {blocked}"), dim));
        }
        spans
    }
}

#[cfg(test)]
mod tests;
