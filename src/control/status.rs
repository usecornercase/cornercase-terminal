use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{ConfigInfo, Ids, Layout, ThemeInfo, ViewInfo};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Report {
    pub(crate) version: String,
    pub(crate) boot: String,
    pub caller: Option<u64>,
    pub shown: Ids,
    pub(crate) groups: Vec<GroupInfo>,
    pub projects: Vec<ProjectInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) keep_awake: Option<KeepAwake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) view: Option<ViewInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) theme: Option<ThemeInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) config: Option<ConfigInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) home: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct KeepAwake {
    pub(crate) held: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) unavailable: Option<String>,
}

impl Report {
    pub(crate) fn workspace_on(&self, branch: &str) -> Result<u64, String> {
        let project = self.here().ok_or("no project is open; open one with `cornercase open PATH`")?;
        let on = |w: &&WorkspaceInfo| w.branch.as_deref() == Some(branch);
        let found: Vec<&WorkspaceInfo> = project.workspaces.iter().filter(on).collect();
        let place = format!("project {} ({})", project.id, project.name);
        match found[..] {
            [workspace] => Ok(workspace.id),
            [] if project.workspaces.is_empty() => {
                Err(format!("{place} has no workspace, so none is on branch {branch}"))
            }
            [] => Err(format!(
                "no workspace of {place} is on branch {branch}; its workspaces:{}",
                candidates(&project.workspaces)
            )),
            _ => Err(format!(
                "branch {branch} is checked out in {} workspaces of {place}; pick one with --workspace:{}",
                found.len(),
                candidates(found.iter().copied())
            )),
        }
    }

    fn here(&self) -> Option<&ProjectInfo> {
        let panes =
            |p: &ProjectInfo| p.workspaces.iter().flat_map(|w| &w.tabs).flat_map(|t| &t.panes).any(|pane| pane.caller);
        let shown = |p: &ProjectInfo| Some(p.id) == self.shown.project;
        self.projects.iter().find(|p| panes(p)).or_else(|| self.projects.iter().find(|p| shown(p)))
    }

    pub(crate) fn loose(&self) -> impl Iterator<Item = &ProjectInfo> {
        let grouped = |p: &ProjectInfo| p.group.is_some_and(|g| self.groups.iter().any(|group| group.id == g));
        self.projects.iter().filter(move |p| !grouped(p))
    }

    pub(crate) fn grouped(&self, group: u64) -> impl Iterator<Item = &ProjectInfo> {
        self.projects.iter().filter(move |p| p.group == Some(group))
    }

    pub fn panes(&self) -> Vec<PaneRow> {
        let projects = self.loose().chain(self.groups.iter().flat_map(|group| self.grouped(group.id)));
        let tabs = projects.flat_map(|project| {
            project
                .workspaces
                .iter()
                .flat_map(move |workspace| workspace.tabs.iter().map(move |tab| (project, workspace, tab)))
        });
        let rows = tabs.flat_map(|(project, workspace, tab)| {
            tab.panes.iter().map(move |pane| self.pane_row(project, workspace, tab, pane))
        });
        rows.collect()
    }

    fn pane_row(&self, project: &ProjectInfo, workspace: &WorkspaceInfo, tab: &TabInfo, pane: &PaneInfo) -> PaneRow {
        PaneRow {
            pane: pane.id,
            tab: tab.id,
            workspace: workspace.id,
            project: project.id,
            program: pane.program.clone(),
            agent: pane.agent.clone(),
            status: pane.status.clone(),
            background_shell: pane.background_shell,
            dialog: pane.dialog,
            at_prompt: pane.at_prompt,
            model: pane.model.clone(),
            context: pane.context,
            caller: pane.caller,
            shown: self.shown.pane == Some(pane.id),
            active: pane.active,
            tab_name: tab.name.clone(),
            workspace_name: workspace.name.clone(),
            branch: workspace.branch.clone(),
            worktree: workspace.worktree,
            project_name: project.name.clone(),
            path: pane.path.clone(),
            survey: pane.survey,
            resuming: pane.resuming,
            parent: pane.parent,
            subagents: pane.subagents,
            role: pane.role.clone(),
            held_sends: pane.held_sends,
            draft: pane.draft,
            account_restart: pane.account_restart.clone(),
            pid: pane.pid,
            memory: pane.memory,
            compacted: pane.compacted,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaneRows {
    pub panes: Vec<PaneRow>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(clippy::struct_excessive_bools, reason = "each flag is its own key in the JSON of `status --panes`")]
pub struct PaneRow {
    pub pane: u64,
    pub tab: u64,
    pub(crate) workspace: u64,
    pub(crate) project: u64,
    pub(crate) program: Option<String>,
    pub(crate) agent: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) background_shell: bool,
    pub(crate) dialog: bool,
    pub(crate) at_prompt: Option<bool>,
    pub(crate) model: Option<String>,
    pub(crate) context: Option<u16>,
    pub caller: bool,
    pub(crate) shown: bool,
    pub(crate) active: bool,
    pub(crate) tab_name: String,
    pub(crate) workspace_name: String,
    pub(crate) branch: Option<String>,
    pub(crate) worktree: bool,
    pub(crate) project_name: String,
    pub(crate) path: Option<PathBuf>,
    pub(crate) survey: bool,
    pub(crate) resuming: bool,
    pub(crate) parent: Option<u64>,
    pub(crate) subagents: u32,
    pub(crate) role: Option<String>,
    pub(crate) held_sends: u32,
    pub(crate) draft: bool,
    pub(crate) account_restart: Option<String>,
    pub(crate) pid: Option<i32>,
    pub(crate) memory: Option<u64>,
    pub(crate) compacted: bool,
}

impl PaneRow {
    pub const COLUMNS: [&str; 32] = [
        "pane",
        "tab",
        "workspace",
        "project",
        "program",
        "agent",
        "status",
        "background_shell",
        "dialog",
        "at_prompt",
        "model",
        "context",
        "caller",
        "shown",
        "active",
        "tab_name",
        "workspace_name",
        "branch",
        "worktree",
        "project_name",
        "path",
        "survey",
        "resuming",
        "parent",
        "subagents",
        "role",
        "held_sends",
        "draft",
        "account_restart",
        "pid",
        "memory",
        "compacted",
    ];
}

fn candidates<'a>(workspaces: impl IntoIterator<Item = &'a WorkspaceInfo>) -> String {
    let mut text = String::new();
    for workspace in workspaces {
        let _ = write!(text, "\n  workspace {}  {}", workspace.id, workspace.name);
        if let Some(branch) = &workspace.branch {
            let _ = write!(text, "  branch {branch}");
        }
        if workspace.worktree {
            text.push_str("  worktree");
        }
    }
    text
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct GroupInfo {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) collapsed: bool,
    pub(crate) icon: String,
    pub(crate) colour: u8,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectInfo {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub path: PathBuf,
    pub(crate) group: Option<u64>,
    pub(crate) active: bool,
    pub workspaces: Vec<WorkspaceInfo>,
    pub(crate) custom_name: Option<String>,
    pub(crate) collapsed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceInfo {
    pub id: u64,
    pub name: String,
    pub(crate) path: PathBuf,
    pub(crate) branch: Option<String>,
    pub(crate) worktree: bool,
    pub(crate) behind: u32,
    pub(crate) pull_request: Option<PullRequestInfo>,
    pub(crate) setup: Option<String>,
    pub(crate) run_dir: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) run: Option<RunStatus>,
    pub(crate) active: bool,
    pub tabs: Vec<TabInfo>,
    pub(crate) custom_name: Option<String>,
    pub(crate) collapsed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) changes: Option<super::ChangesCount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) closed_agents: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) post: Option<PostInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PostInfo {
    pub(crate) url: String,
    pub(crate) archived: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct RunStatus {
    pub(crate) end: String,
    pub(crate) reached: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PullRequestInfo {
    pub(crate) number: u64,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) state: String,
    pub(crate) checks: Option<String>,
    pub(crate) failed: Vec<String>,
    pub(crate) conflicts: bool,
    pub(crate) base: Option<String>,
    pub(crate) provider: String,
    pub(crate) stack: Option<StackInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct StackInfo {
    pub(crate) number: u64,
    pub(crate) base: String,
    pub(crate) position: usize,
    pub(crate) size: usize,
    pub(crate) layers: Vec<LayerInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct LayerInfo {
    pub(crate) number: u64,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) state: String,
    pub(crate) checks: Option<String>,
    pub(crate) conflicts: bool,
    pub(crate) review: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TabInfo {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) status: Option<String>,
    pub(crate) active: bool,
    pub panes: Vec<PaneInfo>,
    pub(crate) custom_name: Option<String>,
    pub(crate) layout: Option<Layout>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) controller: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(clippy::struct_excessive_bools, reason = "each flag is its own key in the JSON of `status`")]
pub struct PaneInfo {
    pub id: u64,
    pub(crate) path: Option<PathBuf>,
    pub program: Option<String>,
    pub(crate) agent: Option<String>,
    pub status: Option<String>,
    pub(crate) background_shell: bool,
    pub(crate) dialog: bool,
    pub(crate) survey: bool,
    pub(crate) at_prompt: Option<bool>,
    pub(crate) model: Option<String>,
    pub(crate) context: Option<u16>,
    pub(crate) active: bool,
    pub(crate) caller: bool,
    pub(crate) resumes: bool,
    pub(crate) resuming: bool,
    pub(crate) title: Option<String>,
    pub(crate) parent: Option<u64>,
    pub(crate) subagents: u32,
    pub(crate) role: Option<String>,
    pub(crate) held_sends: u32,
    pub(crate) draft: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) collapsed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) account: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) accounts: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) account_restart: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pid: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) memory: Option<u64>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) compacted: bool,
}
