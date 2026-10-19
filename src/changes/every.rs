use std::collections::HashSet;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use super::diff::Diff;
use super::{Body, Checkout, Model, Panel};
use crate::git;
use crate::project::{Project, Workspace};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Header {
    pub(crate) id: u64,
    pub(crate) label: String,
    pub(crate) files: Range<usize>,
    pub(crate) workspace: bool,
    pub(crate) folded: bool,
}

pub(crate) fn targets(projects: &[Project], order: &[usize], panel: &Panel) -> Vec<(Checkout, bool)> {
    order
        .iter()
        .flat_map(|&p| {
            let project = &projects[p];
            let workspaces: Vec<_> = project
                .workspaces
                .iter()
                .filter(|w| !project.closing && w.open() && git::branch(&w.path).is_some())
                .collect();
            let several = workspaces
                .iter()
                .filter(|w| panel.summary(w.id).and_then(Model::diff).is_some_and(|d| !d.files.is_empty()))
                .count()
                > 1;
            workspaces
                .into_iter()
                .map(|w| {
                    let hidden = panel.headers.contains(&project.id) || (several && panel.headers.contains(&w.id));
                    (Checkout { workspace: w.id, dir: w.path.clone(), base: None }, hidden)
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

type Changed<'a> = Vec<(&'a Workspace, &'a Arc<Diff>)>;

#[derive(Default)]
struct Grouped {
    diff: Diff,
    owners: Vec<u64>,
    headers: Vec<Header>,
    error: Option<String>,
}

impl Grouped {
    fn add(&mut self, project: &Project, changed: Changed, panel: &Panel, home: Option<&Path>) {
        let start = self.diff.files.len();
        let count = changed.iter().map(|(_, d)| d.files.len()).sum::<usize>();
        self.headers.push(Header {
            id: project.id,
            label: project.label(home),
            files: start..start + count,
            workspace: false,
            folded: panel.headers.contains(&project.id),
        });
        let several = changed.len() > 1;
        for (workspace, changes) in changed {
            let start = self.diff.files.len();
            if several {
                self.headers.push(Header {
                    id: workspace.id,
                    label: workspace.label(),
                    files: start..start + changes.files.len(),
                    workspace: true,
                    folded: panel.headers.contains(&workspace.id),
                });
            }
            self.owners.extend(std::iter::repeat_n(workspace.id, changes.files.len()));
            self.diff.files.extend(changes.files.iter().cloned());
        }
    }
}

fn first_failure(project: &Project, ids: &HashSet<u64>, panel: &Panel, home: Option<&Path>) -> Option<String> {
    project.workspaces.iter().filter(|w| ids.contains(&w.id)).find_map(|w| {
        let failure = panel.summary(w.id)?.result.as_ref().err()?;
        Some(format!("{}: {failure}", project.label(home)))
    })
}

fn changed_in<'a>(project: &'a Project, ids: &HashSet<u64>, panel: &'a Panel) -> Changed<'a> {
    project
        .workspaces
        .iter()
        .filter(|w| ids.contains(&w.id))
        .filter_map(|w| {
            let diff = panel.summary(w.id)?.diff()?;
            (!diff.files.is_empty()).then_some((w, diff))
        })
        .collect()
}

pub(crate) fn group(
    projects: &[Project],
    order: &[usize],
    panel: &Panel,
    targets: &[(Checkout, bool)],
    home: Option<&Path>,
) -> (Body, Vec<u64>, Vec<Header>) {
    let mut grouped = Grouped::default();
    let ids: HashSet<_> = targets.iter().map(|(t, _)| t.workspace).collect();
    for &p in order {
        let project = &projects[p];
        if grouped.error.is_none() {
            grouped.error = first_failure(project, &ids, panel, home);
        }
        let changed = changed_in(project, &ids, panel);
        if !changed.is_empty() {
            grouped.add(project, changed, panel, home);
        }
    }
    let Grouped { diff, owners, headers, error } = grouped;
    let body = if diff.files.is_empty() && targets.iter().any(|(t, _)| panel.summary(t.workspace).is_none()) {
        Body::Loading
    } else if diff.files.is_empty() && error.is_some() {
        Body::Failed(error.unwrap_or_default())
    } else {
        Body::Ready(Arc::new(diff))
    };
    (body, owners, headers)
}

#[cfg(test)]
mod tests;
