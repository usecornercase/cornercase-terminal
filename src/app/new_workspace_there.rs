use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};

use super::App;
use super::overlay::Overlay;
use super::worktrees::NAME_REQUIRED;
use crate::control::NewWorkspace;
use crate::editor::Editor;
use crate::error::Result;
use crate::machines::run_on::{RunOn, Target};
use crate::machines::send::Send;
use crate::ui;

pub(super) struct Asked {
    pub(super) project: u64,
    pub(super) input: Editor,
    pub(super) worktree: Option<bool>,
    pub(super) run_on: Option<RunOn>,
}

impl App {
    pub(super) fn new_workspace_place_there(&self, link: u64, project: u64, worktree: bool) -> String {
        if worktree {
            self.where_there(link, project, true)
        } else {
            self.placed_there(link, project, "in a workspace")
        }
    }

    pub(super) fn new_workspace_run_on_mouse(&mut self, pos: Position, area: Rect) -> bool {
        let Some(Overlay::NewWorkspace { run_on: Some(run_on), creating: false, .. }) = &self.overlay else {
            return false;
        };
        let Some(at) = ui::form_run_on_hit(area, &run_on_row(run_on), pos) else { return false };
        let Some(Overlay::NewWorkspace { run_on: Some(run_on), error, .. }) = &mut self.overlay else { return false };
        let at = at.checked_sub(1);
        if !run_on.is_chosen(at) && run_on.choose(at) {
            *error = None;
        }
        true
    }

    pub(super) fn follow_new_workspace_run_on(&mut self) {
        let Some(Overlay::NewWorkspace { project, run_on, .. }) = &self.overlay else { return };
        let fresh = RunOn::kept(self.run_on_for(*project), run_on.as_ref());
        if let Some(Overlay::NewWorkspace { run_on, .. }) = &mut self.overlay {
            *run_on = fresh;
        }
    }

    pub(super) fn submit_new_workspace(&mut self, asked: Asked, area: Rect) -> Result<Option<Overlay>> {
        if let Some(lost) = asked.run_on.as_ref().and_then(|run_on| run_on.lost.clone()) {
            let Asked { project, input, worktree, run_on } = asked;
            let error = Some(lost);
            return Ok(Some(Overlay::NewWorkspace { project, input, worktree, error, creating: false, run_on }));
        }
        if let Some(target) = asked.run_on.as_ref().and_then(RunOn::machine).cloned() {
            return Ok(self.new_workspace_there(&target, asked));
        }
        let Asked { project, input, worktree, run_on } = asked;
        let mut made = self.create_workspace(project, input, worktree, area)?;
        if let Some(Overlay::NewWorkspace { run_on: shown, .. }) = &mut made {
            *shown = run_on;
        }
        Ok(made)
    }

    fn new_workspace_there(&mut self, target: &Target, asked: Asked) -> Option<Overlay> {
        let name = asked.input.text().trim().to_string();
        let Asked { project, input, worktree, run_on } = asked;
        self.overlay = Some(Overlay::NewWorkspace { project, input, worktree, error: None, creating: false, run_on });
        if self.still_cloning(target.link, project) {
            return self.overlay.take();
        }
        let there = self.project_there(target.link, project).map(|there| there.id);
        let sent = match there {
            _ if name.is_empty() => Err(NAME_REQUIRED.to_string()),
            None => {
                self.open_clone_on_machine(target, project);
                return self.overlay.take();
            }
            Some(id) => {
                let worktree = worktree == Some(true);
                let new = NewWorkspace { name, project: Some(id), worktree, ..NewWorkspace::default() };
                self.send_from_dialog(target, Send::NewWorkspace(new))
            }
        };
        if let Some(Overlay::NewWorkspace { error, creating, .. }) = &mut self.overlay {
            match sent {
                Ok(()) => *creating = true,
                Err(why) => *error = Some(why),
            }
        }
        self.overlay.take()
    }
}

pub(super) fn run_on_row(run_on: &RunOn) -> ui::RunOnRow {
    ui::RunOnRow { spans: run_on.spans(Style::default().fg(Color::DarkGray)), choices: run_on.targets.len() + 1 }
}
