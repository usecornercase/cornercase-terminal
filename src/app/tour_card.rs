use std::path::PathBuf;
use std::time::Instant;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::overlay::Overlay;
use super::toast::Toast;
use crate::editor::Editor;
use crate::git;
use crate::log;
use crate::project::Hierarchy;
use crate::tour::{self, Dialog, Done, Facts, Offer, Saved, Show, Step, TOURS, Tour};
use crate::ui::{self, TourButton};

mod prompt;
mod sections;

pub(in crate::app) const CLIENTS: u64 = 1 << 60;

pub(super) const ANNOUNCE: &str = "new: a guided tour, in Help";

#[derive(Default)]
pub(super) struct TourRuntime {
    saved: Saved,
    path: Option<PathBuf>,
    first_run: bool,
    found: bool,
    active: Option<Active>,
    clients: u64,
    asking: Option<u64>,
    swallowed: bool,
}

struct Active {
    tour: usize,
    step: usize,
    start: Facts,
    card: bool,
    field: Option<Editor>,
    focused: bool,
    idea: usize,
    note: Option<String>,
    opened: Option<Show>,
    agent: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Move {
    Next,
    Back,
    Skip,
}

impl Active {
    fn tour(&self) -> &'static Tour {
        &TOURS[self.tour]
    }

    fn step(&self) -> &'static Step {
        &self.tour().steps[self.step]
    }
}

impl TourRuntime {
    pub(super) fn saved(&self) -> &Saved {
        &self.saved
    }

    pub(super) fn current(&self) -> Option<(usize, usize, bool)> {
        self.active.as_ref().map(|a| (a.tour, a.step, a.card))
    }

    pub(super) fn show_card(&mut self, card: bool) {
        if let Some(active) = &mut self.active {
            active.card = card;
        }
    }

    fn offers(&self) -> bool {
        self.path.is_some() && !self.saved.offered
    }
}

impl App {
    pub(crate) fn set_tour(&mut self, path: PathBuf, first_run: bool) {
        let tour = &mut self.messages.tour;
        let found = tour::load(&path);
        tour.found = found.is_some();
        tour.saved = found.unwrap_or_default();
        tour.path = Some(path);
        tour.first_run = first_run;
    }

    fn tour_dialog(&self) -> Option<Dialog> {
        Some(match self.overlay.as_ref()? {
            Overlay::Issues(_) => Dialog::Issues,
            Overlay::Search(_) => Dialog::Search,
            Overlay::Settings(_) => Dialog::Settings,
            Overlay::Flows { .. } => Dialog::Flows,
            Overlay::Usage => Dialog::Usage,
            Overlay::Remote => Dialog::Remote,
            Overlay::Keys(_) => Dialog::Keys,
            Overlay::Queue(_) => Dialog::Queue,
            _ => return None,
        })
    }

    pub(super) fn tour_facts(&self) -> Facts {
        let open = || self.projects.iter().filter(|p| !p.closing);
        Facts {
            projects: open().count(),
            workspaces: open().map(|p| p.workspaces.len()).sum(),
            tabs: self.projects.tabs().count(),
            panes: self.projects.terms().count(),
            agents: self.projects.terms().filter(|t| t.agent.status().is_some()).count(),
            prompted: self.projects.terms().filter_map(|t| t.context.prompted()).max(),
            panel: self.tour_panel(),
            dialog: self.tour_dialog(),
            todos: self.panels.todos.items().len(),
            agents_section: self.config.agents_section,
            repository: false,
            compact: self.screen.drawn.compact(),
        }
    }

    pub(super) fn offer_tour(&mut self, now: Instant) {
        if self.messages.star.window_open(now) && self.messages.tour.offers() {
            self.first_offer();
        }
        self.watch_tour();
    }

    fn first_offer(&mut self) {
        let tour = &mut self.messages.tour;
        tour.saved.offered = true;
        let (first_run, found) = (tour.first_run, tour.found);
        tour.saved.announced |= first_run || !found;
        if first_run && self.config.tour {
            self.start_tour(0, None, true);
        } else if !first_run && !found {
            self.messages.toast = Some(Toast::new(ANNOUNCE, ui::ToastIcon::Check));
            log::info!("tour", "announced");
        }
        self.save_tour();
    }

    fn watch_tour(&mut self) {
        let Some(active) = &self.messages.tour.active else { return };
        let step = active.step();
        if step.done.met(&active.start, &self.tour_facts()) {
            log::info!("tour", "step done", tour = active.tour().id, step = step.id);
            self.complete_step();
            self.move_tour(Move::Skip);
        }
    }

    pub(super) fn tour_forward(&mut self) {
        self.complete_step();
        self.move_tour(Move::Next);
    }

    pub(super) fn complete_tour_step(&mut self, tour: &'static Tour, step: &str) {
        let current = self.messages.tour.active.as_ref().is_some_and(|a| a.tour().id == tour.id && a.step().id == step);
        if current {
            self.tour_forward();
        } else if self.messages.tour.saved.complete(tour, step) {
            self.save_tour();
        }
    }

    fn complete_step(&mut self) {
        let Some(active) = &self.messages.tour.active else { return };
        let (tour, step) = (active.tour(), active.step().id);
        if self.messages.tour.saved.complete(tour, step) {
            self.save_tour();
        }
    }

    pub(super) fn start_tour(&mut self, tour: usize, step: Option<usize>, card: bool) {
        let Some(t) = TOURS.get(tour) else { return };
        self.hide_section();
        let saved = &mut self.messages.tour.saved;
        let step = step.unwrap_or_else(|| saved.resume_at(t)).min(t.steps.len().saturating_sub(1));
        if saved.finished(t.id) {
            saved.tours.entry(t.id.to_string()).or_default().finished = false;
        }
        log::info!("tour", "started", tour = t.id, step = step);
        let start = self.tour_facts();
        let (opened, agent) = (None, None);
        let active =
            Active { tour, step, start, card, field: None, focused: false, idea: 0, note: None, opened, agent };
        self.messages.tour.active = Some(active);
        self.enter_step();
    }

    fn enter_step(&mut self) {
        let facts = self.tour_facts();
        let asks = self.messages.tour.active.as_ref().is_some_and(|a| a.step().offer == Some(Offer::Prompt));
        let agent = asks.then(|| self.tour_agent()).flatten();
        let Some(active) = &mut self.messages.tour.active else { return };
        active.start = facts;
        active.note = None;
        active.focused = false;
        active.agent = agent;
        active.field = asks.then(|| prompt::idea(active.idea));
        self.show_section();
    }

    pub(super) fn move_tour(&mut self, to: Move) {
        let Some(active) = &mut self.messages.tour.active else { return };
        let count = active.tour().steps.len();
        match to {
            Move::Back => active.step = active.step.saturating_sub(1),
            Move::Next | Move::Skip if active.step + 1 < count => active.step += 1,
            Move::Next | Move::Skip => return self.finish_tour(),
        }
        self.enter_step();
    }

    fn finish_tour(&mut self) {
        let Some(tour) = self.messages.tour.active.as_ref().map(Active::tour) else { return };
        self.messages.tour.saved.finish(tour);
        self.save_tour();
        log::info!("tour", "finished", tour = tour.id);
        self.hide_section();
        self.messages.tour.active = None;
        self.messages.toast =
            Some(Toast::new(format!("{} done: Help has more tours", tour.title), ui::ToastIcon::Check));
    }

    pub(super) fn end_tour(&mut self) {
        self.hide_section();
        if let Some(active) = self.messages.tour.active.take() {
            log::info!("tour", "ended", tour = active.tour().id, step = active.step().id);
        }
    }

    pub(super) fn tour_from_settings(&mut self, reset: bool) {
        if reset {
            self.reset_tour(None);
        } else {
            self.overlay = None;
            self.start_tour(0, None, true);
        }
    }

    pub(super) fn reset_tour(&mut self, tour: Option<&str>) {
        self.messages.tour.saved.reset(tour);
        self.save_tour();
    }

    fn save_tour(&self) {
        let Some(path) = &self.messages.tour.path else { return };
        if let Err(e) = tour::save(path, &self.messages.tour.saved) {
            log::warning!("tour", "failed to save", error = e);
        }
    }

    fn tour_offers(&self, active: &Active, facts: &Facts) -> (Vec<TourButton>, Option<String>, Option<String>) {
        let step = active.step();
        let prompt = step.offer == Some(Offer::Prompt);
        let agent = active.agent.clone();
        let project = self.project().is_some();
        let actions = match step.offer {
            Some(Offer::Prompt) if agent.is_some() && project => vec![TourButton::Idea, TourButton::Start],
            Some(Offer::AgentsSection) if !facts.agents_section => vec![TourButton::TurnOn],
            _ => Vec::new(),
        };
        let note = match () {
            () if prompt && agent.is_none() => Some(
                "No agent is installed here. Install Claude Code, Codex or another agent, then start it in a tab."
                    .into(),
            ),
            () if prompt && !project => Some("Open a project first.".into()),
            () => None,
        };
        (actions, note, agent)
    }

    fn tour_forward_button(active: &Active, facts: &Facts) -> TourButton {
        if active.step().done.asks(facts) {
            TourButton::Skip
        } else if active.step + 1 == active.tour().steps.len() {
            TourButton::Done
        } else {
            TourButton::Next
        }
    }

    pub(super) fn tour_card(&self, area: Rect) -> Option<ui::TourCard> {
        let active = self.messages.tour.active.as_ref().filter(|a| a.card)?;
        let (tour, step) = (active.tour(), active.step());
        let repository = step.done == Done::WorkspaceMade && self.project().is_some_and(|p| git::is_repo_root(&p.path));
        let facts = Facts { repository, ..self.tour_facts() };
        let (target, button) = self.tour_target(&self.layout(area).shown(self.screen.nav), step.target);
        let (actions, note, agent) = self.tour_offers(active, &facts);
        Some(ui::TourCard {
            title: format!("{} · {}/{}", tour.title, active.step + 1, tour.steps.len()),
            step: step.title.into(),
            text: step.text.into(),
            note: active.note.clone().or(note),
            field: active.field.clone().filter(|_| !actions.is_empty()),
            focused: active.focused,
            agent,
            actions,
            back: active.step > 0,
            forward: Self::tour_forward_button(active, &facts),
            target,
            button,
        })
    }

    pub(super) fn tour_shown(&self) -> bool {
        self.messages.tour.active.as_ref().is_some_and(|a| a.card)
    }

    fn swallow_tour_press(&mut self, kind: MouseEventKind) -> bool {
        if !self.messages.tour.swallowed {
            return false;
        }
        let ends = kind == MouseEventKind::Up(MouseButton::Left);
        let held = matches!(kind, MouseEventKind::Drag(MouseButton::Left));
        self.messages.tour.swallowed = held;
        ends || held
    }

    pub(super) fn tour_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> bool {
        if self.swallow_tour_press(ev.kind) {
            return true;
        }
        let Some(card) = self.tour_card(area) else { return false };
        let layout = ui::tour_layout(area, &card);
        if !layout.area.contains(pos) {
            if matches!(ev.kind, MouseEventKind::Down(_))
                && let Some(active) = &mut self.messages.tour.active
            {
                active.focused = false;
            }
            return false;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return true;
        }
        if layout.field.contains(pos) {
            self.focus_tour_field(layout.field, pos);
        } else {
            self.messages.tour.swallowed = true;
            if let Some(button) = ui::tour_hit(area, &card, pos) {
                self.tour_button(button, area);
            }
        }
        true
    }

    fn tour_button(&mut self, button: TourButton, area: Rect) {
        log::info!("tour", "button", button = format!("{button:?}").to_lowercase());
        match button {
            TourButton::Back => self.move_tour(Move::Back),
            TourButton::Skip => self.move_tour(Move::Skip),
            TourButton::Next | TourButton::Done => self.tour_forward(),
            TourButton::End => self.end_tour(),
            TourButton::Docs => self.copy_tour_docs(),
            TourButton::Idea => self.next_idea(),
            TourButton::TurnOn => self.turn_on_agents_section(),
            TourButton::Start => self.start_tour_prompt(area),
        }
    }

    fn copy_tour_docs(&mut self) {
        let Some(active) = &self.messages.tour.active else { return };
        let url = active.tour().docs_url();
        self.copy(&url);
        self.messages.toast = Some(Toast::new(format!("copied {url}"), ui::ToastIcon::Check));
    }

    fn turn_on_agents_section(&mut self) {
        let config = crate::config::Config { agents_section: true, ..self.config.clone() };
        if let Err(e) = self.write_config(&config) {
            self.messages.toast = Some(Toast::new(e, ui::ToastIcon::Bug));
        }
    }
}
