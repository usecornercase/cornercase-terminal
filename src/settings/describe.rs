use serde_json::Value;

use crate::agents;
use crate::config::{Config, Sidebar, Tabs, WorkingIcon};
use crate::control::config::{Choice, Connection, ConnectionKind, Kind, Setting, State};
use crate::host::notify;
use crate::messenger::Via;

use super::{Detail, PickItem, Row, Settings, Status};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Target {
    Row(Row),
    Tabs,
    Mode(String),
    Extra(String),
}

fn switch(row: &Row) -> Option<&'static str> {
    Some(match row {
        Row::PullRequests => "pull_requests",
        Row::MergedPullRequests => "merged_pull_requests",
        Row::Submit => "submit",
        Row::Trust => "accept_trust_prompts",
        Row::Resume => "resume_agents",
        Row::KeepAwake => "keep_awake",
        Row::IconBar => "icon_bar",
        Row::AgentsSection => "agents_section",
        Row::Counts => "counts",
        Row::TodoDot => "todo_dot",
        Row::DimPanes => "dim_inactive_panes",
        Row::Detail(Detail::Model) => "model",
        Row::Detail(Detail::Context) => "context",
        Row::Detail(Detail::Memory) => "memory",
        Row::Detail(Detail::Account) => "account_mark",
        Row::Detail(Detail::AccountSwitcher) => "account_switcher",
        Row::Detail(Detail::AccountPicker) => "account_picker",
        Row::Sound => "sound",
        Row::SoundSubagents => "sound_subagents",
        Row::Updates => "check_updates",
        Row::StarPrompt => "star_prompt",
        Row::Remote => "remote",
        _ => return None,
    })
}

fn text(row: &Row) -> Option<&'static str> {
    Some(match row {
        Row::Folder => "worktrees_dir",
        Row::GitlabUrl => "gitlab_url",
        Row::JiraSite => "jira_site",
        Row::JiraEmail => "jira_email",
        Row::JiraJql => "jira_jql",
        Row::PlaneWorkspace => "plane_workspace",
        Row::PlaneUrl => "plane_url",
        Row::PlaneFilter => "plane_filter",
        Row::AsanaWorkspace => "asana_workspace",
        Row::AsanaProjects => "asana_projects",
        Row::SoundWaiting => "sound_waiting",
        Row::SoundDone => "sound_done",
        _ => return None,
    })
}

fn choice(row: &Row) -> Option<&'static str> {
    Some(match row {
        Row::DefaultAgent => "agent",
        Row::Sidebar => "sidebar",
        Row::Tabs => "tabs",
        Row::WorkingIcon => "working_icon",
        Row::Notifications => "desktop_notifications",
        _ => return None,
    })
}

fn items(choices: Vec<(&'static str, &'static str)>) -> Vec<PickItem> {
    choices
        .into_iter()
        .map(|(value, note)| PickItem { value: value.into(), note: note.into(), dangerous: false })
        .collect()
}

fn json(config: &Config, key: &str) -> Value {
    serde_json::to_value(config).ok().and_then(|keys| keys.get(key).cloned()).unwrap_or(Value::Null)
}

fn strings(values: &[&str]) -> Value {
    Value::Array(values.iter().map(|v| Value::String((*v).into())).collect())
}

impl Settings {
    fn base(&self, row: &Row, key: &str, kind: Kind) -> Setting {
        let text = self.row_text(row);
        Setting {
            key: key.into(),
            page: row.page().name().into(),
            section: row.section().into(),
            label: text.label,
            kind,
            value: json(&self.config, key),
            default: json(&Config::default(), key),
            shown: text.value,
            note: text.note,
            dangerous: text.dangerous,
            ..Setting::default()
        }
    }

    fn choose_from(&self, row: &Row, key: &str, items: Vec<PickItem>, value: String) -> Setting {
        let mut setting = self.base(row, key, Kind::Choice);
        setting.value = Value::String(value);
        setting.choices =
            items.into_iter().map(|i| Choice { value: i.value, note: i.note, dangerous: i.dangerous }).collect();
        setting.question = Some(Self::question(row));
        setting
    }

    fn setting_of(&self, row: &Row) -> Option<Setting> {
        let config = &self.config;
        if let Some(key) = switch(row) {
            return Some(self.base(row, key, Kind::Switch));
        }
        if let Some(key) = text(row) {
            let mut setting = self.base(row, key, Kind::Text);
            setting.field = Some(Self::field_label(row));
            return Some(setting);
        }
        let (key, items, value) = match row {
            Row::DefaultAgent => ("agent", self.agent_items(), config.agent.clone()),
            Row::Sidebar => ("sidebar", items(Sidebar::choices()), Sidebar::from_setting(&config.sidebar).id().into()),
            Row::Tabs => ("tabs", items(Tabs::choices()), Tabs::from_setting(&config.tabs).id().into()),
            Row::WorkingIcon => (
                "working_icon",
                items(WorkingIcon::choices()),
                WorkingIcon::from_setting(&config.working_icon).id().into(),
            ),
            Row::Notifications => {
                ("desktop_notifications", items(notify::choices()), config.desktop_notifications.trim().to_lowercase())
            }
            Row::Fetch => {
                let mut setting = self.base(row, "fetch_minutes", Kind::Number);
                setting.field = Some(Self::field_label(row));
                return Some(setting);
            }
            Row::Prefix => {
                let mut setting = self.base(row, "prefix_key", Kind::Key);
                setting.field = Some("press the key, or write it such as ctrl+] or f12; empty turns it off".into());
                return Some(setting);
            }
            _ => return None,
        };
        Some(self.choose_from(row, choice(row).unwrap_or(key), items, value))
    }

    fn kind_settings(&self, kind: &str) -> Vec<(Setting, Target)> {
        let config = &self.config;
        let (args, modes) = (agents::args(config, kind), agents::modes(config, kind));
        let row = Row::Kind(kind.to_string());
        let mode = agents::mode_of(kind, &args, &modes);
        let extra = agents::join_args(&agents::extra_args(kind, &args, &modes));
        let base = |key: String, label: String, kind: Kind| Setting {
            key,
            page: row.page().name().into(),
            section: row.section().into(),
            label,
            kind,
            default: Value::String(String::new()),
            ..Setting::default()
        };
        let mut found = Vec::new();
        if !modes.is_empty() {
            let name = mode.clone().unwrap_or_else(|| "default".into());
            let mut setting = base(format!("agents.{kind}.mode"), format!("{kind} mode"), Kind::Choice);
            setting.value = Value::String(name.clone());
            setting.default = Value::String("default".into());
            setting.shown.clone_from(&name);
            setting.dangerous = agents::is_dangerous(&name);
            setting.choices = self
                .mode_items(kind)
                .into_iter()
                .map(|i| Choice { value: i.value, note: i.note, dangerous: i.dangerous })
                .collect();
            setting.question = Some(Self::question(&Row::Kind(kind.to_string())));
            found.push((setting, Target::Mode(kind.to_string())));
        }
        let mut setting = base(format!("agents.{kind}.extra"), format!("{kind} extra arguments"), Kind::Text);
        setting.value = Value::String(extra.clone());
        setting.shown = extra;
        setting.field = Some(Self::field_label(&row));
        found.push((setting, Target::Extra(kind.to_string())));
        found
    }

    pub(super) fn entries(&self) -> Vec<(Setting, Target)> {
        let mut found = Vec::new();
        let mut tabs = false;
        for row in self.every_row() {
            match &row {
                Row::Tab(_) if tabs => {}
                Row::Tab(_) => {
                    tabs = true;
                    found.push(self.tabs_entry(&row));
                }
                Row::Kind(_) => {}
                Row::AddAgent => found.extend(self.agent_entries()),
                row => found.extend(self.setting_of(row).map(|setting| (setting, Target::Row(row.clone())))),
            }
        }
        found
    }

    fn tabs_entry(&self, row: &Row) -> (Setting, Target) {
        let shown = self.shown_tabs();
        let mut setting = self.base(row, "issue_tabs", Kind::List);
        setting.label = "sources shown".into();
        setting.shown = shown.join(", ");
        setting.value = strings(&shown);
        setting.note = "every source together is `all`; the order is the order of the tabs".into();
        setting.choices = super::TAB_IDS.iter().map(|id| Choice { value: (*id).into(), ..Choice::default() }).collect();
        (setting, Target::Tabs)
    }

    fn agent_entries(&self) -> Vec<(Setting, Target)> {
        let mut kinds = self.listed_kinds();
        let rest: Vec<String> = agents::kinds(&self.config).into_iter().filter(|k| !kinds.contains(k)).collect();
        kinds.extend(rest);
        kinds.iter().flat_map(|kind| self.kind_settings(kind)).collect()
    }

    pub(crate) fn describe(&self) -> Vec<Setting> {
        self.entries().into_iter().map(|(setting, _)| setting).collect()
    }

    pub(crate) fn connections(&self) -> Vec<Connection> {
        let mut found = self.tracker_connections();
        found.push(self.browsers_connection());
        for via in Via::ALL {
            found.extend(self.messenger_connections(via));
        }
        found
    }

    fn tracker_connections(&self) -> Vec<Connection> {
        let mut found = Vec::new();
        for (source, status) in &self.tokens {
            let row = Row::Token(*source);
            let text = self.row_text(&row);
            let (state, account, from, env) = match status {
                Status::Missing => (State::Missing, None, None, None),
                Status::Saved(account) => {
                    (State::Connected, account.as_ref().map(crate::issues::Account::describe), Some("saved"), None)
                }
                Status::Env => (State::Connected, None, Some("env"), source.token_env()),
            };
            let (page, section) = place(&row);
            found.push(Connection {
                key: source.id().into(),
                kind: ConnectionKind::Tracker,
                page,
                section,
                label: text.label,
                note: text.note,
                state,
                account,
                from: from.map(Into::into),
                env: env.map(Into::into),
                field: Some(format!("{} {}", source.name(), source.token_name())),
                removable: self.removal_of_row(&row).is_some(),
                ..Connection::default()
            });
        }
        found
    }

    fn browsers_connection(&self) -> Connection {
        let row = Row::Browsers;
        let text = self.row_text(&row);
        let (page, section) = place(&row);
        Connection {
            key: "browsers".into(),
            kind: ConnectionKind::Browsers,
            page,
            section,
            label: text.label,
            note: text.note,
            state: if self.browsers > 0 { State::Connected } else { State::Missing },
            count: Some(self.browsers),
            removable: self.browsers > 0,
            ..Connection::default()
        }
    }

    fn messenger_connections(&self, via: Via) -> [Connection; 2] {
        let messenger = self.messenger(via);
        let (bot, chat) = (Row::Bot(via), Row::Chat(via));
        let text = self.row_text(&bot);
        let (page, section) = place(&bot);
        let state = match (&messenger.bot, messenger.refused) {
            (None, _) => State::Missing,
            (Some(_), true) => State::Refused,
            (Some(_), false) => State::Connected,
        };
        let bot_connection = Connection {
            key: via.target().into(),
            kind: ConnectionKind::Bot,
            page: page.clone(),
            section: section.clone(),
            label: format!("{} {}", via.name(), text.label),
            note: text.note,
            state,
            account: messenger.bot.clone(),
            field: Some(via.token_field().into()),
            removable: self.removal_of_row(&bot).is_some(),
            ..Connection::default()
        };
        let text = self.row_text(&chat);
        let chat_connection = Connection {
            key: format!("{}.chat", via.target()),
            kind: ConnectionKind::Chat,
            page,
            section,
            label: format!("{} {}", via.name(), text.label),
            note: text.note,
            state: if messenger.chat.is_some() { State::Connected } else { State::Missing },
            account: messenger.chat.clone(),
            removable: self.removal_of_row(&chat).is_some(),
            ..Connection::default()
        };
        [bot_connection, chat_connection]
    }
}

fn place(row: &Row) -> (String, String) {
    (row.page().name().to_string(), row.section().to_string())
}
