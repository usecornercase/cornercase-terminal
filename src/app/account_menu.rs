use ratatui::layout::Position;

use super::App;
use super::menus::MenuAction;
use super::overlay::Overlay;
use crate::agents::Known;
use crate::control::AccountUse;
use crate::{settings, ui};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AccountItem {
    pub(super) kind: Known,
    pub(super) index: u16,
}

impl App {
    pub(super) fn account_pills(&self) -> Vec<ui::AccountPill> {
        if !self.config.account_switcher {
            return Vec::new();
        }
        self.switchable()
            .into_iter()
            .map(|kind| ui::AccountPill { agent: kind.name().to_string(), account: self.chosen_label(kind) })
            .collect()
    }

    fn switchable(&self) -> Vec<Known> {
        self.listed_accounts().iter().filter(|(_, names)| !names.is_empty()).map(|(kind, _)| *kind).collect()
    }

    fn account_names(&self, kind: Known) -> &[String] {
        self.listed_accounts().iter().find(|(k, _)| *k == kind).map_or(&[], |(_, names)| names.as_slice())
    }

    fn chosen(&self, kind: Known) -> &str {
        let chosen = self.config.selected_accounts().get(kind.name()).map_or("", String::as_str);
        if self.account_names(kind).iter().any(|name| name == chosen) { chosen } else { "" }
    }

    fn chosen_label(&self, kind: Known) -> String {
        self.config.account_label(kind.name(), self.chosen(kind)).to_string()
    }

    fn account_revision(&self) -> (u64, u64) {
        (self.config_file.revision, self.logins.generation)
    }

    pub(super) fn open_pill_menu(&mut self, index: usize, at: Position) {
        let Some(kind) = self.switchable().get(index).copied() else { return };
        let revision = self.account_revision();
        let count = self.account_names(kind).len();
        let actions = (0..=count)
            .filter_map(|i| u16::try_from(i).ok())
            .map(|i| MenuAction::Account(AccountItem { kind, index: i }, revision))
            .chain(std::iter::once(MenuAction::ManageAccounts))
            .collect::<Vec<_>>();
        let rows = u16::try_from(actions.len()).unwrap_or(u16::MAX).saturating_add(3);
        let at = Position::new(at.x, at.y.saturating_sub(rows));
        self.overlay = Some(Overlay::Menu { at, actions });
    }

    fn account_at(&self, kind: Known, index: u16) -> Option<String> {
        match usize::from(index).checked_sub(1) {
            None => Some(String::new()),
            Some(i) => self.account_names(kind).get(i).cloned(),
        }
    }

    pub(super) fn account_item_label(&self, AccountItem { kind, index }: AccountItem) -> String {
        let account = self.account_at(kind, index).unwrap_or_default();
        let mark = if account == self.chosen(kind) { "✓" } else { " " };
        format!("{mark} {}", self.config.account_label(kind.name(), &account))
    }

    pub(super) fn account_item(&mut self, AccountItem { kind, index }: AccountItem, revision: (u64, u64)) {
        if revision != self.account_revision() {
            return;
        }
        let Some(account) = self.account_at(kind, index) else { return };
        let label = self.config.account_label(kind.name(), &account).to_string();
        let done = format!("new {} tabs use {label} · open tabs keep theirs", kind.name());
        let result = self.account_use_request(&AccountUse { provider: kind.name().to_string(), account });
        self.account_answered(result, done);
    }

    pub(super) fn manage_accounts(&mut self) {
        let kind = self.switchable().first().copied().unwrap_or(Known::Claude);
        self.open_settings_on(&settings::Row::System(kind));
    }
}
