use serde_json::json;

use super::super::machines::unknown;
use crate::app::App;
use crate::app::control::Handled;
use crate::control::QueueAdd;
use crate::control::contract::Refusal;
use crate::machines::memory::{AUTO, is_auto};
use crate::queue::{Entry, Task, Wait};

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
    if !add.at.is_empty() {
        return Err("an entry that waits for room has no time: it starts as soon as a machine has room".into());
    }
    if add.repeat.is_some() || add.cleanup.is_some() {
        return Err("a routine has its own times: it cannot wait for room, and --cleanup is a routine's".into());
    }
    if add.flow.is_some() && (add.agent.is_some() || add.mode.is_some()) {
        return Err("a flow chooses its own agents and modes".into());
    }
    Ok(())
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

    pub(super) fn make_free(&self, task: &mut Task, add: &QueueAdd, old: Option<&Entry>) -> Result<(), Refusal> {
        task.wait = Some(Wait::Free);
        task.machine = Some(self.free_machine(add, old)?);
        task.first = old.and_then(|entry| entry.task.first);
        task.flow = self.routine_flow(add, old, &task.place)?;
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
