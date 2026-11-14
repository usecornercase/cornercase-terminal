use serde_json::Value;

use super::{Action, PickItem, Row, Settings};
use crate::agents::{self, Known};
use crate::config::{Config, DEFAULT_RECAP_EFFORT, DEFAULT_RECAP_LANGUAGE};
use crate::control::config::{Choice, Kind, Setting};

const AUTO: &str = "auto";
const DEFAULT: &str = "default";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecapRow {
    Agent,
    Account,
    Model,
    Effort,
    Language,
    Instructions,
}

impl RecapRow {
    pub(super) const ALL: [Self; 6] =
        [Self::Agent, Self::Account, Self::Model, Self::Effort, Self::Language, Self::Instructions];

    pub(super) fn key(self) -> &'static str {
        match self {
            Self::Agent => "recap_agent",
            Self::Account => "recap_account",
            Self::Model => "recap_model",
            Self::Effort => "recap_effort",
            Self::Language => "recap_language",
            Self::Instructions => "recap_instructions",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Agent => "recap agent",
            Self::Account => "recap account",
            Self::Model => "recap model",
            Self::Effort => "recap effort",
            Self::Language => "recap language",
            Self::Instructions => "recap instructions",
        }
    }

    fn note(self) -> &'static str {
        match self {
            Self::Agent => "writes the Recap panel's summaries: Claude Code or Codex",
            Self::Account => "yours alone, not shared",
            Self::Model => "the agent's own models",
            Self::Effort => "low keeps it quick and cheap",
            Self::Language => "auto: the language of your prompts",
            Self::Instructions => "added to the prompt, such as group by client",
        }
    }

    pub(super) fn picks(self) -> bool {
        matches!(self, Self::Agent | Self::Account | Self::Model | Self::Effort)
    }

    pub(crate) fn question(self) -> &'static str {
        match self {
            Self::Agent => "Which agent writes the recaps?",
            Self::Account => "Which account does it use?",
            Self::Model => "Which model writes the recaps?",
            Self::Effort => "How much effort does it spend?",
            Self::Language | Self::Instructions => "",
        }
    }

    pub(crate) fn field(self) -> &'static str {
        match self {
            Self::Language => "recap language (auto is the language of your prompts)",
            _ => "recap instructions, added to the prompt, such as group by client",
        }
    }
}

fn text(config: &Config, row: RecapRow) -> &str {
    match row {
        RecapRow::Agent => &config.recap_agent,
        RecapRow::Account => &config.recap_account,
        RecapRow::Model => &config.recap_model,
        RecapRow::Effort => &config.recap_effort,
        RecapRow::Language => &config.recap_language,
        RecapRow::Instructions => &config.recap_instructions,
    }
}

fn with(config: &Config, row: RecapRow, value: String) -> Config {
    let mut config = config.clone();
    match row {
        RecapRow::Agent => config.recap_agent = value,
        RecapRow::Account => config.recap_account = value,
        RecapRow::Model => config.recap_model = value,
        RecapRow::Effort => config.recap_effort = value,
        RecapRow::Language => config.recap_language = value,
        RecapRow::Instructions => config.recap_instructions = value,
    }
    config
}

fn stored(value: &str, word: &str) -> String {
    if value == word { String::new() } else { value.to_string() }
}

impl Settings {
    fn recap_kind(&self) -> Known {
        let usable = |name: &str| Known::of(name.trim()).filter(|kind| kind.headless().is_some());
        usable(&self.config.recap_agent).or_else(|| usable(&self.config.agent)).unwrap_or(Known::Claude)
    }

    pub(super) fn recap_shown(&self, row: RecapRow) -> String {
        let value = text(&self.config, row).trim();
        let empty = match row {
            RecapRow::Agent => format!("{AUTO} ({})", self.recap_kind().name()),
            RecapRow::Account | RecapRow::Model => DEFAULT.to_string(),
            RecapRow::Effort => DEFAULT_RECAP_EFFORT.to_string(),
            RecapRow::Language => DEFAULT_RECAP_LANGUAGE.to_string(),
            RecapRow::Instructions => "none".to_string(),
        };
        if value.is_empty() || (row == RecapRow::Agent && value == AUTO) { empty } else { value.to_string() }
    }

    pub(super) fn recap_row(&self, row: RecapRow) -> (String, String, String, bool) {
        (row.label().into(), self.recap_shown(row), row.note().into(), false)
    }

    fn recap_value(&self, row: RecapRow) -> String {
        let value = text(&self.config, row).trim();
        match row {
            RecapRow::Agent if value.is_empty() => AUTO.into(),
            RecapRow::Account | RecapRow::Model if value.is_empty() => DEFAULT.into(),
            RecapRow::Effort if value.is_empty() => DEFAULT_RECAP_EFFORT.into(),
            _ => value.to_string(),
        }
    }

    pub(super) fn recap_items(&self, row: RecapRow) -> Vec<PickItem> {
        let item = |value: &str, note: &str| PickItem { value: value.into(), note: note.into(), dangerous: false };
        let kind = self.recap_kind();
        match row {
            RecapRow::Agent => vec![
                item(AUTO, "the default agent when it can, else Claude Code"),
                item("claude", "Claude Code"),
                item("codex", "Codex"),
            ],
            RecapRow::Account => {
                let mut items = vec![item(DEFAULT, "the agent's own login")];
                items.extend(self.accounts.names(kind).into_iter().map(|account| item(&account.name, "")));
                items
            }
            RecapRow::Model => {
                let mut items = vec![item(DEFAULT, "the agent's default")];
                items.extend(kind.models().iter().map(|model| item(model, "")));
                items
            }
            RecapRow::Effort => agents::effort_levels(kind.name())
                .unwrap_or(&[DEFAULT_RECAP_EFFORT])
                .iter()
                .map(|level| item(level, ""))
                .collect(),
            RecapRow::Language | RecapRow::Instructions => Vec::new(),
        }
    }

    pub(super) fn recap_current(&self, row: RecapRow) -> String {
        self.recap_value(row)
    }

    pub(super) fn activate_recap(&mut self, whole: Row, row: RecapRow) -> Action {
        if row.picks() {
            let items = self.recap_items(row);
            self.open_pick(whole, items);
            Action::None
        } else {
            self.start_edit(whole, text(&self.config, row).to_string())
        }
    }

    pub(super) fn recap_chosen(&mut self, row: RecapRow, value: &str) -> Action {
        let word = if row == RecapRow::Agent { AUTO } else { DEFAULT };
        let mut config = with(&self.config, row, stored(value, word));
        if row == RecapRow::Agent {
            config = self.recap_reset(config);
        }
        let notice = format!("{}: {value}", row.label());
        self.save(config, notice)
    }

    fn recap_reset(&self, mut config: Config) -> Config {
        let kind = Known::of(config.recap_agent.trim()).unwrap_or_else(|| self.recap_kind());
        if !config.recap_model.is_empty() && !kind.models().contains(&config.recap_model.as_str()) {
            config.recap_model.clear();
        }
        let levels = agents::effort_levels(kind.name()).unwrap_or(&[]);
        if !levels.contains(&config.recap_effort.as_str()) {
            config.recap_effort = DEFAULT_RECAP_EFFORT.to_string();
        }
        config.recap_account.clear();
        config
    }

    pub(super) fn recap_submitted(&mut self, row: RecapRow, value: &str) -> Action {
        self.edit = None;
        let value = value.trim().to_string();
        let notice =
            if value.is_empty() { format!("{} cleared", row.label()) } else { format!("{}: {value}", row.label()) };
        self.save(with(&self.config, row, value), notice)
    }

    pub(super) fn recap_setting(&self, whole: &Row, row: RecapRow) -> Setting {
        let mut setting = self.base(whole, row.key(), if row.picks() { Kind::Choice } else { Kind::Text });
        if row.picks() {
            setting.value = Value::String(self.recap_value(row));
            setting.default = Value::String(
                match row {
                    RecapRow::Agent => AUTO,
                    RecapRow::Effort => DEFAULT_RECAP_EFFORT,
                    _ => DEFAULT,
                }
                .into(),
            );
            setting.choices = self
                .recap_items(row)
                .into_iter()
                .map(|item| Choice { value: item.value, note: item.note, dangerous: false })
                .collect();
            setting.question = Some(row.question().into());
        } else {
            setting.field = Some(row.field().into());
        }
        setting
    }
}
