use serde_json::Value;

use crate::control::config::{Kind, Setting};
use crate::issues::Secret;
use crate::messenger::Via;
use crate::shortcuts::Prefix;

use super::describe::Target;
use super::{Action, Refused, Row, Settings, Status, TAB_IDS};

impl Settings {
    fn go_to(&mut self, row: &Row) {
        self.page = row.page();
        self.edit = None;
        self.pick = None;
        self.capturing = false;
        self.notice = None;
        if let Some(i) = self.rows().iter().position(|r| r == row) {
            self.cursor = i;
        }
    }

    fn connection_hint(&self, key: &str) -> Option<String> {
        let token = self.tokens.iter().any(|(s, _)| s.id() == key) || Via::ALL.iter().any(|v| v.target() == key);
        if token {
            Some(format!("`{key}` is a token: use the `secret` action (`cornercase config secret {key}`)"))
        } else if key == "browsers" || Via::ALL.iter().any(|v| format!("{}.chat", v.target()) == key) {
            Some(format!("`{key}` is a connection, not a setting: it can only be removed"))
        } else {
            None
        }
    }

    fn typed(setting: &Setting, value: &Value) -> Result<String, Refused> {
        let key = &setting.key;
        let wrong = |what: &str| Refused::Invalid(format!("`{key}` takes {what}"));
        match setting.kind {
            Kind::Switch => value.as_bool().map(|_| String::new()).ok_or_else(|| wrong("true or false")),
            Kind::Number => value.as_u64().map(|n| n.to_string()).ok_or_else(|| wrong("a whole number")),
            Kind::Choice => {
                let text = value.as_str().ok_or_else(|| wrong("text"))?;
                if setting.choices.iter().any(|c| c.value == text) {
                    Ok(text.to_string())
                } else {
                    let all: Vec<&str> = setting.choices.iter().map(|c| c.value.as_str()).collect();
                    Err(wrong(&format!("one of: {}", all.join(", "))))
                }
            }
            Kind::List => Err(wrong("a list of sources")),
            Kind::Text | Kind::Key | Kind::Other => value.as_str().map(String::from).ok_or_else(|| wrong("text")),
        }
    }

    fn submit_text(&mut self, input: String) -> Result<(), Refused> {
        if let Some(edit) = &mut self.edit {
            edit.input = input;
        }
        self.submit_edit();
        match self.edit.take().and_then(|edit| edit.error) {
            Some(error) => Err(Refused::Invalid(error)),
            None => Ok(()),
        }
    }

    pub(crate) fn apply(&mut self, key: &str, value: &Value) -> Result<Option<String>, Refused> {
        let Some((setting, target)) = self.entries().into_iter().find(|(s, _)| s.key == key) else {
            return Err(match self.connection_hint(key) {
                Some(hint) => Refused::Invalid(hint),
                None => Refused::Unknown(format!("there is no setting `{key}`; `cornercase config` lists them")),
            });
        };
        let typed = if setting.kind == Kind::List { String::new() } else { Self::typed(&setting, value)? };
        if *value == setting.value {
            return Ok(None);
        }
        match (&target, setting.kind) {
            (Target::Tabs, _) => self.apply_tabs(value)?,
            (_, Kind::Key) => self.apply_prefix(&typed)?,
            (Target::Row(row @ Row::Forum(_)), _) => {
                self.go_to(row);
                self.flip_forum();
            }
            (Target::Row(row), kind) => {
                self.go_to(row);
                self.activate();
                match kind {
                    Kind::Switch => {}
                    Kind::Choice => self.pick_value(&typed)?,
                    _ => self.submit_text(typed)?,
                }
            }
            (Target::Mode(kind) | Target::Extra(kind), _) => self.apply_agent(kind, &target, typed)?,
        }
        Ok(self.notice.take())
    }

    fn apply_agent(&mut self, kind: &str, target: &Target, typed: String) -> Result<(), Refused> {
        if !self.listed_kinds().iter().any(|listed| listed == kind) {
            self.added.push(kind.to_string());
        }
        self.go_to(&Row::Kind(kind.to_string()));
        if matches!(target, Target::Mode(_)) {
            self.activate();
            self.pick_value(&typed)
        } else {
            self.edit_extra(kind);
            self.submit_text(typed)
        }
    }

    fn pick_value(&mut self, value: &str) -> Result<(), Refused> {
        let at = self.pick_choices().iter().position(|item| item.value == value);
        let Some(at) = at else { return Err(Refused::Invalid(format!("`{value}` is not offered"))) };
        self.choose(Some(at));
        Ok(())
    }

    fn apply_tabs(&mut self, value: &Value) -> Result<(), Refused> {
        let wrong = || Refused::Invalid("`issue_tabs` takes a list of sources".into());
        let names: Vec<&str> = value
            .as_array()
            .ok_or_else(wrong)?
            .iter()
            .map(|v| v.as_str().ok_or_else(wrong))
            .collect::<Result<_, _>>()?;
        if names.is_empty() {
            return Err(Refused::Invalid("at least one tab stays".into()));
        }
        let mut ids: Vec<&'static str> = Vec::new();
        for name in names {
            let Some(id) = TAB_IDS.iter().find(|id| **id == name) else {
                return Err(Refused::Invalid(format!("`{name}` is not a source; use: {}", TAB_IDS.join(", "))));
            };
            if ids.contains(id) {
                return Err(Refused::Invalid(format!("`{name}` is listed twice")));
            }
            ids.push(id);
        }
        self.go_to(&Row::Tab(ids[0]));
        self.save_tabs(&ids);
        Ok(())
    }

    fn apply_prefix(&mut self, text: &str) -> Result<(), Refused> {
        let prefix = if text.trim().is_empty() { None } else { Some(Prefix::read(text).map_err(Refused::Invalid)?) };
        self.go_to(&Row::Prefix);
        self.save_prefix(prefix);
        Ok(())
    }

    fn row_of_connection(&self, key: &str) -> Option<Row> {
        let source = self.tokens.iter().map(|(s, _)| *s).find(|s| s.id() == key).map(Row::Token);
        let bot = Via::ALL.into_iter().find(|v| v.target() == key).map(Row::Bot);
        let chat = Via::ALL.into_iter().find(|v| format!("{}.chat", v.target()) == key).map(Row::Chat);
        source.or(bot).or(chat)
    }

    pub(crate) fn secret(&mut self, key: &str, token: &Secret) -> Result<Action, Refused> {
        let row = match self.row_of_connection(key) {
            Some(row @ (Row::Token(_) | Row::Bot(_))) => row,
            _ if key == "browsers" => return Err(Refused::Invalid(format!("`{key}` takes no secret"))),
            Some(_) => return Err(Refused::Invalid(format!("`{key}` takes no secret"))),
            None => {
                return Err(Refused::Unknown(format!(
                    "there is no connection `{key}`; `cornercase config` lists them"
                )));
            }
        };
        let name = match &row {
            Row::Token(source) => source.token_name().to_string(),
            _ => "bot's token".to_string(),
        };
        let token = token.0.trim();
        if token.is_empty() {
            return Err(Refused::Invalid(format!("paste the {name} first")));
        }
        self.go_to(&row);
        self.activate();
        if self.edit.is_none() {
            return Err(Refused::Blocked(self.notice.take().unwrap_or_default()));
        }
        if let Some(edit) = &mut self.edit {
            edit.input = token.to_string();
        }
        let action = self.submit_edit();
        match self.edit.take().and_then(|edit| edit.error) {
            Some(error) => Err(Refused::Blocked(error)),
            None => Ok(action),
        }
    }

    pub(crate) fn removal_of(&self, key: &str) -> Result<Option<Action>, Refused> {
        if key == "browsers" {
            return Ok((self.browsers > 0).then_some(Action::ForgetBrowsers));
        }
        let Some(row) = self.row_of_connection(key) else {
            return Err(Refused::Unknown(format!("there is no connection `{key}`; `cornercase config` lists them")));
        };
        if let Row::Token(source) = &row
            && self.status(*source) == Some(&Status::Env)
        {
            let env = source.token_env().unwrap_or_default();
            return Err(Refused::Blocked(format!("{env} is set: change or unset it there")));
        }
        Ok(self.removal_of_row(&row))
    }
}
