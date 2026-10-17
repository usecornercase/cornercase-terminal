use std::collections::HashMap;

use ratatui::layout::Rect;

use super::App;
use super::control::{Handled, json, none};
use crate::control::contract::{Code, Refusal};
use crate::control::{self, Ids, Item, OpenView, Report, SetView, Snapshot, ViewInfo};
use crate::log;
use crate::project::view::{Claim, Shown, controller};
use crate::project::{Hierarchy, Tab};

use super::events::Streamed;

pub(super) struct View {
    pub(super) id: u64,
    client: u64,
    pub(super) size: Option<(u16, u16)>,
    pub(super) active: bool,
    pub(super) shown: Shown,
    pub(super) projection: u64,
    claimed: u64,
    revision: u64,
    sent: Option<Report>,
}

impl View {
    pub(super) fn tab(&self) -> Option<u64> {
        self.shown.tab
    }

    pub(super) fn eligible(&self) -> Option<u64> {
        self.shown.tab.filter(|_| self.active && self.size.is_some())
    }

    pub(super) fn area(&self) -> Option<Rect> {
        self.size.map(|(cols, rows)| Rect::new(0, 0, cols, rows))
    }

    pub(super) fn why_not(&self) -> &'static str {
        if !self.active {
            "it is inactive"
        } else if self.size.is_none() {
            "it has no size"
        } else {
            "it shows no tab"
        }
    }
}

#[derive(Default)]
pub(super) struct Views {
    list: Vec<View>,
    seq: u64,
    window: Option<(u64, u64)>,
    changed: bool,
    area: Option<Rect>,
    logged: HashMap<u64, u64>,
}

impl Views {
    pub(super) fn get(&self, id: u64) -> Option<&View> {
        self.list.iter().find(|view| view.id == id)
    }

    fn get_mut(&mut self, id: u64) -> Option<&mut View> {
        self.list.iter_mut().find(|view| view.id == id)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub(super) fn changed(&self) -> bool {
        self.changed
    }

    fn claim(&mut self, id: u64) {
        self.seq += 1;
        let seq = self.seq;
        if let Some(view) = self.get_mut(id) {
            view.claimed = seq;
        }
        self.changed = true;
    }

    fn claims(&self) -> impl Iterator<Item = Claim> + '_ {
        self.list.iter().filter_map(|v| Some(Claim { view: v.id, tab: v.eligible()?, seq: v.claimed }))
    }

    pub(super) fn controller(&self, tab: u64, focus: Option<u64>) -> Option<u64> {
        let window = self.window.filter(|(on, _)| Some(*on) == focus);
        controller(tab, window, self.claims())
    }

    fn sized(&self, focus: Option<u64>) -> HashMap<u64, Rect> {
        let controls = |view: &&View| view.eligible().is_some_and(|tab| self.controller(tab, focus) == Some(view.id));
        let area = |view: &View| Some((view.tab()?, view.area()?));
        self.list.iter().filter(controls).filter_map(area).collect()
    }
}

impl App {
    fn window_shown(&self) -> Shown {
        let Some(project) = self.project() else { return Shown::default() };
        let workspace = project.workspace();
        Shown::place(&self.projects, self.active, workspace.map(|_| project.active), workspace.map(|w| w.active))
    }

    pub(super) fn tab_controller(&self, tab: u64) -> Option<u64> {
        self.views.controller(tab, self.focus().tab)
    }

    fn tab_ids(&self, shown: &Shown) -> Ids {
        let pane = shown.tab.and_then(|id| self.projects.tabs().find(|tab| tab.id == id)).and_then(Tab::pane);
        Ids { project: shown.project, workspace: shown.workspace, tab: shown.tab, pane: pane.map(|term| term.id) }
    }

    fn view_info(&self, view: &View) -> ViewInfo {
        let controls = view.eligible().is_some_and(|tab| self.tab_controller(tab) == Some(view.id));
        ViewInfo {
            id: view.id,
            projection: view.projection,
            active: view.active,
            cols: view.size.map(|(cols, _)| cols),
            rows: view.size.map(|(_, rows)| rows),
            shown: self.tab_ids(&view.shown),
            controls,
        }
    }

    pub(in crate::app) fn view_report(&self, id: u64) -> Option<Report> {
        let info = self.view_info(self.views.get(id)?);
        Some(Report { view: Some(info), ..self.report(None) })
    }

    pub(in crate::app) fn view_ids(&self, id: u64) -> Ids {
        self.views.get(id).map(|view| self.tab_ids(&view.shown)).unwrap_or_default()
    }

    pub(in crate::app) fn view_projection(&self, id: u64) -> Option<u64> {
        self.views.get(id).map(|view| view.projection)
    }

    pub(in crate::app) fn view_request(&mut self, client: u64, open: &OpenView, area: Option<Rect>) -> Handled {
        let size = open.size().map_err(|message| Refusal::new(Code::Malformed, message))?;
        if self.views.list.iter().any(|view| view.client == client) {
            return Err(Refusal::from("this connection already holds a view; open another connection for another"));
        }
        let shown = if let Some(id) = open.tab {
            let (p, w, t) = self.projects.tab_index(id).ok_or_else(|| none("tab", id))?;
            Shown::place(&self.projects, p, Some(w), Some(t))
        } else if let Some(id) = open.workspace {
            let (p, w) = self.usable_workspace(id)?;
            Shown::place(&self.projects, p, Some(w), None)
        } else if let Some(id) = open.project {
            let p = self.projects.project_index(id).ok_or_else(|| none("project", id))?;
            Shown::place(&self.projects, p, None, None)
        } else {
            self.window_shown()
        };
        let id = self.take_id();
        let active = open.active.unwrap_or(true);
        let view = View { id, client, size, active, shown, projection: 1, claimed: 0, revision: 0, sent: None };
        let eligible = view.eligible().is_some();
        self.views.list.push(view);
        if eligible {
            self.views.claim(id);
        }
        log::info!(
            "control",
            "view opened",
            client = client,
            view = id,
            cols = size.map_or(0, |s| s.0),
            rows = size.map_or(0, |s| s.1),
            active = active
        );
        self.size_tabs(area);
        let report = self.view_report(id).ok_or_else(|| Refusal::from("the view closed"))?;
        let view = self.views.get_mut(id).ok_or_else(|| Refusal::from("the view closed"))?;
        view.revision = 1;
        view.sent = Some(report.clone());
        Ok(Some(json(&Snapshot { revision: 1, report })))
    }

    pub(in crate::app) fn view_set_request(&mut self, id: u64, set: &SetView, area: Option<Rect>) -> Handled {
        let size = control::view::size(set.cols, set.rows).map_err(|message| Refusal::new(Code::Malformed, message))?;
        let Some(view) = self.views.get_mut(id) else { return Err(none("view", id)) };
        let was = view.eligible().is_some();
        let (new_size, new_active) = (size.or(view.size), set.active.unwrap_or(view.active));
        let eligible = view.shown.tab.is_some() && new_active && new_size.is_some();
        if set.claim && !eligible {
            let why = if !new_active {
                "it is inactive"
            } else if new_size.is_none() {
                "it has no size"
            } else {
                "it shows no tab"
            };
            return Err(Refusal::from(format!("view {id} cannot size a tab now: {why}")));
        }
        if (new_size, new_active) != (view.size, view.active) {
            view.projection += 1;
        }
        (view.size, view.active) = (new_size, new_active);
        if eligible && (!was || set.claim) {
            self.views.claim(id);
        }
        self.views.changed = true;
        log::info!("control", "view changed", view = id, active = set.active.unwrap_or(false), claim = set.claim);
        self.size_tabs(area);
        let info = self.views.get(id).map(|view| self.view_info(view)).unwrap_or_default();
        Ok(Some(json(&info)))
    }

    pub(in crate::app) fn show_to(&mut self, view: Option<u64>, item: Item) -> Result<Option<u64>, Refusal> {
        let Some(id) = view else { return self.show(item).map(|()| None) };
        let shown = match item {
            Item::Group(group) => {
                let first = self.projects.iter().position(|p| p.group == Some(group) && !p.closing);
                let p = first.ok_or_else(|| none("group", group))?;
                Shown::place(&self.projects, p, None, None)
            }
            Item::Project(project) => {
                let p = self.projects.project_index(project).ok_or_else(|| none("project", project))?;
                if self.projects[p].closing {
                    return Err(format!("project {project} is closing").into());
                }
                Shown::place(&self.projects, p, None, None)
            }
            Item::Workspace(workspace) => {
                let (p, w) = self.usable_workspace(workspace)?;
                Shown::place(&self.projects, p, Some(w), None)
            }
            Item::Tab(tab) => {
                let (p, w, t) = self.projects.tab_index(tab).ok_or_else(|| none("tab", tab))?;
                Shown::place(&self.projects, p, Some(w), Some(t))
            }
            Item::Pane(pane) => {
                let (p, w, t) = self.projects.tab_of_pane(pane).ok_or_else(|| none("pane", pane))?;
                self.projects[p].workspaces[w].tabs[t].focus(pane);
                Shown::place(&self.projects, p, Some(w), Some(t))
            }
        };
        let view = self.views.get_mut(id).ok_or_else(|| none("view", id))?;
        if view.shown.tab != shown.tab || view.shown.workspace != shown.workspace || view.shown.project != shown.project
        {
            view.projection += 1;
        }
        view.shown = shown;
        let eligible = view.eligible().is_some();
        if eligible {
            self.views.claim(id);
        }
        self.views.changed = true;
        let area = self.views.area;
        self.size_tabs(area);
        Ok(self.view_projection(id))
    }

    pub(in crate::app) fn focus_in_view(&mut self, view: Option<u64>, pane: u64) -> Result<Option<u64>, Refusal> {
        if let Some(id) = view
            && self.views.get(id).is_none()
        {
            log::info!("control", "view closed before the focus", view = id, pane = pane);
            return Ok(None);
        }
        self.show_to(view, Item::Pane(pane))
    }

    pub(in crate::app) fn settle_views(&mut self) {
        if self.views.list.is_empty() {
            return;
        }
        let window = self.window_shown();
        let mut moved = false;
        for view in &mut self.views.list {
            if view.shown.settle(&self.projects, || window) {
                view.projection += 1;
                view.claimed = 0;
                moved = true;
            }
        }
        if moved {
            self.views.changed = true;
            let area = self.views.area;
            self.size_tabs(area);
        }
    }

    pub(in crate::app) fn forget_views(&mut self, client: u64) {
        let Some(at) = self.views.list.iter().position(|view| view.client == client) else { return };
        let view = self.views.list.remove(at);
        log::info!("control", "view closed", client = client, view = view.id);
        self.views.changed = true;
        let area = self.views.area;
        self.size_tabs(area);
    }

    pub(crate) fn window_gone(&mut self) {
        if self.views.window.take().is_some() {
            self.views.changed = true;
            let area = self.views.area;
            self.size_tabs(area);
        }
    }

    pub(crate) fn window_used(&mut self) {
        if self.views.list.is_empty() {
            return;
        }
        let Some(tab) = self.visible_tab() else { return };
        let before = self.tab_controller(tab);
        self.views.seq += 1;
        self.views.window = Some((tab, self.views.seq));
        if self.tab_controller(tab) != before {
            self.views.changed = true;
        }
        let area = self.views.area;
        self.size_tabs(area);
    }

    pub(crate) fn resize(&mut self, area: Rect) {
        self.size_tabs(Some(area));
    }

    fn size_tabs(&mut self, window: Option<Rect>) {
        if window.is_some() {
            self.views.area = window;
        }
        let focus = self.focus().tab;
        if self.views.window.is_some_and(|(tab, _)| Some(tab) != focus) {
            self.views.window = None;
        }
        let pane = window.map(|area| self.layout(area).pane);
        let sized = self.views.sized(focus);
        for tab in self.projects.tabs_mut() {
            let Some(area) = sized.get(&tab.id).copied().or(pane) else { continue };
            for (id, rect) in tab.shown(area) {
                if let Some(term) = tab.pane_by_mut(id) {
                    term.resize(rect.height.max(1), rect.width.max(1));
                }
            }
        }
        self.log_controllers(&sized);
    }

    fn log_controllers(&mut self, sized: &HashMap<u64, Rect>) {
        let focus = self.focus().tab;
        let now: HashMap<u64, u64> =
            sized.keys().map(|tab| (*tab, self.views.controller(*tab, focus).unwrap_or(0))).collect();
        for (tab, by) in &now {
            if self.views.logged.get(tab) != Some(by) {
                log::info!("control", "tab sized", tab = *tab, by = format!("view {by}"));
            }
        }
        for tab in self.views.logged.keys().filter(|tab| !now.contains_key(tab)) {
            log::info!("control", "tab sized", tab = *tab, by = "window");
        }
        self.views.logged = now;
    }

    pub(in crate::app) fn tab_area(&self, tab: u64, window: Rect) -> Rect {
        let view = self.views.sized(self.focus().tab).get(&tab).copied();
        view.unwrap_or_else(|| self.layout(window).pane)
    }

    pub(super) fn publish_views(&mut self, report: &Report) {
        self.views.changed = false;
        for at in 0..self.views.list.len() {
            let view = &self.views.list[at];
            let info = self.view_info(view);
            let report = Report { view: Some(info), ..report.clone() };
            let view = &mut self.views.list[at];
            if view.sent.as_ref() == Some(&report) {
                continue;
            }
            view.revision += 1;
            view.sent = Some(report.clone());
            let snapshot = Snapshot { revision: view.revision, report };
            let client = view.client;
            if let Some(text) = snapshot.streamed() {
                self.events.push(client, Streamed::Snapshot(text));
            }
        }
    }
}
