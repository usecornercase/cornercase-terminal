use serde_json::json;

use super::super::flow::pick;
use super::prepare::{Chosen, merged_accounts, queue_label, queue_zone};
use crate::agents;
use crate::app::App;
use crate::app::control::Handled;
use crate::control::QueueAdd;
use crate::git;
use crate::queue::repeat::Repeat;
use crate::queue::{Entry, Place, Task};

pub(super) fn is_routine_request(add: &QueueAdd, old: Option<&Entry>) -> bool {
    add.repeat.is_some()
        || add.flow.is_some()
        || add.cleanup.is_some()
        || old.is_some_and(|entry| entry.is_routine() || entry.is_run())
}

fn refuse_mix(add: &QueueAdd, old: Option<&Entry>) -> Result<(), String> {
    if let Some(run) = old.and_then(|entry| entry.task.routine) {
        return Err(format!("this is a run of routine {run}; edit the routine instead"));
    }
    if old.is_some_and(|entry| !entry.is_routine()) {
        return Err("a one-off entry cannot become a routine; add a new entry".into());
    }
    if add.repeat.is_none() && old.is_none() {
        return Err("a flow is queued as a routine, and so is a cleanup: add --every".into());
    }
    let refused = [
        (!add.at.is_empty(), "--at and --every do not mix: a routine's times come from its repeat"),
        (add.issue.is_some(), "a routine cannot take an issue: the same issue every time makes no sense"),
        (add.workspace.is_some(), "a routine always runs in a new worktree: it takes no workspace"),
        (add.worktree.is_some(), "a routine names its own worktrees: it takes no branch"),
        (add.flow.is_some() && (add.agent.is_some() || add.mode.is_some()), "a flow chooses its own agents and modes"),
        (add.submit == Some(false), "a routine always submits its prompt: it cannot keep a draft"),
    ];
    refused.iter().find(|(wrong, _)| *wrong).map_or(Ok(()), |(_, why)| Err((*why).into()))
}

fn refuse_cleanup(cleanup: bool, flow: Option<&str>, agent: &str) -> Result<(), String> {
    if cleanup && flow.is_none() && agents::Known::of(agent).is_none() {
        return Err(format!("cornercase cannot tell when {agent} finishes, so it cannot remove its workspace"));
    }
    Ok(())
}

fn schedule(text: &str, zone: &str, old: Option<&Entry>, now: i64) -> Result<(Repeat, i64), String> {
    if let Some(entry) = old
        && let Some(repeat) = entry.task.repeat
        && entry.task.zone == zone
        && repeat.accepts(text)
    {
        return Ok((repeat, entry.task.at));
    }
    let repeat = Repeat::parse(text, zone, now)?;
    Ok((repeat, repeat.first(zone, now)?))
}

impl App {
    fn routine_place(&mut self, caller: Option<u64>, add: &QueueAdd, old: Option<&Entry>) -> Result<Place, String> {
        let path = if let (Some(old), None) = (old, add.project) {
            old.task.place.project.clone()
        } else {
            let at = self.queue_workspace(caller, add)?;
            self.projects[at.project()].path.clone()
        };
        if !git::is_repo_root(&path) {
            return Err(format!("a routine needs a git repository: {} is a plain folder", path.display()));
        }
        Ok(Place { project: path, ..Place::default() })
    }

    fn routine_flow(&self, add: &QueueAdd, old: Option<&Entry>, place: &Place) -> Result<Option<String>, String> {
        let name = match (&add.flow, old) {
            (Some(name), _) => name.clone(),
            (None, Some(old)) if add.agent.is_none() => match &old.task.flow {
                Some(name) => name.clone(),
                None => return Ok(None),
            },
            _ => return Ok(None),
        };
        let (flow, _) = pick(&self.flow_listing(Some(&place.project)), &name)?;
        Ok(Some(flow.name))
    }

    pub(super) fn routine_request(
        &mut self,
        caller: Option<u64>,
        edit: Option<u64>,
        add: &QueueAdd,
        old: Option<&Entry>,
    ) -> Handled {
        refuse_mix(add, old)?;
        let place = self.routine_place(caller, add, old)?;
        let flow = self.routine_flow(add, old, &place)?;
        if flow.is_some() && add.mode.is_some() {
            return Err("a flow chooses its own agents and modes".into());
        }
        let chosen = if flow.is_some() { Chosen::default() } else { self.queue_agent(caller, add, old)? };
        let prompt = add.prompt.clone().or_else(|| old.map(|entry| entry.task.prompt.clone())).unwrap_or_default();
        if prompt.trim().is_empty() {
            return Err("a queue entry needs --prompt".into());
        }
        let cleanup = add.cleanup.unwrap_or_else(|| old.is_some_and(|entry| entry.task.cleanup));
        refuse_cleanup(cleanup, flow.as_deref(), &chosen.agent)?;
        let zone = queue_zone(add, old);
        let text = add.repeat.clone().or_else(|| old.and_then(|entry| entry.task.repeat).map(|r| r.to_string()));
        let (repeat, at) = schedule(&text.unwrap_or_default(), &zone, old, crate::time::now())?;
        let Chosen { agent, command, program, mode } = chosen;
        let task = Task {
            at,
            label: queue_label(add, &prompt, old),
            zone,
            agent,
            command,
            program,
            mode,
            submit: true,
            accounts: merged_accounts(add, old),
            repeat: Some(repeat),
            flow,
            cleanup,
            paused: old.is_some_and(|entry| entry.task.paused),
            prompt,
            place,
            routine: None,
        };
        self.requested_accounts(None, &task.accounts)?;
        let id = self.panels.queued.schedule(edit, task)?;
        Ok(Some(json!({ "id": id })))
    }

    pub(super) fn pause_queue_request(&mut self, id: u64, paused: bool) -> Handled {
        self.panels.queued.pause(id, paused, crate::time::now())?;
        Ok(Some(json!({ "id": id })))
    }
}
