use std::collections::HashSet;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use super::diff::Diff;
use super::{Body, Checkout, Model, Panel};
use crate::git;
use crate::project::Project;

#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    pub id: u64,
    pub label: String,
    pub files: Range<usize>,
    pub workspace: bool,
    pub folded: bool,
}

pub fn targets(projects: &[Project], order: &[usize], panel: &Panel) -> Vec<(Checkout, bool)> {
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

pub fn group(
    projects: &[Project],
    order: &[usize],
    panel: &Panel,
    targets: &[(Checkout, bool)],
    home: Option<&Path>,
) -> (Body, Vec<u64>, Vec<Header>) {
    let mut diff = Diff::default();
    let mut owners = Vec::new();
    let mut headers = Vec::new();
    let mut error = None;
    let workspaces: HashSet<_> = targets.iter().map(|(t, _)| t.workspace).collect();
    for &p in order {
        let project = &projects[p];
        if error.is_none() {
            error = project.workspaces.iter().filter(|w| workspaces.contains(&w.id)).find_map(|w| {
                let failure = panel.summary(w.id)?.result.as_ref().err()?;
                Some(format!("{}: {failure}", project.label(home)))
            });
        }
        let workspaces: Vec<_> = project
            .workspaces
            .iter()
            .filter(|w| workspaces.contains(&w.id))
            .filter_map(|w| {
                let diff = panel.summary(w.id)?.diff()?;
                (!diff.files.is_empty()).then_some((w, diff))
            })
            .collect();
        if workspaces.is_empty() {
            continue;
        }
        let start = diff.files.len();
        let count = workspaces.iter().map(|(_, d)| d.files.len()).sum::<usize>();
        headers.push(Header {
            id: project.id,
            label: project.label(home),
            files: start..start + count,
            workspace: false,
            folded: panel.headers.contains(&project.id),
        });
        let several = workspaces.len() > 1;
        for (workspace, changes) in workspaces {
            let start = diff.files.len();
            if several {
                headers.push(Header {
                    id: workspace.id,
                    label: workspace.label(),
                    files: start..start + changes.files.len(),
                    workspace: true,
                    folded: panel.headers.contains(&workspace.id),
                });
            }
            owners.extend(std::iter::repeat_n(workspace.id, changes.files.len()));
            diff.files.extend(changes.files.iter().cloned());
        }
    }
    let body = if diff.files.is_empty() && targets.iter().any(|(t, _)| panel.summary(t.workspace).is_none()) {
        Body::Loading
    } else if diff.files.is_empty() && error.is_some() {
        Body::Failed(error.unwrap_or_default())
    } else {
        Body::Ready(Arc::new(diff))
    };
    (body, owners, headers)
}
