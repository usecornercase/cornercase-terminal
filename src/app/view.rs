use std::time::Instant;

use ratatui::layout::Rect;

use super::App;
use super::interaction::{Drag, Grab};
use super::layout::{List, Side};
use super::links::Resolving;
use super::toast::Toast;
use crate::accounts;
use crate::agents::context::Context;
use crate::agents::{Known, activity};
use crate::config::Config;
use crate::project::{Hierarchy, Project, Tab, Workspace};
use crate::term::Term;
use crate::ui::{self};

impl App {
    pub(super) fn project_entries(&self) -> Vec<ui::ProjectEntry> {
        self.projects
            .iter()
            .zip(crate::project::group_indices(&self.projects, &self.groups))
            .map(|(p, group)| ui::ProjectEntry {
                name: self.project_label(p),
                workspaces: p.workspaces.len(),
                group,
                status: activity::attention(p.workspaces.iter().flat_map(|w| &w.tabs).map(Tab::status)),
            })
            .collect()
    }

    pub(super) fn tab_details(&self, tab: &Tab) -> ui::Details {
        let account = tab
            .pane()
            .and_then(|t| self.pane_account(t))
            .or_else(|| tab.panes.iter().find_map(|t| self.pane_account(t)));
        self.details(tab.context(), tab.memory(), tab.subagents(), account)
    }

    pub(super) fn account_label(&self, kind: Known, name: Option<&str>) -> Option<String> {
        let listed = self.listed_accounts().iter().any(|(listed, names)| *listed == kind && !names.is_empty());
        label(&self.config, kind, name, listed)
    }

    pub(super) fn pane_account(&self, term: &Term) -> Option<String> {
        term.agent_account().and_then(|(kind, name)| self.account_label(kind, name))
    }

    pub(super) fn details(
        &self,
        context: Option<&Context>,
        memory: Option<u64>,
        subagents: usize,
        account: Option<String>,
    ) -> ui::Details {
        ui::Details {
            model: context.filter(|_| self.config.model).map(|c| c.model.clone()),
            account: account.filter(|_| self.config.account_mark && self.has_accounts()),
            percent: context.and_then(|c| c.percent).filter(|_| self.config.context),
            subagents,
            memory: memory.filter(|_| self.config.memory),
        }
    }

    pub(super) fn tab_entry(&self, t: &Tab) -> ui::TabEntry {
        ui::TabEntry {
            name: t.label(&self.config),
            status: t.status(),
            running: t.running(),
            details: self.tab_details(t),
            others: t.others(),
        }
    }

    pub(super) fn workspace_entry(&self, w: &Workspace) -> ui::WorkspaceEntry {
        ui::WorkspaceEntry {
            name: w.label(),
            collapsed: w.collapsed,
            tabs: w.tabs.iter().map(|t| self.tab_entry(t)).collect(),
            behind: w.behind,
            pull: self.pull_mark(w),
            removing: w.removing(),
            setting_up: w.setting_up(),
        }
    }

    pub(super) fn tree_view(&self) -> ui::TreeView {
        let shape = self.tree_shape();
        let rows = ui::tree_rows(&shape);
        let (above, below) =
            ui::tree_layout_rows(self.screen.drawn.list, &shape, &rows, self.scroll.of(List::Projects)).hidden();
        let shown = &rows[above.end..below.start];
        let open = self.tree_open();
        let workspaces = self.projects.iter().enumerate().map(|(p, project)| {
            let entries = project.workspaces.iter().enumerate();
            if open[p] {
                entries.map(|(w, workspace)| self.tree_entry(p, w, workspace, shown)).collect()
            } else {
                Vec::new()
            }
        });
        ui::TreeView { workspaces: workspaces.collect(), shape }
    }

    fn tree_entry(&self, p: usize, w: usize, workspace: &Workspace, shown: &[ui::TreeRow]) -> ui::WorkspaceEntry {
        let tab = |(t, tab): (usize, &Tab)| {
            if shown.contains(&ui::TreeRow::Tab(p, w, t)) {
                self.tab_entry(tab)
            } else {
                ui::TabEntry { status: tab.status(), ..ui::TabEntry::from("") }
            }
        };
        let named = shown.contains(&ui::TreeRow::Workspace(p, w));
        ui::WorkspaceEntry {
            name: if named { workspace.label() } else { String::new() },
            tabs: workspace.tabs.iter().enumerate().map(tab).collect(),
            behind: workspace.behind,
            pull: self.pull_mark(workspace),
            removing: workspace.removing(),
            collapsed: workspace.collapsed,
            setting_up: workspace.setting_up(),
        }
    }

    pub(super) fn tab_view(&mut self, area: Rect) -> Option<ui::TabView> {
        let dragging = self.screen.drag.as_ref().and_then(Drag::divider);
        let pane_area = self.layout(area).shown(self.screen.nav).pane;
        let landing = self.pane_landing(pane_area);
        let pointer = self.link_hover();
        let root = self.project().and_then(Project::workspace).map(|w| w.path.clone());
        let (home, bands) = (self.home.clone(), self.pane_bands());
        let dim_inactive = self.config.dim_inactive_panes;
        self.tab_mut().and_then(|tab| {
            let layout = tab.layout_by_index()?;
            let screens: Vec<_> = tab.panes.iter_mut().map(|t| t.emulator.snapshot().unwrap_or_default()).collect();
            let dragging = dragging.filter(|(id, _)| *id == tab.id).map(|(_, path)| path);
            let link = pointer.zip(root.as_deref()).and_then(|(at, root)| {
                Self::hovered_link(tab, &screens, pane_area, at, Resolving { root, home: home.as_deref() })
            });
            let bands = tab.panes.iter().map(|t| bands.get(&t.id).cloned()).collect();
            Some(ui::TabView { layout, screens, active: tab.active, dim_inactive, dragging, link, landing, bands })
        })
    }

    pub(super) fn lists_view(&self, area: Rect, now: Instant) -> ui::View<'_> {
        let project = self.project();
        let workspaces = project
            .filter(|_| !self.screen.drawn.tree)
            .map(|p| p.workspaces.iter().map(|w| self.workspace_entry(w)).collect());
        ui::View {
            tree: self.screen.drawn.tree.then(|| self.tree_view()),
            agents: self.config.agents_section.then(|| self.agents_view()),
            counts: self.config.counts,
            icons: self.config.icon_bar,
            groups: self.groups.iter().map(|g| g.entry.clone()).collect(),
            projects: self.project_entries(),
            active: self.active,
            projects_scroll: self.scroll.of(List::Projects),
            has_project: project.is_some(),
            workspaces: workspaces.unwrap_or_default(),
            active_workspace: project.map_or(0, |p| p.active),
            active_tab: project.and_then(Project::workspace).filter(|w| !w.tabs.is_empty()).map(|w| w.active),
            workspaces_scroll: self.scroll.of(List::Workspaces),
            issues: self.issues_available(),
            ..self.chrome_view(area, now)
        }
    }

    fn chrome_view(&self, area: Rect, now: Instant) -> ui::View<'_> {
        let visible = self.visible_tab();
        let attention = activity::attention(self.projects.tabs().filter(|t| Some(t.id) != visible).map(Tab::status));
        ui::View {
            hover: self.screen.hover.filter(|_| self.screen.drag.as_ref().is_none_or(Drag::shows_hover)),
            widths: self.screen.widths,
            sidebar: self.sidebar(),
            resizing: self.screen.drag.as_ref().and_then(Drag::border),
            light: self.themes.light(),
            muted: ui::muted(self.themes.host()),
            soft: ui::soft(self.themes.host()),
            toast: self.messages.toast.as_ref().map(Toast::view),
            star: self.messages.star.shows_card(),
            nav: self.screen.nav,
            update: self.update_label(),
            remote: self.remote_button(),
            accounts: self.account_pills(),
            attention,
            spin: self.spin(now),
            drag: self.row_drag_view(area),
            tab_bar: (!self.screen.drawn.tab_bar.is_empty()).then(|| self.tab_bar_view()),
            ..self.panels_view(area, now)
        }
    }

    fn panels_view(&self, area: Rect, now: Instant) -> ui::View<'_> {
        ui::View {
            changes: if self.changes_shown() { self.panel_view() } else { None },
            changes_button: self.changes_button_view(),
            todo: (self.screen.side == Some(Side::Todo))
                .then(|| self.todo_view(self.layout(area).shown(self.screen.nav).changes)),
            flow: (self.screen.side == Some(Side::Flow)).then(|| self.flow_view(now, self.spin(now))).flatten(),
            flow_run: self.has_run(),
            todo_dot: self.config.todo_dot,
            todo_pending: self.panels.todos.pending() > 0,
            ..ui::View::default()
        }
    }

    fn spin(&self, now: Instant) -> Option<u8> {
        let elapsed = self.clock.since_boot(now);
        self.config.spinner().then(|| if self.agent_working() { activity::spin_frame(elapsed) } else { 0 })
    }

    fn row_drag_view(&self, area: Rect) -> Option<ui::Drag> {
        let moved = self.row_drag().filter(|d| d.moved).zip(self.screen.hover);
        moved.and_then(|(d, pos)| match d.target {
            Grab::Row(target) => self.drag_view(target, pos, area),
            Grab::Todo(id) => Some(ui::Drag::Todo(id)),
            Grab::Agent(_) => None,
        })
    }
}

pub(super) fn label(config: &Config, kind: Known, name: Option<&str>, listed: bool) -> Option<String> {
    match name {
        Some(accounts::CUSTOM) => Some(accounts::CUSTOM.to_string()),
        Some(name) => Some(config.account_label(kind.name(), name).to_string()),
        None => listed.then(|| config.account_label(kind.name(), "").to_string()),
    }
}
