use crate::config::Config;
use crate::issues::{Account, Source};
use crate::messenger::Via;

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

    pub(super) fn save_tabs(&mut self, shown: &[&'static str]) -> Action {
        let names: Vec<String> = shown.iter().map(|t| (*t).to_string()).collect();
        let notice = format!("tabs: {}", names.join(", "));
        self.save(Config { issue_tabs: names, ..self.config.clone() }, notice)
    }

    pub(crate) fn removal(&self, row: usize) -> Option<Action> {
        self.rows().get(row).and_then(|row| self.removal_of_row(row))
    }

    pub(crate) fn removal_of_row(&self, row: &Row) -> Option<Action> {
        match Some(row) {
            Some(Row::Token(source)) if matches!(self.status(*source), Some(Status::Saved(_))) => {
                Some(Action::RemoveToken(*source))
            }
            Some(Row::Bot(via)) if self.messenger(*via).bot.is_some() => Some(Action::RemoveBot(*via)),
            Some(Row::Chat(via)) if self.messenger(*via).chat.is_some() => Some(Action::ForgetChat(*via)),
            Some(row) => Self::account_removal(row),
            None => None,
        }
    }

    pub(crate) fn bot_checked(&mut self, via: Via, result: Result<String, String>, paired: bool) {
        self.messenger_mut(via).checking = false;
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
