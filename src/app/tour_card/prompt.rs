use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Position, Rect};
use serde_json::Value;

use super::CLIENTS;
use crate::agents;
use crate::app::App;
use crate::control::{self, Command, Request};
use crate::editor::Editor;
use crate::ui;

const IDEAS: [&str; 3] = [
    "Explain this repository's structure: what each top-level folder holds and where to start reading.",
    "Find one thing in this code worth improving, and say why. Do not change anything yet.",
    "List the commands to build, test and run this project, and check that they work.",
];

pub(super) fn idea(index: usize) -> Editor {
    Editor::new(IDEAS[index % IDEAS.len()])
}

impl App {
    pub(super) fn tour_agent(&self) -> Option<String> {
        let installed = |kind: &str| agents::installed(&agents::command(&self.config, kind));
        let chosen = Some(self.config.agent.clone()).filter(|a| a != "auto" && installed(a));
        chosen.or_else(|| agents::kinds(&self.config).into_iter().find(|kind| installed(kind)))
    }

    pub(super) fn next_idea(&mut self) {
        if let Some(active) = &mut self.messages.tour.active {
            active.idea = (active.idea + 1) % IDEAS.len();
            active.field = Some(idea(active.idea));
        }
    }

    pub(super) fn focus_tour_field(&mut self, field: Rect, pos: Position) {
        let Some(active) = &mut self.messages.tour.active else { return };
        active.focused = true;
        let Some(editor) = &mut active.field else { return };
        let width = usize::from(field.width.saturating_sub(1)).max(1);
        let (line, col) = (usize::from(pos.y - field.y), usize::from(pos.x - field.x));
        editor.place(width, line + editor.scroll(), col);
    }

    pub(super) fn start_tour_prompt(&mut self, area: Rect) {
        let Some(agent) = self.tour_agent() else { return };
        let workspace = self.project().and_then(|p| p.workspace()).map(|w| w.id);
        let prompt = self.messages.tour.active.as_ref().and_then(|a| a.field.as_ref()).map(Editor::text);
        let Some(prompt) = prompt.filter(|p| !p.trim().is_empty()) else { return };
        let tour = &mut self.messages.tour;
        let client = CLIENTS + tour.clients;
        tour.clients += 1;
        tour.asking = Some(client);
        if let Some(active) = &mut tour.active {
            active.focused = false;
            active.note = Some(format!("starting {agent}…"));
        }
        let start =
            control::Start { agent: Some(agent), workspace, prompt: Some(prompt), focus: true, ..Default::default() };
        let command = Command::Start(start);
        let request = Request { caller: None, server: None, contract: None, boot: None, view: None, command };
        self.run_request(client, Ok(request), Some(area), Instant::now());
    }

    pub(in crate::app) fn tour_answered(&mut self, client: u64, reply: &Result<Value, String>) -> bool {
        if !(CLIENTS..CLIENTS * 2).contains(&client) {
            return false;
        }
        if self.messages.tour.asking == Some(client)
            && let Some(active) = &mut self.messages.tour.active
        {
            active.note = Some(match reply {
                Ok(_) => "Started. This step completes once the agent takes the prompt.".into(),
                Err(reason) => reason.clone(),
            });
        }
        true
    }

    pub(in crate::app) fn tour_typing(&self) -> bool {
        self.tour_shown() && self.messages.tour.active.as_ref().is_some_and(|a| a.focused && a.field.is_some())
    }

    pub(in crate::app) fn tour_key(&mut self, key: KeyEvent, area: Rect) {
        let width = self
            .tour_card(area)
            .map_or(1, |card| usize::from(ui::tour_layout(area, &card).field.width.saturating_sub(1)).max(1));
        let Some(active) = &mut self.messages.tour.active else { return };
        if key.code == KeyCode::Esc {
            active.focused = false;
            active.field = Some(idea(active.idea));
            return;
        }
        if let Some(editor) = &mut active.field {
            editor.key(key, width, usize::from(ui::TOUR_FIELD_ROWS));
        }
    }

    pub(in crate::app) fn tour_paste(&mut self, text: &str) {
        if let Some(editor) = self.messages.tour.active.as_mut().and_then(|a| a.field.as_mut()) {
            editor.insert_lines(text);
        }
    }
}
