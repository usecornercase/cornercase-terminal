use serde::{Deserialize, Serialize};

use super::Fields;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub(crate) enum BotRequest {
    Agents,
    New(NewBot),
    List,
    Edit(EditBot),
    Remove(BotTarget),
    Reset(BotTarget),
    Seen(BotTarget),
    Stop(BotTarget),
    Say(BotSay),
    History(BotHistory),
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
    pub(crate) state: String,
    pub(crate) held: usize,
    pub(crate) unread: usize,
    pub(crate) last: Option<BotLast>,
    pub(crate) folder: String,
    pub(crate) created: i64,
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
    pub(crate) efforts: Vec<String>,
    pub(crate) models: Vec<String>,
}

impl BotRequest {
    pub(super) fn fields(&self, fields: &mut Fields) {
        match self {
            Self::Agents => fields.add("action", "agents"),
            Self::List => fields.add("action", "list"),
            Self::New(new) => {
                fields.add("action", "new").add("name", &new.name).maybe("agent", new.agent.as_deref());
                fields.maybe("access", new.access.as_deref()).add("about_bytes", new.about.len())
            }
            Self::Edit(edit) => {
                fields.add("action", "edit").add("bot", &edit.bot).maybe("name", edit.name.as_deref());
                fields.maybe("access", edit.access.as_deref())
            }
            Self::Remove(target) => fields.add("action", "remove").add("bot", &target.bot),
            Self::Reset(target) => fields.add("action", "reset").add("bot", &target.bot),
            Self::Seen(target) => fields.add("action", "seen").add("bot", &target.bot),
            Self::Stop(target) => fields.add("action", "stop").add("bot", &target.bot),
            Self::Say(say) => {
                fields.add("action", "say").add("bot", &say.bot).add("bytes", say.text.len()).add("wait", say.wait)
            }
            Self::History(history) => {
                fields.add("action", "history").add("bot", &history.bot).maybe("last", history.last);
                fields.maybe("after", history.after)
            }
        };
    }
}

#[cfg(test)]
mod tests;
