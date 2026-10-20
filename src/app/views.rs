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

fn resize_shown(tab: &mut Tab, area: Rect) {
    for (id, rect) in tab.shown(area) {
        if let Some(term) = tab.pane_by_mut(id) {
            term.resize(rect.height.max(1), rect.width.max(1));
        }
    }
}

fn log_opened(view: &View) {
    log::info!(
        "control",
        "view opened",
        client = view.client,
        view = view.id,
        cols = view.size.map_or(0, |s| s.0),
        rows = view.size.map_or(0, |s| s.1),
        active = view.active,
        visible = view.visible
    );
}

pub(super) struct View {
    pub(super) id: u64,
    client: u64,
    pub(super) size: Option<(u16, u16)>,
    pub(super) active: bool,
    visible: bool,
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

    fn sees(&self) -> Option<u64> {
        self.eligible().filter(|_| self.visible)
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
    windowed: bool,
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
        self.clients.views.controller(tab, self.focus().tab)
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
            visible: view.visible,
        }
    }

    pub(in crate::app) fn view_report(&self, id: u64) -> Option<Report> {
        let info = self.view_info(self.clients.views.get(id)?);
        Some(Report { view: Some(info), ..self.report(None) })
    }

    pub(in crate::app) fn view_ids(&self, id: u64) -> Ids {
        self.clients.views.get(id).map(|view| self.tab_ids(&view.shown)).unwrap_or_default()
    }

    pub(in crate::app) fn view_projection(&self, id: u64) -> Option<u64> {
        self.clients.views.get(id).map(|view| view.projection)
    }

    pub(in crate::app) fn view_request(&mut self, client: u64, open: &OpenView, area: Option<Rect>) -> Handled {
        let size = open.size().map_err(|message| Refusal::new(Code::Malformed, message))?;
        if self.clients.views.list.iter().any(|view| view.client == client) {
            return Err(Refusal::from("this connection already holds a view; open another connection for another"));
        }
        let shown = self.requested_shown(open)?;
        let id = self.take_id();
        let active = open.active.unwrap_or(true);
        let visible = open.visible.unwrap_or(true);
        let view =
            View { id, client, size, active, visible, shown, projection: 1, claimed: 0, revision: 0, sent: None };
        let eligible = view.eligible().is_some();
        log_opened(&view);
        self.clients.views.list.push(view);
        if eligible {
            self.clients.views.claim(id);
        }
        self.size_tabs(area);
        let report = self.view_report(id).ok_or_else(|| Refusal::from("the view closed"))?;
        let view = self.clients.views.get_mut(id).ok_or_else(|| Refusal::from("the view closed"))?;
        view.revision = 1;
        view.sent = Some(report.clone());
        Ok(Some(json(&Snapshot { revision: 1, report })))
    }

    fn requested_shown(&mut self, open: &OpenView) -> Result<Shown, Refusal> {
        if let Some(id) = open.tab {
            let (p, w, t) = self.projects.tab_index(id).ok_or_else(|| none("tab", id))?;
            Ok(Shown::place(&self.projects, p, Some(w), Some(t)))
        } else if let Some(id) = open.workspace {
            let (p, w) = self.usable_workspace(id)?;
            Ok(Shown::place(&self.projects, p, Some(w), None))
        } else if let Some(id) = open.project {
            let p = self.projects.project_index(id).ok_or_else(|| none("project", id))?;
            Ok(Shown::place(&self.projects, p, None, None))
        } else {
            Ok(self.window_shown())
        }
    }

    pub(in crate::app) fn view_set_request(&mut self, id: u64, set: &SetView, area: Option<Rect>) -> Handled {
        let size = control::view::size(set.cols, set.rows).map_err(|message| Refusal::new(Code::Malformed, message))?;
        let Some(view) = self.clients.views.get_mut(id) else { return Err(none("view", id)) };
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
        if let Some(visible) = set.visible {
            view.visible = visible;
        }
        if eligible && (!was || set.claim) {
            self.clients.views.claim(id);
        }
        self.clients.views.changed = true;
        log::info!(
            "control",
            "view changed",
            view = id,
            active = set.active.unwrap_or(false),
            claim = set.claim,
            visible = set.visible.unwrap_or(true)
        );
        self.size_tabs(area);
        let info = self.clients.views.get(id).map(|view| self.view_info(view)).unwrap_or_default();
        Ok(Some(json(&info)))
    }

    pub(in crate::app) fn show_to(&mut self, view: Option<u64>, item: Item) -> Result<Option<u64>, Refusal> {
        let Some(id) = view else { return self.show(item).map(|()| None) };
        let shown = self.item_shown(item)?;
        let view = self.clients.views.get_mut(id).ok_or_else(|| none("view", id))?;
        if view.shown.tab != shown.tab || view.shown.workspace != shown.workspace || view.shown.project != shown.project
        {
            view.projection += 1;
        }
        view.shown = shown;
        let eligible = view.eligible().is_some();
        if eligible {
            self.clients.views.claim(id);
        }
        self.clients.views.changed = true;
        let area = self.clients.views.area;
        self.size_tabs(area);
        Ok(self.view_projection(id))
    }

    fn item_shown(&mut self, item: Item) -> Result<Shown, Refusal> {
        Ok(match item {
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
        })
    }

    pub(in crate::app) fn focus_in_view(&mut self, view: Option<u64>, pane: u64) -> Result<Option<u64>, Refusal> {
        if let Some(id) = view
            && self.clients.views.get(id).is_none()
        {
            log::info!("control", "view closed before the focus", view = id, pane = pane);
            return Ok(None);
        }
        self.show_to(view, Item::Pane(pane))
    }

    pub(in crate::app) fn settle_views(&mut self) {
        if self.clients.views.list.is_empty() {
            return;
        }
        let window = self.window_shown();
        let mut moved = false;
        for view in &mut self.clients.views.list {
            if view.shown.settle(&self.projects, || window) {
                view.projection += 1;
                view.claimed = 0;
                moved = true;
            }
        }
        if moved {
            self.clients.views.changed = true;
            let area = self.clients.views.area;
            self.size_tabs(area);
        }
    }

    pub(in crate::app) fn forget_views(&mut self, client: u64) {
        let Some(at) = self.clients.views.list.iter().position(|view| view.client == client) else { return };
        let view = self.clients.views.list.remove(at);
        log::info!("control", "view closed", client = client, view = view.id);
        self.clients.views.changed = true;
        let area = self.clients.views.area;
        self.size_tabs(area);
    }

    pub(crate) fn seen_tabs(&self) -> Vec<u64> {
        let window = self.visible_tab().filter(|_| self.clients.views.list.is_empty() || self.clients.views.windowed);
        window.into_iter().chain(self.clients.views.list.iter().filter_map(View::sees)).collect()
    }

    pub(crate) fn window_attached(&mut self) {
        self.clients.views.windowed = true;
    }

    pub(crate) fn window_gone(&mut self) {
        self.clients.views.windowed = false;
        if self.clients.views.window.take().is_some() {
            self.clients.views.changed = true;
            let area = self.clients.views.area;
            self.size_tabs(area);
        }
    }

    pub(crate) fn window_used(&mut self) {
        if self.clients.views.list.is_empty() {
            return;
        }
        let Some(tab) = self.visible_tab() else { return };
        let before = self.tab_controller(tab);
        self.clients.views.seq += 1;
        self.clients.views.window = Some((tab, self.clients.views.seq));
        if self.tab_controller(tab) != before {
            self.clients.views.changed = true;
        }
        let area = self.clients.views.area;
        self.size_tabs(area);
    }

    pub(crate) fn resize(&mut self, area: Rect) {
        self.size_tabs(Some(area));
    }

    fn size_tabs(&mut self, window: Option<Rect>) {
        if window.is_some() {
            self.clients.views.area = window;
        }
        let focus = self.focus().tab;
        if self.clients.views.window.is_some_and(|(tab, _)| Some(tab) != focus) {
            self.clients.views.window = None;
        }
        let pane = window.map(|area| self.layout(area).pane);
        let sized = self.clients.views.sized(focus);
        for tab in self.projects.tabs_mut() {
            let Some(area) = sized.get(&tab.id).copied().or(pane) else { continue };
            resize_shown(tab, area);
        }
        self.log_controllers(&sized);
    }

    fn log_controllers(&mut self, sized: &HashMap<u64, Rect>) {
        let focus = self.focus().tab;
        let now: HashMap<u64, u64> =
            sized.keys().map(|tab| (*tab, self.clients.views.controller(*tab, focus).unwrap_or(0))).collect();
        for (tab, by) in &now {
            if self.clients.views.logged.get(tab) != Some(by) {
                log::info!("control", "tab sized", tab = *tab, by = format!("view {by}"));
            }
        }
        for tab in self.clients.views.logged.keys().filter(|tab| !now.contains_key(tab)) {
            log::info!("control", "tab sized", tab = *tab, by = "window");
        }
        self.clients.views.logged = now;
    }

    pub(in crate::app) fn tab_area(&self, tab: u64, window: Rect) -> Rect {
        let view = self.clients.views.sized(self.focus().tab).get(&tab).copied();
        view.unwrap_or_else(|| self.layout(window).pane)
    }

    pub(super) fn publish_views(&mut self, report: &Report) {
        self.clients.views.changed = false;
        for at in 0..self.clients.views.list.len() {
            let view = &self.clients.views.list[at];
            let info = self.view_info(view);
            let report = Report { view: Some(info), ..report.clone() };
            let view = &mut self.clients.views.list[at];
            if view.sent.as_ref() == Some(&report) {
                continue;
            }
            view.revision += 1;
            view.sent = Some(report.clone());
            let snapshot = Snapshot { revision: view.revision, report };
            let client = view.client;
            if let Some(text) = snapshot.streamed() {
                self.clients.events.push(client, Streamed::Snapshot(text));
            }
        }
    }
}
