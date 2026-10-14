use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::flows_dialog::settings_row;
use super::input::wheel;
use super::overlay::Overlay;
use crate::flows::start::{Action, Hit, Room, Start};
use crate::git;
use crate::project::Hierarchy;
use crate::ui::{self, StartHit};

impl App {
    pub(super) fn start_flow_in(&self, project: u64) -> Option<Start> {
        let project = &self.projects[self.projects.project_index(project)?];
        let listing = self.flow_listing(Some(&project.path));
        Some(Start::new(project.id, self.project_label(project), git::is_repo_root(&project.path), listing))
    }

    pub(super) fn start_flow_here(&self) -> Option<Start> {
        self.project().and_then(|project| self.start_flow_in(project.id))
    }

    pub(super) fn open_start_flow(&mut self, start: Option<Start>) {
        let Some(start) = start else { return };
        self.nav = None;
        let back = self.overlay.take().filter(|o| matches!(o, Overlay::Issues(_))).map(Box::new);
        self.overlay = Some(Overlay::StartFlow { start: Box::new(start), back });
    }

    fn start_flow(&mut self) -> Option<&mut Start> {
        match &mut self.overlay {
            Some(Overlay::StartFlow { start, .. }) => Some(start.as_mut()),
            _ => None,
        }
    }

    pub(super) fn start_flow_key(&mut self, key: KeyEvent, area: Rect) {
        let Some(start) = self.start_flow() else { return };
        let room = room(start, area);
        let action = start.key(key, room);
        self.start_flow_act(action, area);
    }

    pub(super) fn start_flow_paste(&mut self, text: &str, area: Rect) {
        if let Some(start) = self.start_flow() {
            let room = room(start, area);
            start.paste(text, room);
        }
    }

    pub(super) fn start_flow_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let Some(start) = self.start_flow() else { return };
        let room = room(start, area);
        if let Some(delta) = wheel(ev.kind) {
            if ui::start_prompt(area, shape(start)).contains(pos) {
                start.scroll_prompt(delta, room);
            } else {
                start.scroll_list(delta.signum());
            }
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let ui::Overlay::StartFlow(view) = start_flow_view(start, area) else { return };
        let hit = match ui::start_hit(area, &view, pos) {
            Some(StartHit::Row(i)) => Hit::Row(i),
            Some(StartHit::Prompt { line, col }) => Hit::Prompt { line, col },
            Some(StartHit::Branch) => Hit::Branch,
            Some(StartHit::Button(i)) => Hit::Button(view.buttons[i]),
            None => return,
        };
        let action = start.hit(hit, room);
        self.start_flow_act(action, area);
    }

    fn run_open_on(&self, project: u64, branch: &str) -> bool {
        self.runs.iter().any(|run| {
            self.projects.workspace_index(run.workspace).is_some_and(|(p, w)| {
                self.projects[p].id == project
                    && git::branch(&self.projects[p].workspaces[w].path).as_deref() == Some(branch)
            })
        })
    }

    pub(super) fn start_flow_settled(&mut self, failure: Option<&str>) -> bool {
        let Some(start) = self.start_flow().filter(|start| start.waiting) else { return false };
        match failure {
            Some(why) => start.failed(why),
            None => self.overlay = None,
        }
        true
    }

    fn start_flow_act(&mut self, action: Action, area: Rect) {
        match action {
            Action::None => {}
            Action::Close => {
                if let Some(Overlay::StartFlow { back, .. }) = self.overlay.take() {
                    self.overlay = back.map(|back| *back);
                }
            }
            Action::Edit(origin, name) => {
                let project = self.start_flow().map(|start| start.project);
                self.open_flows(project, Some((&origin, &name)));
            }
            Action::Start { flow, task, branch } => {
                let Some(project) = self.start_flow().map(|start| start.project) else { return };
                let started = self.start_run(project, *flow, task, branch.clone(), true, area);
                let opened = started.is_ok() && self.run_open_on(project, &branch);
                match (started, self.start_flow()) {
                    (Ok(()), _) if opened => self.overlay = None,
                    (Ok(()), Some(start)) => start.waiting = true,
                    (Err(why), Some(start)) => start.notice = Some(why),
                    (_, None) => {}
                }
            }
        }
    }
}

fn shape(start: &Start) -> ui::StartShape {
    let rows = start.rows();
    let sections: Vec<&str> = rows.iter().map(|row| row.section).collect();
    ui::StartShape::new(&sections, start.warnings().len(), start.selected().is_some(), start.diagram)
}

fn room(start: &Start, area: Rect) -> Room {
    let prompt = ui::start_prompt(area, shape(start));
    Room { width: usize::from(prompt.width), height: usize::from(prompt.height), wide: ui::flows_wide(area) }
}

pub(super) fn start_flow_view(start: &Start, area: Rect) -> ui::Overlay {
    let room = room(start, area);
    ui::Overlay::StartFlow(Box::new(ui::StartFlow {
        title: start.title(),
        heading: start.heading(),
        rows: start.rows().iter().map(settings_row).collect(),
        cursor: start.cursor,
        prompt: start.prompt.clone(),
        branch: start.branch.clone(),
        focus: start.focus,
        warnings: start.warnings(),
        note: if start.waiting {
            Some(ui::Note::Busy("creating the worktree and starting the team…"))
        } else {
            start.notice.clone().map(ui::Note::Error)
        },
        hint: start.hint(),
        buttons: start.buttons(room),
        preview: start.selected().map(|saved| saved.flow.clone()),
        diagram: start.diagram,
    }))
}
