use crate::agents;
use crate::app::App;
use crate::control::QueueAdd;
use crate::git;
use crate::issues::{Client, Source};
use crate::project::Hierarchy;
use crate::queue::{Entry, Place, Task};

impl App {
    pub(super) fn queue_issue(&self, project: u64, add: &QueueAdd) -> Result<Option<(Client, String)>, String> {
        add.issue
            .as_ref()
            .map(|text| {
                let connected: Vec<_> =
                    Source::REMOTE.into_iter().filter(|source| self.client(*source, project).is_some()).collect();
                let (source, key) = crate::queue::issue::locator(text, add.source.as_deref(), &connected)
                    .map_err(|error| error.to_string())?;
                let api = self.client(source, project).ok_or_else(|| format!("{} is not connected", source.name()))?;
                Ok((api, key))
            })
            .transpose()
    }

    pub(super) fn queue_task(
        &mut self,
        caller: Option<u64>,
        add: &QueueAdd,
        old: Option<&Entry>,
    ) -> Result<Task, String> {
        let agent = old.filter(|entry| add.agent.as_ref().is_none_or(|agent| *agent == entry.task.agent)).map_or_else(
            || self.agent_kind(caller, add.agent.clone().filter(|agent| agent != agents::AUTO)),
            |entry| Ok(entry.task.agent.clone()),
        )?;
        let mut config = self.config.clone();
        let mode = match add.mode.as_deref() {
            Some("default") => agents::mode_of(&agent, &agents::args(&config, &agent), &agents::modes(&config, &agent)),
            Some(mode) => Some(mode.to_string()),
            None => old.filter(|entry| entry.task.agent == agent).map_or_else(
                || agents::mode_of(&agent, &agents::args(&config, &agent), &agents::modes(&config, &agent)),
                |entry| entry.task.mode.clone(),
            ),
        };
        let preserve = old.filter(|entry| {
            entry.task.agent == agent && entry.task.mode == mode && add.agent.as_deref() != Some(agents::AUTO)
        });
        if preserve.is_none()
            && let Some(mode) = &mode
        {
            let modes = agents::modes(&config, &agent);
            if !modes.iter().any(|(name, _)| name == mode) {
                return Err(format!("unknown {agent} mode {mode}"));
            }
            let args = agents::args(&config, &agent);
            config.agent_args.insert(agent.clone(), agents::with_mode(&agent, &args, &modes, Some(mode)));
        }
        let place = self.queue_target(caller, add, old)?;
        let prompt = add.prompt.clone().or_else(|| old.map(|entry| entry.task.prompt.clone())).unwrap_or_default();
        let mut label =
            add.label.clone().unwrap_or_else(|| prompt.lines().next().unwrap_or_default().trim().to_string());
        if add.label.is_none()
            && add.issue.is_none()
            && let Some(old) = old.filter(|entry| entry.task.prompt == prompt)
        {
            label.clone_from(&old.task.label);
        }
        label = crate::text::one_line(&label.chars().filter(|character| !character.is_control()).collect::<String>());
        let command =
            preserve.map_or_else(|| agents::command_line(&config, &agent), |entry| entry.task.command.clone());
        let program = preserve.map_or_else(|| agents::command(&config, &agent), |entry| entry.task.program.clone());
        let zone = if add.zone.is_empty() {
            old.map_or_else(crate::queue::clock::local_zone, |entry| entry.task.zone.clone())
        } else {
            add.zone.clone()
        };
        Ok(Task {
            at: 0,
            zone,
            label,
            agent,
            command,
            program,
            mode,
            prompt,
            submit: add.submit.unwrap_or_else(|| old.is_none_or(|entry| entry.task.submit)),
            place,
        })
    }

    fn queue_target(&mut self, caller: Option<u64>, add: &QueueAdd, old: Option<&Entry>) -> Result<Place, String> {
        if add.workspace.is_none()
            && add.project.is_none()
            && add.worktree.is_none()
            && let Some(old) = old
        {
            return Ok(old.task.place.clone());
        }
        let (p, w) = if let Some(workspace) = add.workspace {
            self.usable_workspace(workspace)?
        } else if let Some(project) = add.project {
            let p = self.projects.project_index(project).ok_or_else(|| super::super::none("project", project))?;
            (p, self.ensure_workspace(p))
        } else {
            self.here_workspace(caller)?
        };
        let project = &self.projects[p];
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
            Some(project.workspaces[w].path.clone())
        };
        let workspace_name = workspace.as_ref().and_then(|_| project.workspaces[w].name.clone());
        let workspace_id = workspace.as_ref().map(|_| project.workspaces[w].id);
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
