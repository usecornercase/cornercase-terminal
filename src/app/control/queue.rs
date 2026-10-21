mod prepare;

use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use serde_json::json;

use super::{Asked, Asker, Handled, Stage, Waits};
use crate::agents;
use crate::agents::usage;
use crate::app::App;
use crate::app::queue_panel::QueueEvent;
use crate::control::contract::Refusal;
use crate::control::{self, QueueAdd};
use crate::error::Error;
use crate::issues;
use crate::issues::Client;
use crate::issues::locate::Locator;
use crate::log::{Job, Level};
use crate::project::{Hierarchy, WorkspaceAt};
use crate::queue::{self, Place, State};

impl App {
    pub(super) fn queue_request(&mut self, asked: Asked, command: control::Queue) -> Handled {
        let Asked { client, asker, area, now } = asked;
        match command {
            control::Queue::List => Ok(Some(json!({ "entries": self.panels.queued.queue().entries }))),
            control::Queue::Remove(id) => self.remove_queue_request(id),
            control::Queue::Run(id) => self.run_queue_request(id, area, now),
            control::Queue::Add(add) => self.add_queue_request(Some(client), asker.caller, None, &add),
            control::Queue::Edit { id, add } => self.add_queue_request(Some(client), asker.caller, Some(id), &add),
        }
    }

    fn remove_queue_request(&mut self, id: u64) -> Handled {
        self.panels.queued.remove(id)?;
        Ok(Some(json!({})))
    }

    fn run_queue_request(&mut self, id: u64, area: Option<Rect>, now: Instant) -> Handled {
        self.set_queue_area(super::sized(area)?);
        if let Err(reason) = self.run_queued(id, now) {
            let unfinished = |entry: &queue::Entry| !matches!(entry.state, State::Starting | State::Started { .. });
            if self.panels.queued.queue().entry(id).is_some_and(unfinished) {
                self.fail_queued(id, &reason);
            }
            return Err(reason.into());
        }
        Ok(Some(json!({ "id": id })))
    }

    pub(in crate::app) fn add_queue_request(
        &mut self,
        client: Option<u64>,
        caller: Option<u64>,
        edit: Option<u64>,
        add: &QueueAdd,
    ) -> Handled {
        if edit.is_some_and(|id| self.panels.queued.starting(id)) {
            return Err("this entry is already starting".into());
        }
        let old = edit
            .map(|id| self.panels.queued.queue().entry(id).cloned().ok_or_else(|| queue::missing(id)))
            .transpose()?;
        let mut prepared = self.prepare_queue(caller, add, old.as_ref())?;
        if prepared.issue.is_none() && prepared.probe.is_none() {
            return self.add_queue_now(edit, &mut prepared);
        }
        if let Some(client) = client {
            let key = self.clients.requests.key();
            self.clients.requests.push(
                client,
                key,
                Waits { done: control::Done::default(), stage: Stage::Reading, timeout: None },
            );
        }
        let job = Job::new(Level::Info, "queue", "prepare");
        self.spawn_job(
            job,
            move || prepared.finish(),
            move |result, _| {
                let result = result.map_err(|error| error.to_string());
                Some(crate::app::AppEvent::Queue(Box::new(QueueEvent { client, edit, result })))
            },
        );
        Ok(None)
    }

    fn prepare_queue(
        &mut self,
        caller: Option<u64>,
        add: &QueueAdd,
        old: Option<&queue::Entry>,
    ) -> Result<Prepared, Refusal> {
        let task = self.queue_task(caller, add, old)?;
        self.requested_accounts(None, &task.accounts)?;
        let at = if add.at.is_empty() {
            old.map(|entry| queue::clock::display(entry.task.at, &task.zone)).unwrap_or_default()
        } else {
            add.at.clone()
        };
        let kept_at = old
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
        let reset = reset_agent(&at)?;
        let probe = reset.map(|agent| (agent, agents::command(&self.config, agent.name()), self.usages.timeout()));
        let repo_root = crate::git::is_repo_root(&task.place.project);
        let (named, follow) = (add.label.is_some(), add.worktree.is_none());
        let template = self.config.prompt.clone();
        let naming = (self.config.issue_branch.clone(), self.branch_user(Some(&task.place.project)));
        let clock = crate::time::now();
        Ok(Prepared { task, at, kept_at, issue, probe, clock, template, naming, named, follow, repo_root })
    }

    fn add_queue_now(&mut self, edit: Option<u64>, prepared: &mut Prepared) -> Handled {
        let Prepared { task, at, kept_at, clock, .. } = prepared;
        task.at = kept_at.map_or_else(|| queue::clock::parse(at, &task.zone, *clock), Ok)?;
        if task.at <= *clock {
            return Err("choose a time in the future".into());
        }
        let task = task.clone();
        let id = self.panels.queued.schedule(edit, task)?;
        Ok(Some(json!({ "id": id })))
    }

    pub(in crate::app) fn queue_reply(&mut self, client: u64, reply: Result<u64, String>) {
        self.clients.requests.pending.retain(|pending| pending.client != Some(client));
        self.answer(Some(client), reply.map(|id| json!({ "id": id })));
    }

    fn queue_place(&self, place: &Place) -> Result<(usize, Option<WorkspaceAt>), String> {
        let p = self
            .projects
            .iter()
            .position(|project| !project.closing && project.path == place.project)
            .ok_or_else(|| format!("the queued project {} is closed", place.project.display()))?;
        let at = match (&place.workspace, place.workspace_id) {
            (Some(path), Some(id)) if place.server == control::server_token() => {
                let at = self.usable_workspace(id)?;
                if at.project() != p || self.projects.workspace(at).path != *path {
                    return Err("the queued workspace moved to another folder".into());
                }
                Some(at)
            }
            (Some(path), _) => {
                let mut matching = self.projects[p].workspaces.iter().filter(|workspace| {
                    workspace.open() && workspace.path == *path && workspace.name == place.workspace_name
                });
                let workspace = matching
                    .next()
                    .ok_or_else(|| format!("the queued workspace {} is closed or renamed", path.display()))?;
                let id = workspace.id;
                if matching.next().is_some() {
                    return Err(
                        "the queued workspace has an ambiguous name after restart; edit its place before starting"
                            .into(),
                    );
                }
                Some(self.projects.workspace_in(self.projects[p].id, id).ok_or("the queued workspace is closed")?)
            }
            (None, _) => None,
        };
        Ok((p, at))
    }

    pub(in crate::app) fn run_queued(&mut self, id: u64, now: Instant) -> Result<(), String> {
        let entry = self.panels.queued.queue().entry(id).ok_or_else(|| queue::missing(id))?.clone();
        if matches!(entry.state, State::Starting | State::Started { .. }) || self.panels.queued.starting(id) {
            return Err("this queue entry has already started".into());
        }
        let (p, at) = self.queue_place(&entry.task.place)?;
        if !agents::installed(&entry.task.program) {
            return Err(format!("{} is not installed", entry.task.agent));
        }
        let start = control::Start {
            agent: Some(entry.task.agent.clone()),
            project: Some(self.projects[p].id),
            workspace: at.map(|at| self.projects.workspace(at).id),
            worktree: entry.task.place.branch.clone(),
            name: Some(entry.task.label.clone()),
            accounts: entry.task.accounts.clone(),
            ..control::Start::default()
        };
        let client = self.mark_starting(id)?;
        let asked = Asked { client, asker: Asker::default(), area: self.panels.queued.area(), now };
        match self.start_request(asked, start) {
            Ok(Some(value)) => {
                self.queue_answered(client, &Ok(value));
            }
            Ok(None) => {}
            Err(reason) => return Err(self.fail_starting(client, id, String::from(reason))),
        }
        Ok(())
    }

    fn mark_starting(&mut self, id: u64) -> Result<u64, String> {
        let client = self.panels.queued.start(id);
        if let Err(error) = self.panels.queued.save() {
            let reason = format!("cannot save the queue before starting: {error}");
            return Err(self.fail_starting(client, id, reason));
        }
        Ok(client)
    }

    fn fail_starting(&mut self, client: u64, id: u64, reason: String) -> String {
        self.panels.queued.finish(client);
        self.panels.queued.fail(id, &reason);
        reason
    }

    pub(in crate::app) fn focus_queued(&mut self, pane: u64, area: Rect) {
        if self.show(control::Item::Pane(pane)).is_ok() {
            self.resize(area);
        }
    }
}

struct Prepared {
    task: queue::Task,
    at: String,
    kept_at: Option<i64>,
    issue: Option<(Client, Locator)>,
    probe: Option<(agents::Known, String, Duration)>,
    clock: i64,
    template: String,
    naming: (String, String),
    named: bool,
    follow: bool,
    repo_root: bool,
}

impl Prepared {
    fn finish(self) -> Result<queue::Task, Error> {
        let Self { mut task, at, kept_at, issue, probe, clock, template, naming, named, follow, repo_root } = self;
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
            None => kept_at.map_or_else(|| queue::clock::parse(&at, &task.zone, clock), Ok).map_err(Error::Api)?,
        };
        Ok(task)
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
