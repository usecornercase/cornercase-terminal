use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::changes::{Mode, Scope};
use crate::issues::People;
use crate::project::group::GroupEntry;
use crate::project::{Project, Tab, Workspace};
use crate::split::Node;

pub use crate::agents::AgentState;

pub const VERSION: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Widths {
    pub projects: u16,
    pub workspaces: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agents: Option<u16>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<GroupEntry>,
    pub projects: Vec<ProjectState>,
    pub active: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widths: Option<Widths>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issues: Option<IssuesState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<ChangesState>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub todo: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub files: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangesState {
    #[serde(default)]
    pub open: bool,
    #[serde(default)]
    pub mode: Mode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<Scope>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssuesState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab: Option<String>,
    #[serde(default)]
    pub closed: bool,
    #[serde(default)]
    pub people: People,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectState {
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<usize>,
    pub workspaces: Vec<WorkspaceState>,
    #[serde(default)]
    pub active: usize,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub collapsed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceState {
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub worktree: bool,
    pub tabs: Vec<TabState>,
    #[serde(default)]
    pub active: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub collapsed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub panes: Vec<PaneState>,
    #[serde(default)]
    pub active: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<Node<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneState {
    pub cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub right_clicks: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentState>,
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
    pub fn of(project: &Project, group: Option<usize>) -> Self {
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

impl WorkspaceState {
    pub fn of(workspace: &Workspace) -> Self {
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
    pub fn of(tab: &Tab) -> Self {
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

pub fn load(path: &Path) -> Option<State> {
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
