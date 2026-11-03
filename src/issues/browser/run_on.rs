use ratatui::style::{Color, Style};
use ratatui::text::Span;

use super::{Action, AgentNote, Browser, Choice, Screen, There};
use crate::issues::Issue;
use crate::machines::run_on::Target;
use crate::machines::{HERE, HERE_ICON};

impl Browser {
    pub(crate) fn machine(&self) -> Option<&Target> {
        self.run_on.as_ref()?.machine()
    }

    pub(crate) fn there(&self) -> Option<&There> {
        self.there.as_ref().filter(|_| self.machine().is_some())
    }

    pub(crate) fn focused(&self) -> Option<&Issue> {
        match &self.screen {
            Screen::Detail { issue, .. } => Some(issue),
            Screen::List => self.selected_issue(),
        }
    }

    pub(super) fn elsewhere(&self) -> bool {
        self.run_on.as_ref().is_some_and(|run_on| run_on.chosen.is_some() || run_on.lost.is_some())
    }

    pub(super) fn lost_there(&mut self) -> Option<Action> {
        self.error = Some(self.run_on.as_ref()?.lost.clone()?);
        Some(Action::None)
    }

    pub(crate) fn starting_there(&self) -> Option<String> {
        let target = self.machine().filter(|_| self.starting)?;
        Some(format!("starting on {}…", target.name))
    }

    pub(super) fn not_cloned(&self) -> bool {
        self.there().is_some_and(|there| !there.cloned)
    }

    pub(super) fn needs_login_tab(&self) -> bool {
        self.there().is_some_and(|there| there.cloned && there.agent != AgentNote::Ready)
    }

    pub(super) fn chosen_machine(&self) -> Option<usize> {
        self.run_on.as_ref().map(|run_on| run_on.chosen.map_or(0, |at| at + 1))
    }

    pub(super) fn machine_choices(&self) -> Vec<Choice> {
        let Some(run_on) = &self.run_on else { return Vec::new() };
        let here = Choice { key: HERE.into(), title: HERE_ICON.to_string(), meta: String::new(), who: None };
        let machines = run_on.targets.iter().map(|target| Choice {
            key: target.name.clone(),
            title: target.icon.to_string(),
            meta: run_on.blocked.map(str::to_string).or_else(|| target.why.clone()).unwrap_or_default(),
            who: None,
        });
        std::iter::once(here).chain(machines).collect()
    }

    pub(super) fn choose_machine(&mut self, key: &str) -> Action {
        let Some(run_on) = &mut self.run_on else { return Action::None };
        let at = run_on.targets.iter().position(|target| target.name == key);
        if (at.is_none() && key != HERE) || !run_on.choose(at) {
            return Action::None;
        }
        self.picker = None;
        self.there = None;
        self.error = None;
        Action::RunOn(at)
    }

    pub(super) fn start_hint_there(&self, issue: &Issue) -> String {
        match (self.machine(), self.there()) {
            (Some(_), Some(there)) => there.start_hint.clone(),
            (Some(target), None) => format!("on {}", target.name),
            (None, _) => self.start_hint(issue),
        }
    }

    pub(super) fn run_on_spans(&self, dim: Style) -> Option<Vec<Span<'static>>> {
        let mut spans = self.run_on.as_ref()?.spans(dim);
        if let Some(missing) = self.there().and_then(|there| there.tracker.clone()) {
            spans.push(Span::styled(format!(" · {missing}"), Style::default().fg(Color::Red)));
        }
        Some(spans)
    }

    pub(super) fn agent_note(&self) -> Option<Span<'static>> {
        let name = &self.machine()?.name;
        let note = match self.there()?.agent {
            AgentNote::Ready => return None,
            AgentNote::NotInstalled => format!("  not installed on {name}"),
            AgentNote::SignedOut => format!("  not logged in on {name}"),
        };
        Some(Span::styled(note, Style::default().fg(Color::Yellow)))
    }

    pub(super) fn command_there(&self) -> Option<String> {
        self.there()?.command.clone()
    }

    pub(super) fn accounts_note(&self, dim: Style) -> Option<Span<'static>> {
        let name = &self.machine()?.name;
        Some(Span::styled(format!("  ({name}'s accounts)"), dim))
    }
}
