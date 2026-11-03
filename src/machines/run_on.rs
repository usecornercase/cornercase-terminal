use ratatui::style::{Color, Style};
use ratatui::text::Span;

use super::HERE;
use crate::text::truncate_right;

pub(crate) const NEEDS_REMOTE: &str = "needs a git remote";
const GAP: &str = "   ";
const SHOWN: usize = 3;
const TEXT: usize = 60;

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
    pub(crate) cloning: Option<u64>,
    pub(crate) lost: Option<String>,
}

impl RunOn {
    pub(crate) fn can_choose(&self, at: usize) -> bool {
        self.blocked.is_none() && self.targets.get(at).is_some_and(Target::enabled)
    }

    pub(crate) fn choose(&mut self, at: Option<usize>) -> bool {
        if at.is_some_and(|at| !self.can_choose(at)) {
            return false;
        }
        if !self.is_chosen(at) {
            self.cloning = None;
        }
        self.chosen = at;
        self.lost = None;
        true
    }

    pub(crate) fn elsewhere(&self) -> bool {
        self.chosen.is_some() || self.lost.is_some()
    }

    pub(crate) fn is_chosen(&self, at: Option<usize>) -> bool {
        self.chosen == at && self.lost.is_none()
    }

    pub(crate) fn kept(fresh: Option<Self>, shown: Option<&Self>) -> Option<Self> {
        let Some(shown) = shown else { return fresh };
        let listed = fresh.is_some();
        let mut fresh = fresh.unwrap_or_default();
        fresh.awaiting = shown.awaiting;
        fresh.cloning = shown.cloning;
        fresh.lost.clone_from(&shown.lost);
        if let Some(target) = shown.machine() {
            fresh.chosen = fresh.targets.iter().position(|there| there.link == target.link);
            if fresh.chosen.is_none() {
                fresh.lost = Some(fresh.lost_why(&target.name));
            }
        }
        (listed || fresh.lost.is_some()).then_some(fresh)
    }

    fn lost_why(&self, name: &str) -> String {
        if self.targets.iter().any(|target| target.name.eq_ignore_ascii_case(name)) {
            format!("{name} changed in Settings; choose it again")
        } else {
            format!("{name} is no longer listed")
        }
    }

    pub(crate) fn machine(&self) -> Option<&Target> {
        self.targets.get(self.chosen?)
    }

    pub(crate) fn spans(&self, dim: Style) -> Vec<Span<'static>> {
        let mark = |chosen: bool| if chosen { '●' } else { '○' };
        let here = self.chosen.is_none() && self.lost.is_none();
        let mut spans = vec![Span::raw(format!("{} {HERE}", mark(here)))];
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
        if let Some(lost) = &self.lost {
            spans.push(Span::styled(format!(" · {lost}"), Style::default().fg(Color::Red)));
        }
        spans
    }
}

pub(crate) fn from_template(name: &str) -> String {
    format!("from {name}'s template (the issue's URL by default)")
}

pub(crate) fn not_there(name: &str) -> String {
    format!("the project is not on {name}")
}

pub(crate) fn clipped(text: &str) -> String {
    let line = text.split(char::is_control).filter(|part| !part.trim().is_empty());
    truncate_right(&line.map(str::trim).collect::<Vec<_>>().join(" "), TEXT)
}

pub(crate) fn capped(items: impl Iterator<Item = String>) -> String {
    let items: Vec<String> = items.map(|item| clipped(&item)).filter(|item| !item.is_empty()).collect();
    let more = items.len().saturating_sub(SHOWN);
    let shown = &items[..items.len().min(SHOWN)];
    match more {
        0 => shown.join(", "),
        more => format!("{} … and {more} more", shown.join(", ")),
    }
}

#[cfg(test)]
mod tests;
