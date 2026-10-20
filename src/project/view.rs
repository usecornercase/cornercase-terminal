use super::{Project, Workspace};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Shown {
    pub(crate) project: Option<u64>,
    pub(crate) workspace: Option<u64>,
    pub(crate) tab: Option<u64>,
    at: [usize; 3],
}

fn first_open(project: &Project) -> Option<usize> {
    let active = project.workspaces.get(project.active).filter(|w| w.open()).map(|_| project.active);
    active.or_else(|| project.workspaces.iter().position(Workspace::open))
}

fn nearest(len: usize, at: usize, usable: impl Fn(usize) -> bool) -> Option<usize> {
    (0..at.min(len)).rev().find(|&i| usable(i)).or_else(|| (at..len).find(|&i| usable(i)))
}

impl Shown {
    pub(crate) fn place(projects: &[Project], p: usize, w: Option<usize>, t: Option<usize>) -> Self {
        let Some(project) = projects.get(p) else { return Self::default() };
        let w = w.or_else(|| first_open(project)).filter(|&w| w < project.workspaces.len());
        let workspace = w.map(|w| &project.workspaces[w]);
        let t = workspace.and_then(|ws| t.or(Some(ws.active)).filter(|&t| t < ws.tabs.len()));
        Self {
            project: Some(project.id),
            workspace: workspace.map(|ws| ws.id),
            tab: workspace.zip(t).map(|(ws, t)| ws.tabs[t].id),
            at: [p, w.unwrap_or_default(), t.unwrap_or_default()],
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.project.is_none()
    }

    fn key(&self) -> (Option<u64>, Option<u64>, Option<u64>) {
        (self.project, self.workspace, self.tab)
    }

    pub(crate) fn settle(&mut self, projects: &[Project], window: impl FnOnce() -> Self) -> bool {
        let before = self.key();
        if self.project.is_none() {
            let adopted = window();
            if !adopted.is_empty() {
                *self = adopted;
            }
            return before != self.key();
        }
        let Some((p, moved)) = self.settled_project(projects) else {
            *self = Self::default();
            return before != (None, None, None);
        };
        let project = &projects[p];
        let (w, moved) = self.settled_workspace(project, moved);
        let Some(w) = w else {
            *self = Self { project: Some(project.id), at: [p, 0, 0], ..Self::default() };
            return before != (self.project, None, None);
        };
        let workspace = &project.workspaces[w];
        let t = self.settled_tab(workspace, moved);
        *self = Self {
            project: Some(project.id),
            workspace: Some(workspace.id),
            tab: t.map(|t| workspace.tabs[t].id),
            at: [p, w, t.unwrap_or_default()],
        };
        before != self.key()
    }

    fn settled_project(&self, projects: &[Project]) -> Option<(usize, bool)> {
        let usable = |p: usize| !projects[p].closing;
        let found = self.project.and_then(|id| projects.iter().position(|p| p.id == id)).filter(|&p| usable(p));
        let nearer = || nearest(projects.len(), self.at[0], usable);
        found.map(|p| (p, false)).or_else(|| nearer().map(|p| (p, true)))
    }

    fn settled_workspace(&self, project: &Project, moved: bool) -> (Option<usize>, bool) {
        if moved {
            return (first_open(project), true);
        }
        let found = self.workspace.and_then(|id| project.index_of(id)).filter(|&w| project.workspaces[w].open());
        match found {
            Some(w) => (Some(w), false),
            None if self.workspace.is_none() => (first_open(project), false),
            None => (nearest(project.workspaces.len(), self.at[1], |w| project.workspaces[w].open()), true),
        }
    }

    fn settled_tab(&self, workspace: &Workspace, moved: bool) -> Option<usize> {
        match self.tab.and_then(|id| workspace.index_of(id)) {
            Some(t) => Some(t),
            None if moved || self.tab.is_none() => Some(workspace.active).filter(|&t| t < workspace.tabs.len()),
            None => {
                let t = if self.at[2] > 0 { self.at[2] - 1 } else { 0 };
                (!workspace.tabs.is_empty()).then(|| t.min(workspace.tabs.len() - 1))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Claim {
    pub(crate) view: u64,
    pub(crate) tab: u64,
    pub(crate) seq: u64,
}

pub(crate) fn controller(tab: u64, window: Option<(u64, u64)>, claims: impl IntoIterator<Item = Claim>) -> Option<u64> {
    let held = window.filter(|(on, _)| *on == tab).map_or(0, |(_, seq)| seq);
    claims
        .into_iter()
        .filter(|claim| claim.tab == tab && claim.seq > held)
        .max_by_key(|claim| claim.seq)
        .map(|c| c.view)
}

#[cfg(test)]
mod tests;
