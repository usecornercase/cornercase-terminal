use crate::agents::headless::Access;
use crate::agents::{self, Known};
use crate::app::App;
use crate::bots::{Bot, MAX_BOTS, NameProblem, chat, folder};
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
            state: String::from(if live.turn.is_some() { "thinking" } else { "idle" }),
            held: live.held.len(),
            unread: live.unread,
            last: live.last.as_ref().map(last),
            folder: live.folder.display().to_string(),
            created: bot.created,
        }
    }

    pub(in crate::app) fn bot_infos(&self) -> Vec<BotInfo> {
        (0..self.panels.bots.live.len()).map(|at| self.bot_info(at)).collect()
    }

    fn bot_kind(&self, asked: Option<&str>) -> Done<Known> {
        match clean(asked) {
            Some(name) => {
                Known::of(&name).ok_or_else(|| Refusal::new(Code::Malformed, format!("{name} is not an agent")))
            }
            None => Known::of(self.config.agent.trim())
                .filter(|kind| self.bot_support(*kind).bot)
                .ok_or_else(|| Refusal::new(Code::Malformed, "say which agent with --agent: claude or codex")),
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
        Ok(())
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
        let blank = Bot {
            id,
            name: name.to_string(),
            agent: kind.name().to_string(),
            model: None,
            effort: None,
            account: None,
            access,
            created: crate::time::now(),
            conversation: None,
        };
        let edit = EditBot {
            model: new.model.clone(),
            effort: new.effort.clone(),
            account: new.account.clone(),
            ..EditBot::default()
        };
        let mut bot = blank;
        self.bot_settings(kind, &mut bot, &edit)?;
        let made =
            folder::create(&dir, &bot, &new.about).map_err(|error| Refusal::new(Code::Failed, error.to_string()))?;
        self.panels.bots.live.push(Live::new(bot, made));
        let at = self.panels.bots.live.len() - 1;
        self.publish_bot_change(at, "created");
        Ok(at)
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
        let live = &mut self.panels.bots.live[at];
        live.bot = bot;
        let _ = folder::save(&live.folder, &live.bot);
        self.publish_bot_change(at, "edited");
        Ok(at)
    }
}

fn last(line: &chat::Line) -> BotLast {
    let kind = if line.kind == chat::Kind::You { "you" } else { "bot" };
    BotLast { kind: kind.to_string(), text: line.text.chars().take(crate::bots::LAST_CHARS).collect(), at: line.at }
}
