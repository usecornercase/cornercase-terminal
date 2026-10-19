mod prepare;

use std::time::Instant;

use ratatui::layout::Rect;
use serde_json::json;

use super::{Handled, Pending, Stage};
use crate::agents;
use crate::agents::usage;
use crate::app::App;
use crate::app::queue_panel::{CLIENTS, QueueEvent};
use crate::control::{self, QueueAdd};
use crate::error::Error;
use crate::issues;
use crate::log::{Job, Level};
use crate::queue::{self, Place, State};

impl App {
    pub(super) fn queue_request(
        &mut self,
        client: u64,
        caller: Option<u64>,
        command: control::Queue,
        area: Option<Rect>,
        now: Instant,
    ) -> Handled {
        match command {
            control::Queue::List => Ok(Some(json!({ "entries": self.queued.queue.entries }))),
            control::Queue::Remove(id) => {
                if self.queued.running.values().any(|running| *running == id) {
                    return Err("this entry is already starting".into());
                }
                self.queued.queue.remove(id)?;
                Ok(Some(json!({})))
            }
            control::Queue::Run(id) => {
                self.queued.area = Some(super::sized(area)?);
                if let Err(reason) = self.run_queued(id, now) {
                    if self
                        .queued
                        .queue
                        .entry(id)
                        .is_some_and(|entry| !matches!(entry.state, State::Starting | State::Started { .. }))
                    {
                        self.fail_queued(id, &reason);
                    }
                    return Err(reason.into());
                }
                Ok(Some(json!({ "id": id })))
            }
            control::Queue::Add(add) => self.add_queue_request(Some(client), caller, None, &add),
            control::Queue::Edit { id, add } => self.add_queue_request(Some(client), caller, Some(id), &add),
        }
    }

    pub(in crate::app) fn add_queue_request(
        &mut self,
        client: Option<u64>,
        caller: Option<u64>,
        edit: Option<u64>,
        add: &QueueAdd,
    ) -> Handled {
        if edit.is_some_and(|id| self.queued.running.values().any(|running| *running == id)) {
            return Err("this entry is already starting".into());
        }
        let old = edit.map(|id| self.queued.queue.entry(id).cloned().ok_or_else(|| queue::missing(id))).transpose()?;
        let mut task = self.queue_task(caller, add, old.as_ref())?;
        self.requested_accounts(None, &task.accounts)?;
        let at = if add.at.is_empty() {
            old.as_ref().map(|entry| queue::clock::display(entry.task.at, &task.zone)).unwrap_or_default()
        } else {
            add.at.clone()
        };
        let kept_at = old
            .as_ref()
            .filter(|entry| add.at.is_empty() || add.at == queue::clock::display(entry.task.at, &task.zone))
            .map(|entry| entry.task.at);
        let project = self
            .projects
            .iter()
            .find(|project| !project.closing && project.path == task.place.project)
            .ok_or("the queued project is closed")?
            .id;
        let issue = self.queue_issue(project, add)?;
        if issue.is_none() && task.prompt.trim().is_empty() {
            return Err("a queue entry needs --prompt or --issue".into());
        }
        let template = self.config.prompt.clone();
        let naming = (self.config.issue_branch.clone(), self.branch_user(Some(&task.place.project)));
        let reset = reset_agent(&at)?;
        let probe = reset.map(|agent| (agent, agents::command(&self.config, agent.name()), self.usage_timeout));
        let clock = crate::time::now();
        if issue.is_none() && probe.is_none() {
            task.at = kept_at.map_or_else(|| queue::clock::parse(&at, &task.zone, clock), Ok)?;
            if task.at <= clock {
                return Err("choose a time in the future".into());
            }
            let id = match edit {
                Some(id) => {
                    self.queued.queue.edit(id, task)?;
                    id
                }
                None => self.queued.queue.add(task)?,
            };
            return Ok(Some(json!({ "id": id })));
        }
        if let Some(client) = client {
            let key = self.requests.key();
            self.requests.pending.push(Pending {
                client: Some(client),
                key,
                timeout: None,
                done: control::Done::default(),
                stage: Stage::Reading,
            });
        }
        let named = add.label.is_some();
        let follow = add.worktree.is_none();
        let repo_root = crate::git::is_repo_root(&task.place.project);
        let job = Job::new(Level::Info, "queue", "prepare");
        self.spawn_job(
            job,
            move || {
                if let Some((api, locator)) = issue {
                    let issue = issues::locate::find(&api, &locator)?;
                    let branch = issues::naming::branch(&issue, &naming.0, &naming.1);
                    task.prompt = issues::prompt(&template, &issue, &branch);
                    if !named {
                        task.label = format!("{} {}", issue.key, issue.title);
                    }
                    if repo_root && task.place.workspace.is_none() && (follow || task.place.branch.is_none()) {
                        task.place.branch = Some(branch);
                    }
                }
                task.at = match probe {
                    Some((agent, command, timeout)) => {
                        let report = usage::probe(agent, &command, None, timeout)?;
                        reset_time(&report, clock).map_err(Error::Api)?
                    }
                    None => {
                        kept_at.map_or_else(|| queue::clock::parse(&at, &task.zone, clock), Ok).map_err(Error::Api)?
                    }
                };
                Ok(task)
            },
            move |result, _| {
                Some(crate::app::AppEvent::Queue(Box::new(QueueEvent {
                    client,
                    edit,
                    result: result.map_err(|error| error.to_string()),
                })))
            },
        );
        Ok(None)
    }

    pub(in crate::app) fn queue_reply(&mut self, client: u64, reply: Result<u64, String>) {
        self.requests.pending.retain(|pending| pending.client != Some(client));
        self.answer(Some(client), reply.map(|id| json!({ "id": id })));
    }

    fn queue_place(&self, place: &Place) -> Result<(usize, usize), String> {
        let p = self
            .projects
            .iter()
            .position(|project| !project.closing && project.path == place.project)
            .ok_or_else(|| format!("the queued project {} is closed", place.project.display()))?;
        let w = match (&place.workspace, place.workspace_id) {
            (Some(path), Some(id)) if place.server == control::server_token() => {
                let (found, w) = self.usable_workspace(id)?;
                if found != p || self.projects[p].workspaces[w].path != *path {
                    return Err("the queued workspace moved to another folder".into());
                }
                w
            }
            (Some(path), _) => {
                let mut matching = self.projects[p].workspaces.iter().enumerate().filter(|(_, workspace)| {
                    workspace.open() && workspace.path == *path && workspace.name == place.workspace_name
                });
                let (_, workspace) = matching
                    .next()
                    .ok_or_else(|| format!("the queued workspace {} is closed or renamed", path.display()))?;
                let id = workspace.id;
                if matching.next().is_some() {
                    return Err(
                        "the queued workspace has an ambiguous name after restart; edit its place before starting"
                            .into(),
                    );
                }
                self.projects[p].index_of(id).ok_or("the queued workspace is closed")?
            }
            (None, _) => self.projects[p].active,
        };
        Ok((p, w))
    }

    pub(in crate::app) fn run_queued(&mut self, id: u64, now: Instant) -> Result<(), String> {
        let entry = self.queued.queue.entry(id).ok_or_else(|| queue::missing(id))?.clone();
        if matches!(entry.state, State::Starting | State::Started { .. })
            || self.queued.running.values().any(|running| *running == id)
        {
            return Err("this queue entry has already started".into());
        }
        let (p, w) = self.queue_place(&entry.task.place)?;
        if !agents::installed(&entry.task.program) {
            return Err(format!("{} is not installed", entry.task.agent));
        }
        let start = control::Start {
            agent: Some(entry.task.agent.clone()),
            project: Some(self.projects[p].id),
            workspace: entry.task.place.workspace.as_ref().map(|_| self.projects[p].workspaces[w].id),
            worktree: entry.task.place.branch.clone(),
            name: Some(entry.task.label.clone()),
            accounts: entry.task.accounts.clone(),
            ..control::Start::default()
        };
        self.queued.next_client += 1;
        let client = CLIENTS + self.queued.next_client;
        self.queued.running.insert(client, id);
        if let Some(entry) = self.queued.queue.entry_mut(id) {
            entry.state = State::Starting;
        }
        if let Some(path) = &self.queued.path
            && let Err(error) = crate::persist::save(path, &self.queue_saved())
        {
            self.queued.running.remove(&client);
            let reason = format!("cannot save the queue before starting: {error}");
            if let Some(entry) = self.queued.queue.entry_mut(id) {
                entry.state = State::Failed { reason: reason.clone() };
            }
            return Err(reason);
        }
        match self.start_request(client, None.into(), start, self.queued.area, now) {
            Ok(Some(value)) => {
                self.queue_answered(client, &Ok(value));
            }
            Ok(None) => {}
            Err(reason) => {
                let reason = String::from(reason);
                self.queued.running.remove(&client);
                if let Some(entry) = self.queued.queue.entry_mut(id) {
                    entry.state = State::Failed { reason: reason.clone() };
                }
                return Err(reason);
            }
        }
        Ok(())
    }

    pub(in crate::app) fn focus_queued(&mut self, pane: u64, area: Rect) {
        if self.show(control::Item::Pane(pane)).is_ok() {
            self.resize(area);
        }
    }
}

fn reset_agent(at: &str) -> Result<Option<agents::Known>, String> {
    match at.trim() {
        "claude reset" => Ok(Some(agents::Known::Claude)),
        "codex reset" => Ok(Some(agents::Known::Codex)),
        at if at.ends_with(" reset") => Err("plan resets are available for claude and codex".into()),
        _ => Ok(None),
    }
}

pub(super) fn reset_time(report: &usage::Report, now: i64) -> Result<i64, String> {
    report
        .windows
        .iter()
        .find(|window| window.label == "session (5h)")
        .and_then(|window| window.resets_at)
        .filter(|at| *at > now)
        .ok_or_else(|| {
            "the plan probe returned no future reset for its 5-hour window; choose a clock time instead".into()
        })
}

#[cfg(test)]
mod tests;
