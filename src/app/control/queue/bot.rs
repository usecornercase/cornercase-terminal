use std::collections::BTreeMap;

use serde_json::json;

use super::prepare::{queue_label, queue_zone};
use super::routine::schedule;
use super::{Prepared, reset_agent};
use crate::agents;
use crate::app::App;
use crate::app::control::Handled;
use crate::bots::Bot;
use crate::control::QueueAdd;
use crate::queue::{self, Entry, Place, Task};

const MOVED: &str = "an entry cannot move between a bot and a project, or to another bot; add a new entry";

fn refuse_bot_mix(add: &QueueAdd) -> Result<(), String> {
    let taken =
        |name: &str| format!("a bot's queue entry takes no {name}: the bot keeps its own agent, mode and account");
    let named = [
        (add.project.is_some(), "project"),
        (add.workspace.is_some(), "workspace"),
        (add.worktree.is_some(), "worktree"),
        (add.issue.is_some(), "issue"),
        (add.source.is_some(), "source"),
        (add.flow.is_some(), "flow"),
        (add.cleanup.is_some(), "cleanup"),
        (add.agent.is_some(), "agent"),
        (add.mode.is_some(), "mode"),
        (!add.accounts.is_empty(), "account"),
        (add.fresh.is_some(), "fresh"),
    ];
    if let Some((_, name)) = named.iter().find(|(set, _)| *set) {
        return Err(taken(name));
    }
    if add.submit == Some(false) {
        return Err("a bot's queue entry always sends its prompt".into());
    }
    if !add.at.is_empty() && add.repeat.is_some() {
        return Err("--at and --every do not mix: a routine's times come from its repeat".into());
    }
    Ok(())
}

fn bot_task(bot: &Bot, add: &QueueAdd, old: Option<&Entry>, prompt: String) -> Task {
    Task {
        zone: queue_zone(add, old),
        label: queue_label(add, &prompt, old),
        agent: bot.agent.clone(),
        prompt,
        submit: true,
        place: Place { bot: Some(bot.id.clone()), ..Place::default() },
        accounts: BTreeMap::new(),
        paused: old.is_some_and(|entry| entry.task.paused),
        ..Task::default()
    }
}

fn bot_prompt(add: &QueueAdd, old: Option<&Entry>) -> Result<String, String> {
    let prompt = add.prompt.clone().or_else(|| old.map(|entry| entry.task.prompt.clone())).unwrap_or_default();
    if prompt.trim().is_empty() {
        return Err("a bot's queue entry needs --prompt".into());
    }
    match crate::bots::message_refusal(&prompt) {
        Some(why) => Err(why),
        None => Ok(prompt),
    }
}

impl App {
    fn queue_bot(&self, add: &QueueAdd, old: Option<&Entry>) -> Result<Bot, crate::control::contract::Refusal> {
        let bot = match (&add.bot, old) {
            (Some(key), _) => self.panels.bots.live[self.bot_at(key)?].bot.clone(),
            (None, Some(old)) => {
                let id = old.bot().ok_or(MOVED)?;
                self.panels.bots.live[self.bot_at(id)?].bot.clone()
            }
            (None, None) => return Err("a bot's queue entry needs --bot".into()),
        };
        if old.is_some_and(|old| old.bot() != Some(bot.id.as_str())) {
            return Err(MOVED.into());
        }
        Ok(bot)
    }

    pub(super) fn bot_queue_request(
        &mut self,
        client: Option<u64>,
        edit: Option<u64>,
        add: &QueueAdd,
        old: Option<&Entry>,
    ) -> Handled {
        refuse_bot_mix(add)?;
        let bot = self.queue_bot(add, old)?;
        let task = bot_task(&bot, add, old, bot_prompt(add, old)?);
        if add.repeat.is_some() || old.is_some_and(Entry::is_routine) {
            let text = add.repeat.clone().or_else(|| old.and_then(|e| e.task.repeat).map(|r| r.to_string()));
            let (repeat, at) = schedule(&text.unwrap_or_default(), &task.zone, old, crate::time::now())?;
            let id = self.panels.queued.schedule(edit, Task { at, repeat: Some(repeat), ..task })?;
            return Ok(Some(json!({ "id": id })));
        }
        let prepared = self.bot_reminder(task, add, old)?;
        self.schedule_prepared(client, edit, prepared)
    }

    fn bot_reminder(&self, task: Task, add: &QueueAdd, old: Option<&Entry>) -> Result<Prepared, String> {
        let at = if add.at.is_empty() {
            old.map(|entry| queue::clock::display(entry.task.at, &task.zone)).unwrap_or_default()
        } else {
            add.at.clone()
        };
        let kept_at = old
            .filter(|entry| add.at.is_empty() || add.at == queue::clock::display(entry.task.at, &task.zone))
            .map(|entry| entry.task.at);
        let probe =
            reset_agent(&at)?.map(|agent| (agent, agents::command(&self.config, agent.name()), self.usages.timeout()));
        Ok(Prepared {
            task,
            at,
            kept_at,
            issue: None,
            probe,
            clock: crate::time::now(),
            template: String::new(),
            naming: (String::new(), String::new()),
            named: true,
            follow: false,
            repo_root: false,
        })
    }
}
