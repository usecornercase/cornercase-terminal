use serde_json::json;

use super::super::flow::pick;
use super::super::machines::unknown;
use crate::app::App;
use crate::app::control::Handled;
use crate::control::QueueAdd;
use crate::control::contract::Refusal;
use crate::machines::memory::{AUTO, is_auto};
use crate::queue::{Entry, Place, Task, Wait};

const FREE: &str = "free";

pub(super) fn is_free_request(add: &QueueAdd, old: Option<&Entry>) -> bool {
    add.when.is_some() || add.machine.is_some() || old.is_some_and(Entry::is_free)
}

pub(super) fn refuse_free_mix(add: &QueueAdd, old: Option<&Entry>) -> Result<(), String> {
    if add.when.as_deref().is_some_and(|when| when != FREE) {
        return Err("`when` takes free: the entry waits until a machine has room".into());
    }
    if add.when.is_none() && old.is_none_or(|entry| !entry.is_free()) {
        return Err(
            "--machine goes with --when free; to start on a machine now use `cornercase start --machine`".into()
        );
    }
    if old.is_some_and(|entry| !entry.is_free()) {
        return Err("an entry with a time cannot start waiting for room; add a new entry".into());
    }
    let refused = [
        (!add.at.is_empty(), "an entry that waits for room has no time: it starts as soon as a machine has room"),
        (add.repeat.is_some(), "a routine has its own times: it cannot wait for room"),
        (add.cleanup.is_some(), "--cleanup belongs to a routine"),
        (add.flow.is_some() && (add.agent.is_some() || add.mode.is_some()), "a flow chooses its own agents and modes"),
    ];
    refused.iter().find(|(wrong, _)| *wrong).map_or(Ok(()), |(_, why)| Err((*why).into()))
}

impl App {
    pub(super) fn free_machine(&self, add: &QueueAdd, old: Option<&Entry>) -> Result<String, Refusal> {
        let asked = add.machine.clone().or_else(|| old.and_then(|entry| entry.task.machine.clone()));
        let Some(asked) = asked.map(|name| name.trim().to_string()) else { return Ok(AUTO.into()) };
        if is_auto(&asked) {
            return Ok(AUTO.into());
        }
        let machine = self.config.machines.iter().find(|machine| machine.name.eq_ignore_ascii_case(&asked));
        machine.map(|machine| machine.name.clone()).ok_or_else(|| unknown(&asked))
    }

    pub(super) fn free_flow(
        &self,
        add: &QueueAdd,
        old: Option<&Entry>,
        place: &Place,
    ) -> Result<Option<String>, String> {
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

    pub(super) fn make_free(&self, task: &mut Task, add: &QueueAdd, old: Option<&Entry>) -> Result<(), Refusal> {
        task.wait = Some(Wait::Free);
        task.machine = Some(self.free_machine(add, old)?);
        task.first = old.and_then(|entry| entry.task.first);
        task.flow = self.free_flow(add, old, &task.place)?;
        if task.flow.is_some() {
            task.agent.clear();
            task.command.clear();
            task.program.clear();
            task.mode = None;
        }
        Ok(())
    }

    pub(super) fn first_queue_request(&mut self, id: u64) -> Handled {
        self.panels.queued.put_first(id)?;
        Ok(Some(json!({ "id": id })))
    }
}
