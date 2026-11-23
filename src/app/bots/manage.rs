use std::path::Path;

use crate::agents::headless::Access;
use crate::agents::{self, Known};
use crate::app::App;
use crate::bots::{Bot, ChatApps, MAX_BOTS, NameProblem, chat, folder};
use crate::control::bots::{AgentInfo, BotInfo, BotLast, EditBot, NewBot};
use crate::control::contract::{Code, Refusal};

use super::Live;

type Done<T> = Result<T, Refusal>;

struct Support {
    bot: bool,
    why: Option<String>,
    accesses: Vec<Access>,
}

fn clean(text: Option<&str>) -> Option<String> {
    text.map(str::trim).filter(|text| !text.is_empty()).map(str::to_string)
}

fn bot_agent_names() -> String {
    let names: Vec<&str> = Known::ALL.into_iter().filter(|kind| kind.bot().is_some()).map(Known::name).collect();
    match names.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} or {last}", rest.join(", ")),
        _ => names.join(""),
    }
}

fn needs_a_model(kind: Known, bot: &Bot) -> Done<()> {
    agents::effort_needs_model_refusal(kind.name(), bot.model.as_deref(), bot.effort.as_deref())
        .map_or(Ok(()), |why| Err(Refusal::new(Code::Malformed, why)))
}

fn chat_apps_of(asked: Option<&str>, kept: ChatApps) -> Done<ChatApps> {
    clean(asked).map_or(Ok(kept), |text| {
        ChatApps::parse(&text)
            .ok_or_else(|| Refusal::new(Code::Malformed, "the chat apps option is away, always or off"))
    })
}

fn problem(problem: &NameProblem) -> Refusal {
    let code = if matches!(problem, NameProblem::Taken(_)) { Code::Conflict } else { Code::Malformed };
    Refusal::new(code, problem.message())
}

impl App {
    fn bot_support(&self, kind: Known) -> Support {
        let installed = agents::installed(&agents::command(&self.config, kind.name()));
        match kind.bot() {
            None => Support { bot: false, why: Some(String::from("not yet")), accesses: Vec::new() },
            Some(_) if !installed => {
                Support { bot: false, why: Some(String::from("not installed")), accesses: Vec::new() }
            }
            Some(support) => {
                Support { bot: true, why: support.why.map(str::to_string), accesses: support.accesses.to_vec() }
            }
        }
    }

    pub(in crate::app) fn bot_agent_infos(&self) -> Vec<AgentInfo> {
        Known::ALL
            .into_iter()
            .map(|kind| {
                let support = self.bot_support(kind);
                AgentInfo {
                    agent: kind.name().to_string(),
                    title: kind.title().to_string(),
                    installed: agents::installed(&agents::command(&self.config, kind.name())),
                    bot: support.bot,
                    why: support.why,
                    accesses: support.accesses.iter().map(|access| access.name().to_string()).collect(),
                    reads: kind.bot().filter(|bot| !bot.reads_anywhere).map(|_| String::from("folder")),
                    efforts: kind
                        .effort()
                        .map_or_else(Vec::new, |effort| effort.levels.iter().map(ToString::to_string).collect()),
                    models: kind.models().iter().map(ToString::to_string).collect(),
                }
            })
            .collect()
    }

    pub(in crate::app) fn bot_info(&self, at: usize) -> BotInfo {
        let live = &self.panels.bots.live[at];
        let bot = &live.bot;
        BotInfo {
            id: bot.id.clone(),
            name: bot.name.clone(),
            agent: bot.agent.clone(),
            model: bot.model.clone(),
            effort: bot.effort.clone(),
            account: bot.account.clone(),
            access: bot.access.name().to_string(),
            follow: bot.follow,
            following: self.following_panes(at),
            chat_apps: bot.chat_apps.name().to_string(),
            state: String::from(if live.turn.is_some() { "thinking" } else { "idle" }),
            held: live.held.len(),
            unread: live.unread,
            last: live.last.as_ref().map(last),
            folder: live.folder.display().to_string(),
            created: bot.created,
            routines: self.bot_entries(&bot.id).filter(|entry| entry.is_routine()).count(),
            next: self.bot_next_run(&bot.id),
            secrets: live.secrets.count(),
            secrets_unreadable: live.secrets.is_unreadable(),
            asks: self.ask_infos(at),
        }
    }

    pub(in crate::app) fn bot_infos(&self) -> Vec<BotInfo> {
        (0..self.panels.bots.live.len()).map(|at| self.bot_info(at)).collect()
    }

    pub(in crate::app) fn bot_about(&self, key: &str) -> Done<(String, String)> {
        let at = self.bot_at(key)?;
        let live = &self.panels.bots.live[at];
        let about = folder::about(&live.folder)
            .map_err(|error| Refusal::new(Code::Failed, format!("cannot read {}'s bot.md: {error}", live.bot.id)))?;
        Ok((live.bot.id.clone(), about))
    }

    fn bot_kind(&self, asked: Option<&str>) -> Done<Known> {
        match clean(asked) {
            Some(name) => {
                Known::of(&name).ok_or_else(|| Refusal::new(Code::Malformed, format!("{name} is not an agent")))
            }
            None => Known::of(self.config.agent.trim()).filter(|kind| self.bot_support(*kind).bot).ok_or_else(|| {
                Refusal::new(Code::Malformed, format!("say which agent with --agent: {}", bot_agent_names()))
            }),
        }
    }

    fn bot_access(&self, kind: Known, asked: Option<&str>) -> Done<Access> {
        let support = self.bot_support(kind);
        if !support.bot {
            let why = support.why.unwrap_or_default();
            return Err(Refusal::new(Code::Malformed, format!("{} cannot be a bot: {why}", kind.name())));
        }
        let access = match clean(asked) {
            Some(text) => {
                Access::parse(&text).ok_or_else(|| Refusal::new(Code::Malformed, "the access is cornercase or full"))?
            }
            None => Access::Cornercase,
        };
        if support.accesses.contains(&access) {
            return Ok(access);
        }
        let why = support.why.unwrap_or_default();
        let text = format!(
            "{} can be a bot only with --access full here: {why}. Full access means it can change any file and run anything, unattended",
            kind.name()
        );
        Err(Refusal::new(Code::Malformed, text))
    }

    fn bot_settings(&self, kind: Known, bot: &mut Bot, edit: &EditBot) -> Done<()> {
        let name = kind.name();
        let given = |text: &Option<String>| text.as_deref().map(str::trim).map(str::to_string);
        if let Some(model) = given(&edit.model) {
            agents::model_refusal(name, Some(&model)).map_or(Ok(()), |why| Err(Refusal::new(Code::Malformed, why)))?;
            bot.model = Some(model).filter(|model| !model.is_empty());
        }
        if let Some(level) = given(&edit.effort) {
            if let Some(why) =
                Some(&level).filter(|level| !level.is_empty()).and_then(|level| agents::effort_refusal(name, level))
            {
                return Err(Refusal::new(Code::Malformed, why));
            }
            bot.effort = Some(level).filter(|level| !level.is_empty());
        }
        if let Some(account) = given(&edit.account) {
            if !account.is_empty() && !crate::accounts::names(self.accounts.dir(), kind).contains(&account) {
                return Err(Refusal::new(Code::NotFound, format!("{name} has no account {account}")));
            }
            bot.account = Some(account).filter(|account| !account.is_empty());
        }
        bot.chat_apps = chat_apps_of(edit.chat_apps.as_deref(), bot.chat_apps)?;
        needs_a_model(kind, bot)
    }

    pub(in crate::app) fn bot_new(&mut self, new: &NewBot) -> Done<usize> {
        let dir =
            self.panels.bots.dir.clone().ok_or_else(|| Refusal::new(Code::Unavailable, "this server keeps no bots"))?;
        if self.panels.bots.live.len() >= MAX_BOTS {
            let text = format!("there are {MAX_BOTS} bots already, the most there can be; remove one first");
            return Err(Refusal::new(Code::Conflict, text));
        }
        let name = new.name.trim();
        let others: Vec<&Bot> = self.panels.bots.live.iter().map(|live| &live.bot).collect();
        if let Some(found) = crate::bots::name_refusal(name, &others) {
            return Err(problem(&found));
        }
        let kind = self.bot_kind(new.agent.as_deref())?;
        let access = self.bot_access(kind, new.access.as_deref())?;
        let id = crate::bots::new_id(name, &others, &|id| folder::of(&dir, id).exists());
        let mut bot = Bot {
            id,
            name: name.to_string(),
            agent: kind.name().to_string(),
            model: None,
            effort: None,
            account: None,
            access,
            created: crate::time::now(),
            conversation: None,
            follow: new.follow.unwrap_or(true),
            chat_apps: ChatApps::Away,
            briefed: None,
            memory_edits: 0,
        };
        let edit = EditBot {
            model: new.model.clone(),
            effort: new.effort.clone(),
            account: new.account.clone(),
            chat_apps: new.chat_apps.clone(),
            ..EditBot::default()
        };
        self.bot_settings(kind, &mut bot, &edit)?;
        let made =
            folder::create(&dir, &bot, &new.about).map_err(|error| Refusal::new(Code::Failed, error.to_string()))?;
        Ok(self.bot_made(bot, made, &new.about))
    }

    fn bot_made(&mut self, bot: Bot, folder: std::path::PathBuf, about: &str) -> usize {
        let key = self.panels.bots.take_key();
        self.panels.bots.live.push(Live::new(bot, folder, key));
        let at = self.panels.bots.live.len() - 1;
        self.publish_bot_change(at, "created");
        if !about.trim().is_empty() {
            self.start_setup(at);
        }
        at
    }

    pub(in crate::app) fn bot_edit(&mut self, edit: &EditBot) -> Done<usize> {
        let at = self.bot_at(&edit.bot)?;
        let mut bot = self.panels.bots.live[at].bot.clone();
        let kind = Known::of(&bot.agent)
            .ok_or_else(|| Refusal::new(Code::Failed, format!("{} is not an agent", bot.agent)))?;
        if clean(edit.agent.as_deref()).is_some_and(|agent| agent != bot.agent) {
            let text = "a bot keeps its agent: another agent cannot continue its conversation; make another bot";
            return Err(Refusal::new(Code::Malformed, text));
        }
        if let Some(name) = clean(edit.name.as_deref()) {
            let others: Vec<&Bot> =
                self.panels.bots.live.iter().filter(|live| live.bot.id != bot.id).map(|live| &live.bot).collect();
            if let Some(found) = crate::bots::name_refusal(&name, &others) {
                return Err(problem(&found));
            }
            bot.name = name;
        }
        if clean(edit.access.as_deref()).is_some() {
            bot.access = self.bot_access(kind, edit.access.as_deref())?;
        }
        self.bot_settings(kind, &mut bot, edit)?;
        let noted = write_notes(&self.panels.bots.live[at].folder, edit)?;
        let followed = self.panels.bots.live[at].bot.follow;
        bot.follow = edit.follow.unwrap_or(followed);
        let remembered = u64::from(edit.memory.is_some());
        bot.memory_edits += remembered;
        if let Err(refusal) = save_edited(&self.panels.bots.live[at].folder, &bot, noted) {
            self.panels.bots.live[at].bot.memory_edits += remembered;
            return Err(refusal);
        }
        self.panels.bots.live[at].bot = bot;
        self.follow_changed(at, followed);
        self.publish_bot_change(at, "edited");
        Ok(at)
    }
}

fn write_notes(folder: &Path, edit: &EditBot) -> Done<Option<&'static str>> {
    let failed = |error: std::io::Error| Refusal::new(Code::Failed, error.to_string());
    if let Some(about) = &edit.about {
        folder::write_about(folder, about).map_err(failed)?;
    }
    if let Some(memory) = &edit.memory {
        folder::write_memory(folder, memory).map_err(failed)?;
    }
    Ok(match (edit.about.is_some(), edit.memory.is_some()) {
        (true, true) => Some("the about text and the memory were"),
        (true, false) => Some("the about text was"),
        (false, true) => Some("the memory was"),
        (false, false) => None,
    })
}

fn save_edited(folder: &Path, bot: &Bot, noted: Option<&str>) -> Done<()> {
    folder::save(folder, bot).map_err(|error| {
        let text = if let Some(noted) = noted {
            format!("{noted} saved but the settings were not: {error}")
        } else {
            format!("the settings were not saved: {error}")
        };
        Refusal::new(Code::Failed, text)
    })
}

fn last(line: &chat::Line) -> BotLast {
    let kind = if line.kind == chat::Kind::You { "you" } else { "bot" };
    BotLast { kind: kind.to_string(), text: line.text.chars().take(crate::bots::LAST_CHARS).collect(), at: line.at }
}
