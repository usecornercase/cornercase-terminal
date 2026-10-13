use super::App;
use crate::agents::activity;
use crate::agents::context::Context;
use crate::project::{Tab, Workspace};
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
        self.details(tab.context(), tab.memory())
    }

    pub(super) fn details(&self, context: Option<&Context>, memory: Option<u64>) -> ui::Details {
        ui::Details {
            model: context.filter(|_| self.config.model).map(|c| c.model.clone()),
            percent: context.and_then(|c| c.percent).filter(|_| self.config.context),
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
            tabs: w.tabs.iter().map(|t| self.tab_entry(t)).collect(),
            behind: w.behind,
            removing: w.removing(),
        }
    }

    pub(super) fn tree_view(&self) -> ui::TreeView {
        let shape = self.tree_shape();
        let rows = ui::tree_rows(&shape);
        let (above, below) = ui::tree_layout_rows(self.drawn.list, &shape, &rows, self.scroll.projects).hidden();
        let shown = &rows[above.end..below.start];
        let open = self.tree_open();
        let workspaces = self.projects.iter().enumerate().map(|(p, project)| {
            let entry = |(w, workspace): (usize, &Workspace)| {
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
                    removing: workspace.removing(),
                }
            };
            if open[p] { project.workspaces.iter().enumerate().map(entry).collect() } else { Vec::new() }
        });
        ui::TreeView { workspaces: workspaces.collect(), shape }
    }
}
