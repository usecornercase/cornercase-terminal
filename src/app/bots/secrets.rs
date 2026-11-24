use serde_json::{Value, json};

use crate::app::App;
use crate::bots::secrets::{self as keeping, Kept, Key, KeyState};
use crate::control::contract::{Code, Refusal};
use crate::log;

type Done<T> = Result<T, Refusal>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) enum Caller {
    Person,
    Bot { id: String, own: bool },
}

fn shown(name: &str) -> String {
    if keeping::name_refusal(name).is_none() { name.to_string() } else { String::from("of that name") }
}

pub(super) fn failed(text: impl Into<String>) -> Refusal {
    Refusal::new(Code::Failed, text)
}

impl App {
    pub(in crate::app) fn secrets_caller(&self, client: Option<u64>) -> Caller {
        let bots = &self.clients.bots;
        match client {
            Some(client) => match (bots.owner_of(client), bots.bot_of(client).or_else(|| bots.former_of(client))) {
                (Some(id), _) => Caller::Bot { id: id.to_string(), own: true },
                (None, Some(id)) => Caller::Bot { id: id.to_string(), own: false },
                (None, None) => Caller::Person,
            },
            None => Caller::Person,
        }
    }

    fn secrets_may_change(&self, client: Option<u64>) -> Done<()> {
        match self.secrets_caller(client) {
            Caller::Person => Ok(()),
            Caller::Bot { .. } => Err(failed(keeping::NO_CHANGE)),
        }
    }

    fn secrets_may_list(&self, client: Option<u64>, id: &str) -> Done<()> {
        match self.secrets_caller(client) {
            Caller::Bot { id: own, .. } if own != id => Err(failed(keeping::OWN_ONLY)),
            _ => Ok(()),
        }
    }

    fn secrets_key(&mut self) -> Done<Key> {
        let path = self.panels.bots.key_path.clone().ok_or_else(|| failed("this server keeps no bots"))?;
        if self.panels.bots.key == KeyState::Missing {
            match keeping::make_key(&path) {
                Ok(key) => self.panels.bots.key = KeyState::Ready(key),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    self.panels.bots.key = keeping::read_key(&path);
                    return Err(failed(format!(
                        "a key appeared at {} since cornercase started; restart cornercase so the bots' secrets are read with it",
                        path.display()
                    )));
                }
                Err(error) => return Err(failed(format!("cannot make the key: {error}"))),
            }
        }
        match &self.panels.bots.key {
            KeyState::Ready(key) => Ok(key.clone()),
            _ => Err(failed(format!("the key of the bots' secrets cannot be read: {}", path.display()))),
        }
    }

    fn secrets_unchanged(&self, at: usize) -> Done<()> {
        let live = &self.panels.bots.live[at];
        if keeping::load(&live.folder, &live.bot.id, &self.panels.bots.key) == live.secrets {
            return Ok(());
        }
        let id = &live.bot.id;
        Err(failed(format!(
            "the secrets of {id} changed on disk since cornercase read them: restart cornercase to read them, or cornercase bot secret remove {id} --all starts again"
        )))
    }

    fn secrets_retire(&mut self, at: usize, kept: &[keeping::Entry]) {
        let live = &mut self.panels.bots.live[at];
        if live.answering() {
            let gone = live.secrets.entries().iter().filter(|old| !kept.contains(old)).cloned();
            live.retired.extend(gone);
        }
    }

    pub(in crate::app) fn bot_settled(&mut self, at: usize) {
        let live = &mut self.panels.bots.live[at];
        if live.answering() {
            return;
        }
        live.retired.clear();
        let id = live.bot.id.clone();
        self.clients.bots.revoke_owners(&id);
    }

    pub(super) fn secrets_keep(&mut self, at: usize, entries: Vec<keeping::Entry>) -> Done<()> {
        self.secrets_unchanged(at)?;
        let key = self.secrets_key()?;
        self.secrets_retire(at, &entries);
        let live = &mut self.panels.bots.live[at];
        keeping::save(&live.folder, &live.bot.id, &key, &entries)
            .map_err(|error| failed(format!("cannot save the secrets of {}: {error}", live.bot.id)))?;
        live.secrets = Kept::Open(entries);
        self.publish_bot_change(at, "edited");
        Ok(())
    }

    pub(in crate::app) fn bot_secret_set(
        &mut self,
        client: Option<u64>,
        bot: &str,
        (name, value): (&str, &str),
    ) -> Done<Value> {
        self.secrets_may_change(client)?;
        let at = self.bot_at(bot)?;
        if let Some(why) = keeping::name_refusal(name).or_else(|| keeping::value_refusal(value)) {
            return Err(Refusal::new(Code::Malformed, why));
        }
        let live = &self.panels.bots.live[at];
        let id = live.bot.id.clone();
        if live.secrets.is_unreadable() {
            return Err(failed(keeping::unreadable(&id)));
        }
        let replaced = live.secrets.find(name).is_some();
        if !replaced && live.secrets.count() >= keeping::SECRETS_MAX {
            let text = format!("{id} has {} secrets, the most there can be; remove one first", keeping::SECRETS_MAX);
            return Err(Refusal::new(Code::Conflict, text));
        }
        let saved = crate::time::now();
        let entries = live.secrets.with(name, value, saved);
        self.secrets_keep(at, entries)?;
        let name_logged = keeping::logged_name(name, &[value]);
        log::info!("bots", "secret set", bot = id, name = name_logged, bytes = value.len(), replaced = replaced);
        self.asks_saved(at, &[name.to_string()]);
        Ok(json!({ "bot": id, "name": name, "saved": saved }))
    }

    pub(in crate::app) fn bot_secret_list(&self, client: Option<u64>, bot: &str) -> Done<Value> {
        let at = self.bot_at(bot)?;
        let live = &self.panels.bots.live[at];
        self.secrets_may_list(client, &live.bot.id)?;
        if live.secrets.is_unreadable() {
            return Err(failed(keeping::unreadable_hint(&live.bot.id)));
        }
        let secrets: Vec<Value> =
            live.secrets.entries().iter().map(|entry| json!({ "name": entry.name, "saved": entry.saved })).collect();
        Ok(json!({ "bot": live.bot.id, "key": "file", "secrets": secrets }))
    }

    pub(in crate::app) fn bot_secret_remove(
        &mut self,
        client: Option<u64>,
        bot: &str,
        (name, all): (Option<&str>, bool),
    ) -> Done<Value> {
        self.secrets_may_change(client)?;
        let at = self.bot_at(bot)?;
        if name.is_some() == all {
            return Err(Refusal::new(Code::Malformed, "name one secret, or --all"));
        }
        let live = &self.panels.bots.live[at];
        let id = live.bot.id.clone();
        let Some(name) = name else { return self.secrets_remove_all(at) };
        if live.secrets.is_unreadable() {
            return Err(failed(keeping::unreadable_hint(&id)));
        }
        if live.secrets.find(name).is_none() {
            return Err(Refusal::new(Code::NotFound, format!("{id} has no secret {}", shown(name))));
        }
        let entries = live.secrets.without(name);
        self.secrets_keep(at, entries)?;
        log::info!("bots", "secret removed", bot = id, name = name);
        self.asks_unsaved(at, Some(name));
        Ok(json!({ "bot": id, "removed": 1 }))
    }

    fn secrets_remove_all(&mut self, at: usize) -> Done<Value> {
        self.secrets_retire(at, &[]);
        let live = &mut self.panels.bots.live[at];
        keeping::erase(&live.folder).map_err(|error| failed(format!("cannot remove the secrets: {error}")))?;
        let (id, removed) = (live.bot.id.clone(), live.secrets.count());
        live.secrets = Kept::default();
        self.publish_bot_change(at, "edited");
        log::info!("bots", "secret removed", bot = id, name = "all", count = removed);
        self.asks_unsaved(at, None);
        Ok(json!({ "bot": id, "removed": removed }))
    }

    pub(in crate::app) fn bot_secret_get(&self, client: Option<u64>, name: &str) -> Done<Value> {
        let Caller::Bot { id, own: true } = self.secrets_caller(client) else {
            return Err(failed(keeping::ONLY_ITSELF));
        };
        let at = self.panels.bots.by_id(&id).ok_or_else(|| Refusal::new(Code::NotFound, format!("no bot {id}")))?;
        let live = &self.panels.bots.live[at];
        if !live.answering() {
            return Err(failed(keeping::ONLY_ITSELF));
        }
        if live.secrets.is_unreadable() {
            return Err(failed(keeping::unreadable(&id)));
        }
        let hint = "cornercase bot secret list shows them";
        let found = live.secrets.find(name);
        let missing = || Refusal::new(Code::NotFound, format!("no secret {} in {id}; {hint}", shown(name)));
        let found = found.ok_or_else(missing)?;
        let known: Vec<&str> =
            live.secrets.entries().iter().chain(&live.retired).map(|entry| entry.value.0.as_str()).collect();
        log::info!("bots", "secret get", bot = id, name = keeping::logged_name(name, &known));
        Ok(json!({ "bot": id, "name": name, "value": found.value.0 }))
    }
}
