use serde_json::json;

use super::{Asker, Handled, Pending, Reply, Stage, json};
use crate::app::App;
use crate::bots::chat::Line;
use crate::control::Done;
use crate::control::bots::{BotHistory, BotRequest, BotSay, BotTarget};
use crate::control::contract::{Code, Refusal};

impl App {
    pub(super) fn bot_request(&mut self, client: u64, asker: Asker, request: BotRequest) -> Handled {
        match request {
            BotRequest::Agents => Ok(Some(json(&json!({ "agents": self.bot_agent_infos() })))),
            BotRequest::List => Ok(Some(json(&json!({ "bots": self.bot_infos() })))),
            BotRequest::New(new) => {
                let at = self.bot_new(&new)?;
                Ok(Some(json(&self.bot_info(at))))
            }
            BotRequest::Edit(edit) => {
                let at = self.bot_edit(&edit)?;
                Ok(Some(json(&self.bot_info(at))))
            }
            BotRequest::Say(say) => self.bot_say_request(client, &say),
            BotRequest::History(history) => self.bot_history_request(&history),
            BotRequest::SecretSet(_)
            | BotRequest::SecretList(_)
            | BotRequest::SecretRemove(_)
            | BotRequest::SecretGet(_)
            | BotRequest::SecretAsk(_)
            | BotRequest::SecretGive(_)
            | BotRequest::SecretDecline(_) => self.bot_secret_request(client, asker, request),
            other => self.bot_target_request(other),
        }
    }

    fn bot_secret_request(&mut self, client: u64, asker: Asker, request: BotRequest) -> Handled {
        let answer = match request {
            BotRequest::SecretSet(set) => self.bot_secret_set(Some(client), &set.bot, (&set.name, &set.value.0))?,
            BotRequest::SecretList(target) => self.bot_secret_list(Some(client), &target.bot)?,
            BotRequest::SecretRemove(remove) => {
                self.bot_secret_remove(Some(client), &remove.bot, (remove.name.as_deref(), remove.all))?
            }
            BotRequest::SecretGet(get) => self.bot_secret_get(Some(client), &get.name)?,
            BotRequest::SecretAsk(ask) => {
                let run = self.ask_run(client, asker.bot);
                self.bot_secret_ask(Some(client), run, &ask)?
            }
            BotRequest::SecretGive(give) => self.bot_secret_give(Some(client), &give)?,
            BotRequest::SecretDecline(answer) => self.bot_secret_decline(Some(client), &answer)?,
            _ => return Err(Refusal::new(Code::Failed, "not a secret request")),
        };
        Ok(Some(json(&answer)))
    }

    fn bot_target_request(&mut self, request: BotRequest) -> Handled {
        let id =
            |app: &App, target: &BotTarget| app.bot_at(&target.bot).map(|at| app.panels.bots.live[at].bot.id.clone());
        let answer = match request {
            BotRequest::Remove(target) => {
                let (id, entries) = self.bot_remove(&target.bot)?;
                let mut value = self.queue_removal(entries);
                value["bot"] = json!(id);
                value
            }
            BotRequest::Reset(target) => {
                self.bot_reset(&target.bot)?;
                json!({ "bot": id(self, &target)? })
            }
            BotRequest::Seen(target) => {
                self.bot_seen(&target.bot)?;
                json!({ "bot": id(self, &target)? })
            }
            BotRequest::Stop(target) => {
                let stopped = self.bot_stop(&target.bot)?;
                json!({ "bot": id(self, &target)?, "stopped": stopped })
            }
            BotRequest::Setup(target) => {
                let (at, line) = self.bot_setup(&target.bot)?;
                json!({ "bot": self.panels.bots.live[at].bot.id, "line": line })
            }
            BotRequest::About(target) => {
                let (bot, about) = self.bot_about(&target.bot)?;
                json!({ "bot": bot, "about": about })
            }
            _ => return Err(Refusal::new(Code::Failed, "not a bot request")),
        };
        Ok(Some(answer))
    }

    fn bot_say_request(&mut self, client: u64, say: &BotSay) -> Handled {
        let (at, line) = self.bot_say(&say.bot, &say.text, say.wait)?;
        let bot = self.panels.bots.live[at].bot.id.clone();
        if !say.wait {
            return Ok(Some(json(&json!({ "bot": bot, "line": line }))));
        }
        let key = self.clients.requests.key();
        let pending = Pending {
            client: Some(client),
            key,
            timeout: None,
            done: Done::default(),
            stage: Stage::Bot { bot, asked: line.id },
        };
        self.clients.requests.pending.push(pending);
        Ok(None)
    }

    fn bot_history_request(&self, history: &BotHistory) -> Handled {
        let at = self.bot_at(&history.bot)?;
        let live = &self.panels.bots.live[at];
        let path = live.folder.join(crate::bots::folder::CHAT_FILE);
        let after = history.after.unwrap_or(0);
        let mut lines: Vec<Line> = crate::bots::chat::read(&path).into_iter().filter(|line| line.id > after).collect();
        if let Some(last) = history.last {
            lines = lines.split_off(lines.len().saturating_sub(last));
        }
        Ok(Some(json(&json!({ "bot": live.bot.id, "name": live.bot.name, "lines": lines }))))
    }

    pub(in crate::app) fn bot_reply(&self, bot: &str, asked: u64) -> Option<Reply> {
        let at = self.panels.bots.by_id(bot)?;
        let line = self.bot_answer(at, asked)?;
        Some(Ok(json(&json!({ "bot": bot, "asked": asked, "line": line }))))
    }
}
