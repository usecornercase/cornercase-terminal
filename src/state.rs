use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::agents::AgentState;
use crate::changes::{Mode, Scope};
use crate::issues::People;
use crate::project::group::GroupEntry;
use crate::project::split::Node;
use crate::project::{Project, Tab, Workspace};
use crate::term::Term;

pub(crate) const VERSION: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Widths {
    pub(crate) projects: u16,
    pub(crate) workspaces: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) changes: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) stack: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) agents: Option<u16>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct State {
    pub(crate) version: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) groups: Vec<GroupEntry>,
    pub(crate) projects: Vec<ProjectState>,
    pub(crate) active: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) widths: Option<Widths>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) issues: Option<IssuesState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) changes: Option<ChangesState>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) todo: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) files: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChangesState {
    #[serde(default)]
    pub(crate) open: bool,
    #[serde(default)]
    pub(crate) mode: Mode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) scope: Option<Scope>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct IssuesState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tab: Option<String>,
    #[serde(default)]
    pub(crate) closed: bool,
    #[serde(default)]
    pub(crate) people: People,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ProjectState {
    pub(crate) path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) group: Option<usize>,
    pub(crate) workspaces: Vec<WorkspaceState>,
    #[serde(default)]
    pub(crate) active: usize,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) collapsed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct WorkspaceState {
    pub(crate) path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) worktree: bool,
    pub(crate) tabs: Vec<TabState>,
    #[serde(default)]
    pub(crate) active: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) base: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) collapsed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct TabState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,
    pub(crate) panes: Vec<PaneState>,
    #[serde(default)]
    pub(crate) active: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) layout: Option<Node<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PaneState {
    pub(crate) cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) right_clicks: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) agent: Option<AgentState>,
}

impl WorkspaceState {
    pub(crate) fn plain(path: PathBuf, cwds: Vec<Option<PathBuf>>, active: usize) -> Self {
        let tabs = cwds.into_iter().map(|cwd| TabState {
            name: None,
            panes: vec![PaneState { cwd, right_clicks: false, agent: None }],
            active: 0,
            layout: None,
        });
        Self { path, name: None, worktree: false, tabs: tabs.collect(), active, base: None, collapsed: false }
    }
}

impl ProjectState {
    pub(crate) fn of(project: &Project, group: Option<usize>) -> Self {
        Self {
            path: project.path.clone(),
            name: project.name.clone(),
            group,
            workspaces: project.workspaces.iter().map(WorkspaceState::of).collect(),
            active: project.active,
            collapsed: project.collapsed,
        }
    }
}

impl ProjectState {
    pub(crate) fn restore(&self, id: u64, path: PathBuf, group: Option<u64>, workspaces: Vec<Workspace>) -> Project {
        let mut project = Project::new(id, path, self.name.clone());
        project.group = group;
        project.collapsed = self.collapsed;
        project.active = self.active.min(workspaces.len().saturating_sub(1));
        project.workspaces = workspaces;
        project
    }
}

impl WorkspaceState {
    pub(crate) fn restore(&self, id: u64, path: PathBuf, tabs: Vec<Tab>) -> Workspace {
        let mut workspace = Workspace::new(id, path, self.name.clone(), self.worktree);
        workspace.base.clone_from(&self.base);
        workspace.collapsed = self.collapsed;
        workspace.active = self.active.min(tabs.len().saturating_sub(1));
        workspace.tabs = tabs;
        workspace
    }
}

impl TabState {
    pub(crate) fn restore(&self, id: u64, panes: Vec<Term>) -> Tab {
        let mut tab = Tab::restored(id, self.name.clone(), panes, self.layout.as_ref());
        tab.active = self.active.min(tab.panes.len() - 1);
        tab.right_clicks =
            self.panes.iter().zip(&tab.panes).filter(|(s, _)| s.right_clicks).map(|(_, t)| t.id).collect();
        tab
    }
}

impl WorkspaceState {
    pub(crate) fn of(workspace: &Workspace) -> Self {
        Self {
            path: workspace.path.clone(),
            name: workspace.name.clone(),
            worktree: workspace.worktree,
            base: workspace.base.clone(),
            tabs: workspace.tabs.iter().map(TabState::of).collect(),
            active: workspace.active,
            collapsed: workspace.collapsed,
        }
    }
}

impl TabState {
    pub(crate) fn of(tab: &Tab) -> Self {
        Self {
            name: tab.name.clone(),
            panes: tab
                .panes
                .iter()
                .map(|term| PaneState {
                    cwd: term.cwd(),
                    right_clicks: tab.right_clicks_to_pane(term.id),
                    agent: term.resume.clone(),
                })
                .collect(),
            active: tab.active,
            layout: (tab.panes.len() > 1).then(|| tab.layout_by_index()).flatten(),
        }
    }
}

impl State {
    fn folded(self) -> Self {
        let active = self.active;
        let projects = self.projects.into_iter().enumerate().map(|(i, p)| ProjectState { collapsed: i != active, ..p });
        Self { version: VERSION, projects: projects.collect(), ..self }
    }
}

#[derive(Deserialize)]
struct Versioned {
    version: u32,
}

#[derive(Deserialize)]
struct V2State {
    workspaces: Vec<V2Workspace>,
    active: usize,
}

#[derive(Deserialize)]
struct V2Workspace {
    path: PathBuf,
    #[serde(default)]
    name: Option<String>,
    terminals: Vec<PaneState>,
    #[serde(default)]
    active: usize,
}

impl From<V2State> for State {
    fn from(old: V2State) -> Self {
        let projects = old
            .workspaces
            .into_iter()
            .map(|w| {
                let cwds = w.terminals.into_iter().map(|t| t.cwd).collect();
                let workspace = WorkspaceState::plain(w.path.clone(), cwds, w.active);
                ProjectState {
                    path: w.path,
                    name: w.name,
                    group: None,
                    workspaces: vec![workspace],
                    active: 0,
                    collapsed: false,
                }
            })
            .collect();
        Self {
            version: VERSION,
            groups: Vec::new(),
            projects,
            active: old.active,
            widths: None,
            issues: None,
            changes: None,
            todo: false,
            files: false,
        }
    }
}

#[derive(Deserialize)]
struct V1State {
    terminals: Vec<V1Term>,
    active: usize,
}

#[derive(Deserialize)]
struct V1Term {
    cwd: Option<PathBuf>,
    #[serde(default)]
    name: Option<String>,
}

impl From<V1State> for V2State {
    fn from(old: V1State) -> Self {
        let mut active = 0;
        let mut workspaces = Vec::new();
        for (i, term) in old.terminals.into_iter().enumerate() {
            let Some(cwd) = term.cwd else { continue };
            if i <= old.active {
                active = workspaces.len();
            }
            let terminals = vec![PaneState { cwd: Some(cwd.clone()), right_clicks: false, agent: None }];
            workspaces.push(V2Workspace { path: cwd, name: term.name, terminals, active: 0 });
        }
        Self { workspaces, active }
    }
}

pub(crate) fn load(path: &Path) -> Option<State> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str::<Versioned>(&text).ok()?.version {
        VERSION => serde_json::from_str(&text).ok(),
        3 | 4 => serde_json::from_str::<State>(&text).ok().map(State::folded),
        2 => serde_json::from_str::<V2State>(&text).ok().map(|v2| State::from(v2).folded()),
        1 => serde_json::from_str::<V1State>(&text).ok().map(|v1| State::from(V2State::from(v1)).folded()),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
