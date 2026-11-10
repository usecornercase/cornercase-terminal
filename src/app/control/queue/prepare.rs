use std::collections::BTreeMap;

use crate::agents;
use crate::app::App;
use crate::config::Config;
use crate::control::QueueAdd;
use crate::git;
use crate::issues::Client;
use crate::issues::locate::Locator;
use crate::project::{Hierarchy, WorkspaceAt};
use crate::queue::{Entry, Place, Task};

impl App {
    pub(super) fn queue_issue(&self, project: u64, add: &QueueAdd) -> Result<Option<(Client, Locator)>, String> {
        add.issue
            .as_ref()
            .map(|text| self.locate_issue(project, text, add.source.as_deref()).map_err(String::from))
            .transpose()
    }

    pub(super) fn queue_agent(
        &mut self,
        caller: Option<u64>,
        add: &QueueAdd,
        old: Option<&Entry>,
    ) -> Result<Chosen, String> {
        let agent = old.filter(|entry| add.agent.as_ref().is_none_or(|agent| *agent == entry.task.agent)).map_or_else(
            || self.agent_kind(caller, add.agent.clone().filter(|agent| agent != agents::AUTO)),
            |entry| Ok(entry.task.agent.clone()),
        )?;
        let mode = queue_mode(&self.config, &agent, add, old);
        let preserve = old.filter(|entry| {
            entry.task.agent == agent && entry.task.mode == mode && add.agent.as_deref() != Some(agents::AUTO)
        });
        let command = match preserve {
            Some(entry) => entry.task.command.clone(),
            None => agents::start_line(
                &self.config,
                &agent,
                Some(mode.as_deref().unwrap_or(agents::DEFAULT_MODE)),
                None,
                None,
            )?,
        };
        let program =
            preserve.map_or_else(|| agents::command(&self.config, &agent), |entry| entry.task.program.clone());
        Ok(Chosen { agent, command, program, mode })
    }

    pub(super) fn queue_task(
        &mut self,
        caller: Option<u64>,
        add: &QueueAdd,
        old: Option<&Entry>,
    ) -> Result<Task, String> {
        let Chosen { agent, command, program, mode } = self.queue_agent(caller, add, old)?;
        let place = self.queue_target(caller, add, old)?;
        let prompt = add.prompt.clone().or_else(|| old.map(|entry| entry.task.prompt.clone())).unwrap_or_default();
        Ok(Task {
            zone: queue_zone(add, old),
            label: queue_label(add, &prompt, old),
            agent,
            command,
            program,
            mode,
            prompt,
            submit: add.submit.unwrap_or_else(|| old.is_none_or(|entry| entry.task.submit)),
            place,
            accounts: merged_accounts(add, old),
            ..Task::default()
        })
    }

    pub(super) fn queue_workspace(&mut self, caller: Option<u64>, add: &QueueAdd) -> Result<WorkspaceAt, String> {
        if let Some(workspace) = add.workspace {
            return Ok(self.usable_workspace(workspace)?);
        }
        if let Some(project) = add.project {
            let p = self.projects.project_index(project).ok_or_else(|| super::super::none("project", project))?;
            return Ok(self.ensure_workspace(p));
        }
        self.here_workspace(caller)
    }

    fn queue_target(&mut self, caller: Option<u64>, add: &QueueAdd, old: Option<&Entry>) -> Result<Place, String> {
        if add.workspace.is_none()
            && add.project.is_none()
            && add.worktree.is_none()
            && let Some(old) = old
        {
            return Ok(old.task.place.clone());
        }
        let at = self.queue_workspace(caller, add)?;
        let (project, found) = (&self.projects[at.project()], self.projects.workspace(at));
        let branch = add.worktree.clone();
        if let Some(branch) = &branch {
            if branch.trim().is_empty() {
                return Err("the worktree branch needs a name".into());
            }
            if !git::is_repo_root(&project.path) {
                return Err("a queued worktree needs a repository root".into());
            }
        }
        let workspace = if branch.is_some()
            || (add.workspace.is_none()
                && (add.project.is_some() || (add.issue.is_some() && git::is_repo_root(&project.path))))
        {
            None
        } else {
            Some(found.path.clone())
        };
        let workspace_name = workspace.as_ref().and_then(|_| found.name.clone());
        let workspace_id = workspace.as_ref().map(|_| found.id);
        Ok(Place {
            project: project.path.clone(),
            workspace,
            branch,
            workspace_name,
            workspace_id,
            server: crate::control::server_token().into(),
        })
    }
}

#[derive(Default)]
pub(super) struct Chosen {
    pub(super) agent: String,
    pub(super) command: String,
    pub(super) program: String,
    pub(super) mode: Option<String>,
}

pub(super) fn queue_mode(config: &Config, agent: &str, add: &QueueAdd, old: Option<&Entry>) -> Option<String> {
    let chosen = || agents::mode_of(agent, &agents::args(config, agent), &agents::modes(config, agent));
    match add.mode.as_deref() {
        Some(agents::DEFAULT_MODE) => None,
        Some(mode) => Some(mode.to_string()),
        None => old.filter(|entry| entry.task.agent == agent).map_or_else(chosen, |entry| entry.task.mode.clone()),
    }
}

pub(super) fn queue_label(add: &QueueAdd, prompt: &str, old: Option<&Entry>) -> String {
    let mut label = add.label.clone().unwrap_or_else(|| prompt.lines().next().unwrap_or_default().trim().to_string());
    if add.label.is_none()
        && add.issue.is_none()
        && let Some(old) = old.filter(|entry| entry.task.prompt == prompt)
    {
        label.clone_from(&old.task.label);
    }
    crate::text::one_line(&label.chars().filter(|character| !character.is_control()).collect::<String>())
}

pub(super) fn queue_zone(add: &QueueAdd, old: Option<&Entry>) -> String {
    if add.zone.is_empty() {
        old.map_or_else(crate::queue::clock::local_zone, |entry| entry.task.zone.clone())
    } else {
        add.zone.clone()
    }
}

pub(super) fn merged_accounts(add: &QueueAdd, old: Option<&Entry>) -> BTreeMap<String, String> {
    old.map(|entry| entry.task.accounts.clone()).unwrap_or_default().into_iter().chain(add.accounts.clone()).collect()
}
