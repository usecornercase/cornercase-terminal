use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::Fields;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", content = "entry", rename_all = "kebab-case")]
pub(crate) enum BotRequest {
    Agents,
    New(NewBot),
    List,
    Edit(EditBot),
    About(BotTarget),
    Memory(BotTarget),
    Show(BotTarget),
    Remove(BotTarget),
    Reset(BotTarget),
    Seen(BotTarget),
    Stop(BotTarget),
    Setup(BotTarget),
    Say(BotSay),
    History(BotHistory),
    SecretSet(SecretSet),
    SecretList(BotTarget),
    SecretRemove(SecretRemove),
    SecretGet(SecretGet),
    SecretAsk(SecretAsk),
    SecretGive(SecretGive),
    SecretDecline(SecretAnswer),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SecretAsk {
    pub(crate) names: Vec<String>,
    pub(crate) about: String,
    pub(crate) replace: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SecretGive {
    pub(crate) bot: String,
    pub(crate) ask: u64,
    pub(crate) values: BTreeMap<String, crate::issues::Secret>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SecretAnswer {
    pub(crate) bot: String,
    pub(crate) ask: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AskInfo {
    pub(crate) id: u64,
    pub(crate) names: Vec<String>,
    pub(crate) about: String,
    pub(crate) replace: bool,
    pub(crate) routine: Option<u64>,
    pub(crate) missing: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SecretSet {
    pub(crate) bot: String,
    pub(crate) name: String,
    pub(crate) value: crate::issues::Secret,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SecretRemove {
    pub(crate) bot: String,
    pub(crate) name: Option<String>,
    pub(crate) all: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SecretGet {
    pub(crate) name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct NewBot {
    pub(crate) name: String,
    pub(crate) agent: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) account: Option<String>,
    pub(crate) access: Option<String>,
    pub(crate) follow: Option<bool>,
    pub(crate) chat_apps: Option<String>,
    pub(crate) about: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct EditBot {
    pub(crate) bot: String,
    pub(crate) name: Option<String>,
    pub(crate) agent: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) account: Option<String>,
    pub(crate) access: Option<String>,
    pub(crate) follow: Option<bool>,
    pub(crate) chat_apps: Option<String>,
    pub(crate) about: Option<String>,
    pub(crate) memory: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct BotTarget {
    pub(crate) bot: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct BotSay {
    pub(crate) bot: String,
    pub(crate) text: String,
    pub(crate) wait: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct BotHistory {
    pub(crate) bot: String,
    pub(crate) last: Option<usize>,
    pub(crate) after: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct BotLast {
    pub(crate) kind: String,
    pub(crate) text: String,
    pub(crate) at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct BotInfo {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) agent: String,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) account: Option<String>,
    pub(crate) access: String,
    pub(crate) follow: bool,
    pub(crate) following: Vec<u64>,
    pub(crate) chat_apps: String,
    pub(crate) state: String,
    pub(crate) held: usize,
    pub(crate) unread: usize,
    pub(crate) last: Option<BotLast>,
    pub(crate) folder: String,
    pub(crate) created: i64,
    pub(crate) routines: usize,
    pub(crate) next: Option<i64>,
    pub(crate) secrets: usize,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) secrets_unreadable: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) asks: Vec<AskInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AgentInfo {
    pub(crate) agent: String,
    pub(crate) title: String,
    pub(crate) installed: bool,
    pub(crate) bot: bool,
    pub(crate) why: Option<String>,
    pub(crate) accesses: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reads: Option<String>,
    pub(crate) efforts: Vec<String>,
    pub(crate) models: Vec<String>,
}

fn name_field<'a>(fields: &'a mut Fields, name: &str) -> &'a mut Fields {
    if crate::bots::secrets::name_refusal(name).is_none() {
        fields.add("name", name)
    } else {
        fields.add("name_bytes", name.len())
    }
}

impl BotRequest {
    fn secret_fields<'a>(&self, fields: &'a mut Fields) -> &'a mut Fields {
        match self {
            Self::SecretSet(set) => {
                fields.add("action", "secret-set").add("bot", &set.bot);
                if crate::bots::secrets::logged_name(&set.name, &[&set.value.0]) == set.name {
                    name_field(fields, &set.name);
                } else {
                    fields.add("name_bytes", set.name.len());
                }
                fields.add("bytes", set.value.0.len())
            }
            Self::SecretList(target) => fields.add("action", "secret-list").add("bot", &target.bot),
            Self::SecretRemove(remove) => {
                fields.add("action", "secret-remove").add("bot", &remove.bot);
                if let Some(name) = &remove.name {
                    name_field(fields, name);
                }
                fields.add("all", remove.all)
            }
            Self::SecretGet(get) => name_field(fields.add("action", "secret-get"), &get.name),
            Self::SecretAsk(ask) => {
                fields.add("action", "secret-ask").add("names", ask.names.len());
                fields.add("about_bytes", ask.about.len()).add("replace", ask.replace)
            }
            Self::SecretGive(give) => {
                fields.add("action", "secret-give").add("bot", &give.bot).add("ask", give.ask);
                fields.add("values", give.values.len())
            }
            Self::SecretDecline(answer) => {
                fields.add("action", "secret-decline").add("bot", &answer.bot).add("ask", answer.ask)
            }
            _ => fields,
        }
    }

    pub(super) fn fields(&self, fields: &mut Fields) {
        match self {
            Self::Agents => fields.add("action", "agents"),
            Self::List => fields.add("action", "list"),
            Self::New(new) => {
                fields.add("action", "new").add("name", &new.name).maybe("agent", new.agent.as_deref());
                fields.maybe("access", new.access.as_deref()).maybe("follow", new.follow);
                fields.maybe("chat_apps", new.chat_apps.as_deref()).add("about_bytes", new.about.len())
            }
            Self::Edit(edit) => {
                fields.add("action", "edit").add("bot", &edit.bot).maybe("name", edit.name.as_deref());
                fields.maybe("access", edit.access.as_deref()).maybe("follow", edit.follow);
                fields.maybe("chat_apps", edit.chat_apps.as_deref());
                fields.maybe("about_bytes", edit.about.as_ref().map(String::len));
                fields.maybe("memory_bytes", edit.memory.as_ref().map(String::len))
            }
            Self::About(target) => fields.add("action", "about").add("bot", &target.bot),
            Self::Memory(target) => fields.add("action", "memory").add("bot", &target.bot),
            Self::Show(target) => fields.add("action", "show").add("bot", &target.bot),
            Self::Remove(target) => fields.add("action", "remove").add("bot", &target.bot),
            Self::Reset(target) => fields.add("action", "reset").add("bot", &target.bot),
            Self::Seen(target) => fields.add("action", "seen").add("bot", &target.bot),
            Self::Stop(target) => fields.add("action", "stop").add("bot", &target.bot),
            Self::Setup(target) => fields.add("action", "setup").add("bot", &target.bot),
            Self::Say(say) => {
                fields.add("action", "say").add("bot", &say.bot).add("bytes", say.text.len()).add("wait", say.wait)
            }
            Self::History(history) => {
                fields.add("action", "history").add("bot", &history.bot).maybe("last", history.last);
                fields.maybe("after", history.after)
            }
            Self::SecretSet(_)
            | Self::SecretList(_)
            | Self::SecretRemove(_)
            | Self::SecretGet(_)
            | Self::SecretAsk(_)
            | Self::SecretGive(_)
            | Self::SecretDecline(_) => self.secret_fields(fields),
        };
    }
}

#[cfg(test)]
mod tests;
