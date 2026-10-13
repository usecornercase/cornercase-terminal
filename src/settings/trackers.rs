use crate::config::Config;
use crate::issues::{Account, Source};

use super::{Action, Move, Row, Settings, Status};

impl Settings {
    pub(crate) fn checked(&mut self, source: Source, result: Result<Account, String>) {
        self.checking.retain(|s| *s != source);
        match result {
            Ok(account) => {
                self.edit = None;
                self.notice = Some(format!("{} connected as {}", source.name(), account.describe()));
                self.set_status(source, Status::Saved(Some(account)));
            }
            Err(e) => {
                if let Some(edit) = &mut self.edit {
                    edit.error = Some(e);
                }
            }
        }
    }

    pub(crate) fn removed(&mut self, source: Source) {
        self.set_status(source, Status::Missing);
        self.notice = Some(format!("the {} {} was removed", source.name(), source.token_name()));
    }

    fn set_status(&mut self, source: Source, status: Status) {
        if let Some((_, s)) = self.tokens.iter_mut().find(|(s, _)| *s == source) {
            *s = status;
        }
    }

    pub(super) fn toggle_tab(&mut self, id: &'static str) -> Action {
        let mut shown = self.shown_tabs();
        if shown.contains(&id) {
            if shown.len() == 1 {
                self.notice = Some("at least one tab stays".into());
                return Action::None;
            }
            shown.retain(|t| *t != id);
        } else {
            shown.push(id);
        }
        self.save_tabs(&shown)
    }

    pub(crate) fn move_tab(&mut self, row: usize, to: Move) -> Action {
        let Some(Row::Tab(id)) = self.rows().get(row).cloned() else { return Action::None };
        let mut shown = self.shown_tabs();
        let Some(from) = shown.iter().position(|t| *t == id) else { return Action::None };
        let target = match to {
            Move::Up => from.checked_sub(1),
            Move::Down => Some(from + 1).filter(|t| *t < shown.len()),
        };
        let Some(target) = target else { return Action::None };
        shown.swap(from, target);
        let action = self.save_tabs(&shown);
        if let Some(i) = self.rows().iter().position(|r| *r == Row::Tab(id)) {
            self.cursor = i;
        }
        action
    }

    fn save_tabs(&mut self, shown: &[&'static str]) -> Action {
        let names: Vec<String> = shown.iter().map(|t| (*t).to_string()).collect();
        let notice = format!("tabs: {}", names.join(", "));
        self.save(Config { issue_tabs: names, ..self.config.clone() }, notice)
    }

    pub(crate) fn removal(&self, row: usize) -> Option<Action> {
        match self.rows().get(row) {
            Some(Row::Token(source)) if matches!(self.status(*source), Some(Status::Saved(_))) => {
                Some(Action::RemoveToken(*source))
            }
            Some(Row::TelegramBot) if self.telegram.bot.is_some() => Some(Action::RemoveTelegram),
            Some(Row::TelegramChat) if self.telegram.chat.is_some() => Some(Action::ForgetTelegram),
            _ => None,
        }
    }

    pub(crate) fn telegram_checked(&mut self, result: Result<String, String>, paired: bool) {
        self.telegram.checking = false;
        match result {
            Ok(name) => {
                self.edit = None;
                let next = if paired { "" } else { ": pair your phone in the row below" };
                self.notice = Some(format!("bot @{name} is ready{next}"));
            }
            Err(e) => {
                if let Some(edit) = &mut self.edit {
                    edit.error = Some(e);
                }
            }
        }
    }
}
