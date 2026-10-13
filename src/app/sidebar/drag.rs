use std::time::Instant;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use crate::app::App;
use crate::app::input::wheel;
use crate::app::interaction::AUTO_SCROLL_EVERY;
use crate::app::interaction::Drag;
use crate::app::interaction::Grab;
use crate::app::interaction::RowDrag;
use crate::app::menus::Target;
use crate::log::{self};
use crate::project::move_before;
use crate::search::Goto;
use crate::ui::{self, SidebarRow, WorkspaceRow};

impl App {
    pub(in crate::app) fn grab(&mut self, target: Target, row: Rect, area: Rect, fold: bool) {
        self.drag =
            Some(Drag::Row(RowDrag { target: Grab::Row(target), row, moved: false, area, scrolled: None, fold }));
    }

    pub(in crate::app) fn drag_row(&mut self, drag: RowDrag, ev: MouseEvent, areas: &ui::Areas, area: Rect) {
        let pos = Position::new(ev.column, ev.row);
        match ev.kind {
            MouseEventKind::Drag(MouseButton::Left) => {
                self.drag = Some(Drag::Row(RowDrag { moved: drag.moved || !drag.row.contains(pos), area, ..drag }));
                self.auto_scroll(Instant::now());
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.drag = None;
                let dropped = drag.moved || !drag.row.contains(pos);
                match drag.target {
                    Grab::Row(target) if dropped => self.drop_row(target, pos, area),
                    Grab::Row(target) if drag.fold => self.toggle_fold(target),
                    Grab::Row(target) => self.click_row(target),
                    Grab::Todo(id) if dropped => self.drop_todo(id, pos, area),
                    Grab::Todo(id) => self.click_todo(id, drag.row, pos, area),
                    Grab::Agent(_) if dropped => {}
                    Grab::Agent(pane) => self.jump_to_pane(pane),
                }
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                if let Some(delta) = wheel(ev.kind) {
                    match drag.target {
                        Grab::Row(_) | Grab::Agent(_) => _ = self.scroll_column(areas, pos, delta),
                        Grab::Todo(_) => self.scroll_todo(areas.changes, delta),
                    }
                }
                self.drag = Some(Drag::Row(RowDrag { moved: true, area, ..drag }));
            }
            _ => self.drag = None,
        }
    }

    pub(in crate::app) fn click_row(&mut self, target: Target) {
        match target {
            Target::Group(id) => {
                if let Some(entry) = self.group_mut(id) {
                    entry.collapsed = !entry.collapsed;
                }
            }
            Target::Project(id) => {
                if let Some(p) = self.project_index(id) {
                    self.active = p;
                    self.nav = self.nav.map(|_| ui::Nav::Workspaces);
                    self.unfold_focus();
                }
            }
            Target::Workspace(project, workspace) => {
                self.goto(Goto::Place { project, workspace: Some(workspace), tab: None });
            }
            Target::Tab(project, workspace, tab) => {
                self.goto(Goto::Place { project, workspace: Some(workspace), tab: Some(tab) });
            }
        }
    }

    pub(in crate::app) fn drag_view(&self, target: Target, pos: Position, area: Rect) -> Option<ui::Drag> {
        let areas = self.layout(area).shown(self.nav);
        let row = self.tree_row_of(target)?;
        if let ui::TreeRow::Tab(p, w, t) = row
            && !areas.tab_bar.is_empty()
        {
            let shown = p == self.active && self.project().is_some_and(|project| project.active == w);
            return shown.then(|| ui::Drag::Bar(t, self.tab_strip(&areas).drop(t, pos)));
        }
        if areas.tree {
            let shape = self.tree_shape();
            return Some(ui::Drag::Tree(row, ui::tree_drop(areas.list, &shape, self.scroll.projects, row, pos)));
        }
        let sidebar = |row: SidebarRow| {
            let rows = self.sidebar_rows();
            ui::Drag::Sidebar(row, ui::sidebar_drop(areas.list, areas.pitch, &rows, self.scroll.projects, row, pos))
        };
        let workspaces = |row: WorkspaceRow| {
            let (list, tabs) = (areas.workspaces_list, self.tab_lines());
            ui::Drag::Workspaces(row, ui::workspace_drop(list, areas.pitch, &tabs, self.scroll.workspaces, row, pos))
        };
        match row {
            ui::TreeRow::Group(g) => Some(sidebar(SidebarRow::Group(g))),
            ui::TreeRow::Project(p) => Some(sidebar(SidebarRow::Project(p))),
            ui::TreeRow::Workspace(p, w) if p == self.active => Some(workspaces(WorkspaceRow::Workspace(w))),
            ui::TreeRow::Tab(p, w, t) if p == self.active => Some(workspaces(WorkspaceRow::Tab(w, t))),
            _ => None,
        }
    }

    fn drop_row(&mut self, target: Target, pos: Position, area: Rect) {
        let landing = match self.drag_view(target, pos, area) {
            Some(
                ui::Drag::Sidebar(_, Some(landing))
                | ui::Drag::Workspaces(_, Some(landing))
                | ui::Drag::Tree(_, Some(landing)),
            ) => landing,
            Some(ui::Drag::Bar(_, Some(before))) => ui::Landing { at: before, spot: ui::Spot::Tab(before) },
            _ => return,
        };
        let Some(row) = self.tree_row_of(target) else { return };
        log::info!("app", "row moved", row = format!("{target:?}"), to = format!("{:?}", landing.spot));
        match (row, landing.spot) {
            (ui::TreeRow::Group(g), ui::Spot::Group(before)) => move_before(&mut self.groups, g, before, None),
            (ui::TreeRow::Project(p), ui::Spot::Project { group, before }) => {
                self.move_project(self.projects[p].id, group, before);
            }
            (ui::TreeRow::Workspace(p, w), ui::Spot::Workspace(before)) => {
                let project = &mut self.projects[p];
                move_before(&mut project.workspaces, w, before, Some(&mut project.active));
            }
            (ui::TreeRow::Tab(p, w, t), ui::Spot::Tab(before)) => {
                let workspace = &mut self.projects[p].workspaces[w];
                move_before(&mut workspace.tabs, t, before, Some(&mut workspace.active));
            }
            _ => {}
        }
    }

    pub(in crate::app) fn auto_scroll(&mut self, now: Instant) {
        let Some(drag) = self.row_drag().filter(|d| d.moved) else { return };
        let Some(pos) = self.hover else { return };
        if drag.scrolled.is_some_and(|at| now.duration_since(at) < AUTO_SCROLL_EVERY) {
            return;
        }
        let areas = self.layout(drag.area).shown(self.nav);
        let target = match drag.target {
            Grab::Row(target) => target,
            Grab::Todo(_) => {
                if self.auto_scroll_todo(areas.changes, pos) {
                    self.drag = Some(Drag::Row(RowDrag { scrolled: Some(now), ..drag }));
                }
                return;
            }
            Grab::Agent(_) => return,
        };
        if matches!(target, Target::Tab(..)) && !areas.tab_bar.is_empty() {
            if let Some(delta) = self.tab_strip(&areas).edge(pos) {
                self.scroll.tab_bar = self.tab_strip(&areas).scrolled(delta);
                self.drag = Some(Drag::Row(RowDrag { scrolled: Some(now), ..drag }));
            }
            return;
        }
        let sidebar = areas.tree || matches!(target, Target::Group(_) | Target::Project(_));
        let rows = if sidebar {
            self.sidebar_layout(&areas)
        } else {
            ui::workspace_layout(areas.workspaces_list, areas.pitch, &self.tab_lines(), self.scroll.workspaces)
        };
        let Some(delta) = rows.edge(pos) else { return };
        let scroll = if sidebar { &mut self.scroll.projects } else { &mut self.scroll.workspaces };
        *scroll = rows.scrolled(delta);
        self.drag = Some(Drag::Row(RowDrag { scrolled: Some(now), ..drag }));
    }
}
