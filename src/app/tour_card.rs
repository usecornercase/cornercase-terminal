use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use serde_json::Value;

use super::App;
use super::layout::Side;
use super::overlay::Overlay;
use super::toast::Toast;
use crate::agents;
use crate::control::{self, Command, Request};
use crate::log;
use crate::project::Hierarchy;
use crate::settings::Page;
use crate::todo::editor::Editor;
use crate::tour::{self, Dialog, Facts, Offer, Panel, Saved, TOURS, Tour};
use crate::ui::{self, TourButton};

pub(in crate::app) const CLIENTS: u64 = 1 << 60;

const WINDOW_GAP: Duration = Duration::from_secs(2);

const IDEAS: [&str; 3] = [
    "Explain this repository's structure: what each top-level folder holds and where to start reading.",
    "Find one thing in this code worth improving, and say why. Do not change anything yet.",
    "List the commands to build, test and run this project, and check that they work.",
];

pub(super) const ANNOUNCE: &str = "new: a guided tour, in Help";

#[derive(Default)]
pub(super) struct TourRuntime {
    pub(super) saved: Saved,
    pub(super) path: Option<PathBuf>,
    pub(super) first_run: bool,
    pub(super) found: bool,
    pub(super) active: Option<Active>,
    pub(super) clients: u64,
    pub(super) asking: Option<u64>,
    pub(super) swallowed: bool,
}

pub(super) struct Active {
    pub(super) tour: usize,
    pub(super) step: usize,
    pub(super) start: Facts,
    pub(super) card: bool,
    pub(super) field: Option<Editor>,
    pub(super) focused: bool,
    pub(super) idea: usize,
    pub(super) note: Option<String>,
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

    fn step(&self) -> &'static tour::Step {
        &self.tour().steps[self.step]
    }
}

impl App {
    pub(crate) fn set_tour(&mut self, path: PathBuf, first_run: bool) {
        let found = tour::load(&path);
        self.tour.found = found.is_some();
        self.tour.saved = found.unwrap_or_default();
        self.tour.path = Some(path);
        self.tour.first_run = first_run;
    }

    pub(super) fn tour_facts(&self) -> Facts {
        let open = || self.projects.iter().filter(|p| !p.closing);
        let panes = self.projects.terms();
        let panel = match self.side.filter(|_| self.panel_shown()) {
            Some(Side::Changes) => Some(Panel::Changes),
            Some(Side::Files) => Some(Panel::Files),
            Some(Side::Todo) => Some(Panel::Todo),
            Some(Side::Flow) => Some(Panel::Flow),
            None => None,
        };
        let dialog = match &self.overlay {
            Some(Overlay::Issues(_)) => Some(Dialog::Issues),
            Some(Overlay::Search(_)) => Some(Dialog::Search),
            Some(Overlay::Settings(_)) => Some(Dialog::Settings),
            Some(Overlay::Flows { .. }) => Some(Dialog::Flows),
            Some(Overlay::Usage) => Some(Dialog::Usage),
            Some(Overlay::Remote) => Some(Dialog::Remote),
            Some(Overlay::Keys(_)) => Some(Dialog::Keys),
            Some(Overlay::Queue(_)) => Some(Dialog::Queue),
            _ => None,
        };
        Facts {
            projects: open().count(),
            workspaces: open().map(|p| p.workspaces.len()).sum(),
            tabs: self.projects.tabs().count(),
            panes: self.projects.terms().count(),
            agents: self.projects.terms().filter(|t| t.agent.status().is_some()).count(),
            prompted: panes.filter_map(|t| t.context.prompted()).max(),
            panel,
            dialog,
            todos: self.todos.items().len(),
            agents_section: self.config.agents_section,
            repository: self.project().is_some_and(|p| crate::git::is_repo_root(&p.path)),
            compact: self.drawn.compact(),
        }
    }

    pub(super) fn offer_tour(&mut self, now: Instant) {
        let window = self.star.window.is_some_and(|at| now.saturating_duration_since(at) <= WINDOW_GAP);
        if window && self.tour.path.is_some() && !self.tour.saved.offered {
            self.tour.saved.offered = true;
            if self.tour.first_run {
                self.tour.saved.announced = true;
                if self.config.tour {
                    self.start_tour(0, None, true);
                }
            } else if !self.tour.found {
                self.tour.saved.announced = true;
                self.toast = Some(Toast::new(ANNOUNCE, ui::ToastIcon::Check));
                log::info!("tour", "announced");
            }
            self.save_tour();
        }
        self.watch_tour();
    }

    fn watch_tour(&mut self) {
        let Some(active) = &self.tour.active else { return };
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
        let current = self.tour.active.as_ref().is_some_and(|a| a.tour().id == tour.id && a.step().id == step);
        if current {
            self.tour_forward();
        } else if self.tour.saved.complete(tour, step) {
            self.save_tour();
        }
    }

    fn complete_step(&mut self) {
        let Some(active) = &self.tour.active else { return };
        let (tour, step) = (active.tour(), active.step().id);
        if self.tour.saved.complete(tour, step) {
            self.save_tour();
        }
    }

    pub(super) fn start_tour(&mut self, tour: usize, step: Option<usize>, card: bool) {
        let Some(t) = TOURS.get(tour) else { return };
        let step = step.unwrap_or_else(|| self.tour.saved.resume_at(t)).min(t.steps.len().saturating_sub(1));
        if self.tour.saved.finished(t.id) {
            self.tour.saved.tours.entry(t.id.to_string()).or_default().finished = false;
        }
        log::info!("tour", "started", tour = t.id, step = step);
        self.tour.active = Some(Active {
            tour,
            step,
            start: self.tour_facts(),
            card,
            field: None,
            focused: false,
            idea: 0,
            note: None,
        });
        self.enter_step();
    }

    fn enter_step(&mut self) {
        let facts = self.tour_facts();
        let Some(active) = &mut self.tour.active else { return };
        active.start = facts;
        active.note = None;
        active.focused = false;
        active.field = (active.step().offer == Some(Offer::Prompt)).then(|| Editor::new(IDEAS[active.idea % IDEAS.len()]));
    }

    pub(super) fn move_tour(&mut self, to: Move) {
        let Some(active) = &mut self.tour.active else { return };
        let count = active.tour().steps.len();
        match to {
            Move::Back => active.step = active.step.saturating_sub(1),
            Move::Next | Move::Skip if active.step + 1 < count => active.step += 1,
            Move::Next | Move::Skip => {
                let tour = active.tour();
                self.tour.saved.finish(tour);
                self.save_tour();
                log::info!("tour", "finished", tour = tour.id);
                self.tour.active = None;
                self.toast = Some(Toast::new(format!("{} done: Help has more tours", tour.title), ui::ToastIcon::Check));
                return;
            }
        }
        self.enter_step();
    }

    pub(super) fn end_tour(&mut self) {
        if let Some(active) = self.tour.active.take() {
            log::info!("tour", "ended", tour = active.tour().id, step = active.step().id);
        }
    }

    pub(super) fn reset_tour(&mut self, tour: Option<&str>) {
        self.tour.saved.reset(tour);
        self.save_tour();
    }

    pub(super) fn save_tour(&self) {
        let Some(path) = &self.tour.path else { return };
        if let Err(e) = tour::save(path, &self.tour.saved) {
            log::warning!("tour", "failed to save", error = e);
        }
    }

    fn tour_agent(&self) -> Option<String> {
        let installed = |kind: &str| agents::installed(&agents::command(&self.config, kind));
        let chosen = Some(self.config.agent.clone()).filter(|a| a != "auto" && installed(a));
        chosen.or_else(|| agents::kinds(&self.config).into_iter().find(|kind| installed(kind)))
    }

    pub(super) fn tour_card(&self, area: Rect) -> Option<ui::TourCard> {
        let active = self.tour.active.as_ref().filter(|a| a.card)?;
        let (tour, step) = (active.tour(), active.step());
        let facts = self.tour_facts();
        let areas = self.layout(area).shown(self.nav);
        let (target, button) = self.tour_target(&areas, step.target);
        let agent = (step.offer == Some(Offer::Prompt)).then(|| self.tour_agent()).flatten();
        let actions = match step.offer {
            Some(Offer::Prompt) if agent.is_some() && self.project().is_some() => {
                vec![TourButton::Idea, TourButton::Start]
            }
            Some(Offer::AgentsSection) if !facts.agents_section => vec![TourButton::TurnOn],
            Some(Offer::Settings) => vec![TourButton::Settings],
            _ => Vec::new(),
        };
        let note = active.note.clone().or_else(|| match step.offer {
            Some(Offer::Prompt) if agent.is_none() => {
                Some("No agent is installed here. Install Claude Code, Codex or another agent, then start it in a tab.".into())
            }
            Some(Offer::Prompt) if self.project().is_none() => Some("Open a project first.".into()),
            _ => None,
        });
        let last = active.step + 1 == tour.steps.len();
        let forward = if step.done.asks(&facts) {
            TourButton::Skip
        } else if last {
            TourButton::Done
        } else {
            TourButton::Next
        };
        Some(ui::TourCard {
            title: format!("{} · {}/{}", tour.title, active.step + 1, tour.steps.len()),
            step: step.title.into(),
            text: step.text.into(),
            note,
            field: active.field.clone().filter(|_| !actions.is_empty()),
            focused: active.focused,
            agent,
            actions,
            back: active.step > 0,
            forward,
            target,
            button,
        })
    }

    pub(super) fn tour_shown(&self) -> bool {
        self.tour.active.as_ref().is_some_and(|a| a.card)
    }

    pub(super) fn tour_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> bool {
        if self.tour.swallowed {
            match ev.kind {
                MouseEventKind::Drag(MouseButton::Left) => return true,
                MouseEventKind::Up(MouseButton::Left) => {
                    self.tour.swallowed = false;
                    return true;
                }
                _ => self.tour.swallowed = false,
            }
        }
        let Some(card) = self.tour_card(area) else { return false };
        let layout = ui::tour_layout(area, &card);
        if !layout.area.contains(pos) {
            if matches!(ev.kind, MouseEventKind::Down(_))
                && let Some(active) = &mut self.tour.active
            {
                active.focused = false;
            }
            return false;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return true;
        }
        self.tour.swallowed = true;
        if layout.field.contains(pos) {
            if let Some(active) = &mut self.tour.active {
                active.focused = true;
                if let Some(editor) = &mut active.field {
                    let width = usize::from(layout.field.width.saturating_sub(1)).max(1);
                    let (line, col) = (usize::from(pos.y - layout.field.y), usize::from(pos.x - layout.field.x));
                    editor.place(width, line + editor.scroll(), col);
                }
            }
            return true;
        }
        if let Some(button) = ui::tour_hit(area, &card, pos) {
            self.tour_button(button, area);
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
            TourButton::Docs => {
                if let Some(active) = &self.tour.active {
                    let url = active.tour().docs_url();
                    self.copy(&url);
                    self.toast = Some(Toast::new(format!("copied {url}"), ui::ToastIcon::Check));
                }
            }
            TourButton::Idea => {
                if let Some(active) = &mut self.tour.active {
                    active.idea = (active.idea + 1) % IDEAS.len();
                    active.field = Some(Editor::new(IDEAS[active.idea]));
                }
            }
            TourButton::TurnOn => {
                let mut config = self.config.clone();
                config.agents_section = true;
                if let Err(e) = self.write_config(&config) {
                    self.toast = Some(Toast::new(e, ui::ToastIcon::Bug));
                }
            }
            TourButton::Settings => {
                self.settings_page = Page::Agents;
                self.open_settings();
            }
            TourButton::Start => self.start_tour_prompt(area),
        }
    }

    fn start_tour_prompt(&mut self, area: Rect) {
        let Some(agent) = self.tour_agent() else { return };
        let workspace = self.project().and_then(|p| p.workspace()).map(|w| w.id);
        let prompt = self.tour.active.as_ref().and_then(|a| a.field.as_ref()).map(Editor::text);
        let Some(prompt) = prompt.filter(|p| !p.trim().is_empty()) else { return };
        let client = CLIENTS + self.tour.clients;
        self.tour.clients += 1;
        self.tour.asking = Some(client);
        if let Some(active) = &mut self.tour.active {
            active.focused = false;
            active.note = Some(format!("starting {agent}…"));
        }
        let start = control::Start { agent: Some(agent), workspace, prompt: Some(prompt), focus: true, ..Default::default() };
        let command = Command::Start(start);
        let request = Request { caller: None, server: None, contract: None, boot: None, view: None, command };
        self.run_request(client, Ok(request), 0, Some(area), Instant::now());
    }

    pub(in crate::app) fn tour_answered(&mut self, client: u64, reply: &Result<Value, String>) -> bool {
        if !(CLIENTS..CLIENTS * 2).contains(&client) {
            return false;
        }
        if self.tour.asking == Some(client)
            && let Some(active) = &mut self.tour.active
        {
            active.note = Some(match reply {
                Ok(_) => "Started. This step completes once the agent takes the prompt.".into(),
                Err(reason) => reason.clone(),
            });
        }
        true
    }

    pub(super) fn tour_typing(&self) -> bool {
        self.tour_shown() && self.tour.active.as_ref().is_some_and(|a| a.focused && a.field.is_some())
    }

    pub(super) fn tour_key(&mut self, key: KeyEvent, area: Rect) {
        let width = self
            .tour_card(area)
            .map(|card| usize::from(ui::tour_layout(area, &card).field.width.saturating_sub(1)).max(1))
            .unwrap_or(1);
        let Some(active) = &mut self.tour.active else { return };
        if key.code == KeyCode::Esc {
            active.focused = false;
            active.field = Some(Editor::new(IDEAS[active.idea]));
            return;
        }
        if let Some(editor) = &mut active.field {
            editor.key(key, width, usize::from(ui::TOUR_FIELD_ROWS));
        }
    }

    pub(super) fn tour_paste(&mut self, text: &str) {
        if let Some(editor) = self.tour.active.as_mut().and_then(|a| a.field.as_mut()) {
            editor.insert_lines(text);
        }
    }
}
