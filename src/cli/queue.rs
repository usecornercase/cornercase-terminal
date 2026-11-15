use std::collections::BTreeMap;
use std::fmt::Write;

use serde::Deserialize;
use serde_json::Value;

use super::args::queue::{ListArgs, QueueAction, QueueAdd};
use super::{ask, print_json};
use crate::control;
use crate::error::{Error, Result};
use crate::queue::clock;

mod show;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Listed {
    id: u64,
    at: i64,
    label: String,
    agent: String,
    state: String,
    pane: Option<u64>,
    server: Option<String>,
    reason: Option<String>,
    accounts: BTreeMap<String, String>,
    repeat: Option<String>,
    paused: bool,
    flow: Option<String>,
    routine: Option<u64>,
    next: Option<i64>,
    started: Option<i64>,
    place: ListedPlace,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ListedPlace {
    bot: Option<String>,
}

impl Listed {
    fn status(&self) -> String {
        if self.paused {
            return "paused".into();
        }
        match (self.state.as_str(), self.pane, &self.reason) {
            ("kept", _, Some(reason)) => format!("kept: {reason}"),
            ("started", Some(pane), _) => {
                format!("started · pane {pane} · server {}", self.server.as_deref().unwrap_or_default())
            }
            ("failed", _, Some(reason)) => format!("failed: {reason}"),
            (state, ..) => state.to_string(),
        }
    }
}

fn lines(value: &Value, zone: &str) -> String {
    let mut out = String::new();
    for entry in value["entries"].as_array().into_iter().flatten() {
        let entry = Listed::deserialize(entry).unwrap_or_default();
        let at = clock::display(entry.at, zone);
        let agent = match (&entry.place.bot, &entry.flow) {
            (Some(bot), _) => format!("bot {bot}"),
            (None, Some(flow)) => format!("flow {flow}"),
            (None, None) => entry.agent.clone(),
        };
        let _ = write!(out, "{} {at} {} {} · {agent}", entry.id, entry.status(), entry.label);
        if let Some(repeat) = &entry.repeat {
            let _ = write!(out, " · ↻ every {repeat}");
        }
        if let Some(routine) = entry.routine {
            let _ = write!(out, " · routine {routine}");
        }
        for (agent, account) in &entry.accounts {
            let _ = write!(out, " · {agent}: {}", if account.is_empty() { "System" } else { account });
        }
        out.push('\n');
    }
    out
}

enum Asked {
    Queue(Box<control::Queue>, bool),
    Show(control::QueueShow, bool),
}

pub(super) fn run(action: QueueAction) -> Result<()> {
    if let QueueAction::List(ListArgs { bot: Some(bot), print }) = &action {
        return list_of_bot(bot, print.json);
    }
    match request_of(action, super::guard::own_bot().as_deref())? {
        Asked::Queue(command, json) => run_queue(*command, json),
        Asked::Show(show, json) => run_show(show, json),
    }
}

fn bot_id(asked: &str) -> Result<String> {
    crate::client::use_bots()?;
    let value = ask("bot", control::Command::Bot(control::bots::BotRequest::List))?;
    let bots = value["bots"].as_array().cloned().unwrap_or_default();
    let named = |by_name: bool| {
        bots.iter().find_map(|bot| {
            let (id, name) = (bot["id"].as_str()?, bot["name"].as_str()?);
            crate::bots::is_named(id, name, asked, by_name).then(|| id.to_string())
        })
    };
    Ok(named(false).or_else(|| named(true)).unwrap_or_else(|| asked.trim().to_string()))
}

fn list_of_bot(asked: &str, json: bool) -> Result<()> {
    crate::client::use_bot_routines()?;
    let id = bot_id(asked)?;
    let mut value = ask("queue", control::Command::Queue(control::Queue::List))?;
    if let Some(entries) = value["entries"].as_array_mut() {
        entries.retain(|entry| entry["place"]["bot"].as_str() == Some(id.as_str()));
    }
    if json { print_json(&value) } else { super::print_out(&lines(&value, &clock::local_zone())) }
}

fn run_show(show: control::QueueShow, json: bool) -> Result<()> {
    crate::client::use_queue_results()?;
    let conversation = show.conversation;
    let value = ask("queue show", control::Command::QueueShow(show))?;
    if json {
        print_json(&value)?;
    } else {
        super::print_out(&show::text(&value, &clock::local_zone()))?;
    }
    if show::partial(&value, conversation) { Err(Error::Partial) } else { Ok(()) }
}

fn run_queue(command: control::Queue, json: bool) -> Result<()> {
    let list = matches!(command, control::Queue::List);
    if matches!(command, control::Queue::Remove(_) | control::Queue::ClearStarted | control::Queue::Restore(_)) {
        crate::client::use_queue_undo()?;
    }
    if needs_routines(&command) {
        crate::client::use_queue_routines()?;
    }
    if needs_bot_routines(&command) {
        crate::client::use_bot_routines()?;
    }
    if needs_mode_default(&command) {
        crate::client::use_mode_default()?;
    }
    let value = ask("queue", control::Command::Queue(command))?;
    if json {
        return print_json(&value);
    }
    if list {
        return super::print_out(&lines(&value, &clock::local_zone()));
    }
    let printed = ["undo", "run", "id"].into_iter().find_map(|key| value.get(key).and_then(Value::as_u64));
    if let Some(id) = printed {
        super::print_out(&format!("{id}\n"))?;
    }
    Ok(())
}

fn needs_routines(command: &control::Queue) -> bool {
    match command {
        control::Queue::Pause(_) | control::Queue::Resume(_) => true,
        control::Queue::Add(add) | control::Queue::Edit { add, .. } => {
            add.repeat.is_some() || add.flow.is_some() || add.cleanup.is_some()
        }
        _ => false,
    }
}

fn needs_bot_routines(command: &control::Queue) -> bool {
    match command {
        control::Queue::Add(add) | control::Queue::Edit { add, .. } => add.bot.is_some(),
        _ => false,
    }
}

fn needs_mode_default(command: &control::Queue) -> bool {
    match command {
        control::Queue::Add(add) | control::Queue::Edit { add, .. } => {
            add.mode.as_deref() == Some(crate::agents::DEFAULT_MODE)
        }
        _ => false,
    }
}

fn request_of(action: QueueAction, own: Option<&str>) -> Result<Asked> {
    let queue = |command: control::Queue, json: bool| Ok(Asked::Queue(Box::new(command), json));
    match action {
        QueueAction::Add(add) => {
            let json = add.print.json;
            queue(control::Queue::Add(queue_add(add, true, own)?), json)
        }
        QueueAction::Edit { add, id } => {
            let json = add.print.json;
            queue(control::Queue::Edit { id, add: queue_add(add, false, None)? }, json)
        }
        QueueAction::List(list) => queue(control::Queue::List, list.print.json),
        QueueAction::Remove { id, print } => queue(control::Queue::Remove(id), print.json),
        QueueAction::Run { id } => queue(control::Queue::Run(id), false),
        QueueAction::ClearStarted(print) => queue(control::Queue::ClearStarted, print.json),
        QueueAction::Restore { undo } => queue(control::Queue::Restore(undo), false),
        QueueAction::Pause { id } => queue(control::Queue::Pause(id), false),
        QueueAction::Resume { id } => queue(control::Queue::Resume(id), false),
        QueueAction::Show { id, conversation, print } => {
            Ok(Asked::Show(control::QueueShow { id, conversation }, print.json))
        }
    }
}

fn own_default(add: &QueueAdd, new: bool, own: Option<&str>) -> Result<Option<String>> {
    let Some(own) = own.filter(|_| new && add.project.is_none() && add.workspace.is_none() && add.worktree.is_none())
    else {
        return Ok(None);
    };
    let set = [
        (add.agent.is_some(), "--agent"),
        (add.mode.is_some(), "--mode"),
        (!add.accounts.is_empty(), "--account"),
        (add.issue.is_some(), "--issue"),
        (add.source.is_some(), "--source"),
        (add.flow.is_some(), "--flow"),
        (add.cleanup || add.no_cleanup, "--cleanup"),
        (add.fresh.asked().is_some(), "--fresh"),
    ];
    if let Some((_, flag)) = set.iter().find(|(set, _)| *set) {
        return Err(Error::WrongUsage(format!(
            "a bot's queue entry takes no {flag}: the bot keeps its own agent, mode and account; add --project ID to queue an agent in a project"
        )));
    }
    Ok(Some(own.to_string()))
}

fn queue_add(add: QueueAdd, new: bool, own: Option<&str>) -> Result<control::QueueAdd> {
    if new && add.at.is_none() && add.every.is_none() {
        return Err(Error::WrongUsage("queue add needs --at or --every".into()));
    }
    let bot = match add.bot.clone() {
        Some(bot) => Some(bot),
        None => own_default(&add, new, own)?,
    };
    if bot.is_some() {
        if new && add.prompt.is_none() {
            return Err(Error::WrongUsage("a bot's queue entry needs --prompt".into()));
        }
    } else if new && add.prompt.is_none() && add.issue.is_none() {
        return Err(Error::WrongUsage("queue add needs --prompt or --issue".into()));
    }
    if new && bot.is_none() && add.every.is_none() && (add.flow.is_some() || add.cleanup || add.no_cleanup) {
        return Err(Error::WrongUsage("--flow and --cleanup belong to a routine: add --every".into()));
    }
    let cleanup = if add.cleanup { Some(true) } else { add.no_cleanup.then_some(false) };
    Ok(control::QueueAdd {
        at: add.at.unwrap_or_default(),
        zone: clock::local_zone(),
        agent: add.agent,
        mode: add.mode,
        project: add.project,
        workspace: add.workspace,
        worktree: add.worktree,
        prompt: add.prompt,
        issue: add.issue,
        source: add.source,
        label: add.name,
        submit: None,
        accounts: super::accounts::account_choices(add.accounts)?,
        repeat: add.every,
        flow: add.flow,
        cleanup,
        fresh: super::fresh(&add.fresh)?,
        bot,
    })
}

#[cfg(test)]
mod tests;
