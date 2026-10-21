use ratatui::layout::Position;

use super::App;
use super::account_restart::Restart;
use super::menus::MenuAction;
use super::overlay::Overlay;
use super::toast::Toast;
use crate::agents::Known;
use crate::project::Hierarchy;
use crate::term::Term;
use crate::{settings, ui};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AccountItem {
    pub(super) pane: u64,
    pub(super) generation: u32,
    pub(super) kind: Known,
    pub(super) index: u16,
}

impl App {
    pub(super) fn account_rows(&self) -> Vec<ui::AccountRow> {
        self.tab().map(|tab| tab.panes.iter().map(|term| self.pane_pills(term)).collect()).unwrap_or_default()
    }

    pub(super) fn pane_pills(&self, term: &Term) -> ui::AccountRow {
        let agent = self.row_agent(term);
        let pills = self
            .row_kinds(term)
            .into_iter()
            .map(|kind| ui::AccountPill { agent: kind.name().to_string(), account: self.pill_label(term, kind) })
            .collect();
        ui::AccountRow { pills, blocked: agent.is_none() && !term.shell_in_foreground() }
    }

    fn row_agent(&self, term: &Term) -> Option<Known> {
        term.agent_account().map(|(kind, _)| kind).filter(|kind| self.switchable().contains(kind))
    }

    fn row_kinds(&self, term: &Term) -> Vec<Known> {
        self.row_agent(term).map_or_else(|| self.switchable(), |kind| vec![kind])
    }

    fn pill_label(&self, term: &Term, kind: Known) -> String {
        match term.agent_account().filter(|(k, _)| *k == kind) {
            Some((_, name)) => self.account_label(kind, name).unwrap_or_default(),
            None => self.config.account_label(kind.name(), term.accounts().name(kind).unwrap_or_default()).to_string(),
        }
    }

    fn account_in(term: &Term, kind: Known) -> &str {
        match term.agent_account().filter(|(k, _)| *k == kind) {
            Some((_, name)) => name.unwrap_or_default(),
            None => term.accounts().name(kind).unwrap_or_default(),
        }
    }

    fn switchable(&self) -> Vec<Known> {
        self.listed_accounts().iter().filter(|(_, names)| !names.is_empty()).map(|(kind, _)| *kind).collect()
    }

    fn account_names(&self, kind: Known) -> &[String] {
        self.listed_accounts().iter().find(|(k, _)| *k == kind).map_or(&[], |(_, names)| names.as_slice())
    }

    pub(super) fn chosen(&self, kind: Known) -> &str {
        let chosen = self.config.selected_accounts().get(kind.name()).map_or("", String::as_str);
        if self.account_names(kind).iter().any(|name| name == chosen) { chosen } else { "" }
    }

    fn account_revision(&self) -> (u64, u64) {
        (self.config_file.revision(), self.accounts.logins.generation())
    }

    pub(super) fn open_pill_menu(&mut self, pane: u64, index: usize, at: Position) {
        let Some(kind) = self.projects.pane_by(pane).and_then(|term| self.row_kinds(term).get(index).copied()) else {
            return;
        };
        self.open_account_menu(pane, kind, at);
        if let Some(Overlay::Menu { at, actions }) = &mut self.overlay {
            let rows = u16::try_from(actions.len()).unwrap_or(u16::MAX).saturating_add(3);
            at.y = at.y.saturating_sub(rows);
        }
    }

    pub(super) fn account_entry(&self, pane: Option<u64>) -> Option<MenuAction> {
        pane.filter(|_| self.has_accounts()).map(MenuAction::AccountMenu)
    }

    pub(super) fn tab_pane(&self, tab: u64) -> Option<u64> {
        let at = self.projects.tab_index(tab)?;
        self.projects.tab(at).pane().map(|term| term.id)
    }

    pub(super) fn open_account_submenu(&mut self, pane: u64, at: Position) {
        let Some(term) = self.projects.pane_by(pane) else { return };
        let generation = term.generation();
        let kinds = self.row_kinds(term);
        match kinds.as_slice() {
            [] => {}
            [kind] => self.open_account_menu(pane, *kind, at),
            _ => {
                let item = |kind| MenuAction::AccountsOf(AccountItem { pane, generation, kind, index: 0 });
                self.overlay = Some(Overlay::Menu { at, actions: kinds.into_iter().map(item).collect() });
            }
        }
    }

    pub(super) fn open_account_menu(&mut self, pane: u64, kind: Known, at: Position) {
        let Some(generation) = self.projects.pane_by(pane).map(Term::generation) else { return };
        let (revision, count) = (self.account_revision(), self.account_names(kind).len());
        let item = |index| AccountItem { pane, generation, kind, index };
        let note = self.account_blocker(pane, kind).map(|_| MenuAction::AccountNote(item(0)));
        let actions = (0..=count)
            .filter_map(|i| u16::try_from(i).ok())
            .map(|i| MenuAction::Account(item(i), revision))
            .chain(note)
            .chain(std::iter::once(MenuAction::ManageAccounts))
            .collect::<Vec<_>>();
        self.overlay = Some(Overlay::Menu { at, actions });
    }

    fn account_blocker(&self, pane: u64, kind: Known) -> Option<String> {
        let Some(term) = self.projects.pane_by(pane) else { return Some(format!("pane {pane} is gone")) };
        match term.agent_account().filter(|(k, _)| *k == kind) {
            Some((_, name)) => self.not_restartable(pane, name),
            None => self.shell_blocker(pane, kind),
        }
    }

    pub(super) fn menu_blocker(&self, actions: &[MenuAction]) -> Option<String> {
        let item = actions.iter().find_map(|action| match action {
            MenuAction::Account(item, _) | MenuAction::AccountNote(item) => Some(*item),
            _ => None,
        });
        item.and_then(|item| self.account_blocker(item.pane, item.kind))
    }

    pub(super) fn account_disabled(action: MenuAction, blocked: bool) -> bool {
        match action {
            MenuAction::AccountNote(_) => true,
            MenuAction::Account(..) => blocked,
            _ => false,
        }
    }

    pub(super) fn open_accounts_of(&mut self, item: AccountItem, at: Position) {
        if self.projects.pane_by(item.pane).is_some_and(|term| term.generation() == item.generation) {
            self.open_account_menu(item.pane, item.kind, at);
        }
    }

    fn account_at(&self, kind: Known, index: u16) -> Option<String> {
        match usize::from(index).checked_sub(1) {
            None => Some(String::new()),
            Some(i) => self.account_names(kind).get(i).cloned(),
        }
    }

    pub(super) fn account_menu_label(&self, action: MenuAction, blocker: Option<&str>) -> Option<String> {
        Some(match action {
            MenuAction::Account(item, _) => self.account_item_label(item),
            MenuAction::AccountNote(_) => blocker.unwrap_or("it can change now").to_string(),
            MenuAction::AccountsOf(AccountItem { pane, kind, .. }) => {
                let account = self.projects.pane_by(pane).map(|term| self.pill_label(term, kind)).unwrap_or_default();
                format!("{}: {account} ›", kind.name())
            }
            MenuAction::AccountMenu(_) => "account ›".into(),
            MenuAction::ManageAccounts => "manage accounts…".into(),
            _ => return None,
        })
    }

    fn account_item_label(&self, AccountItem { pane, kind, index, .. }: AccountItem) -> String {
        let account = self.account_at(kind, index).unwrap_or_default();
        let current = self.projects.pane_by(pane).map(|term| Self::account_in(term, kind));
        let mark = if current == Some(account.as_str()) { "✓" } else { " " };
        format!("{mark} {}", self.config.account_label(kind.name(), &account))
    }

    pub(super) fn account_item(&mut self, item: AccountItem, revision: (u64, u64)) {
        if revision != self.account_revision() || self.account_blocker(item.pane, item.kind).is_some() {
            return;
        }
        let Some(term) = self.projects.pane_by(item.pane).filter(|term| term.generation() == item.generation) else {
            return;
        };
        let Some(to) = self.account_at(item.kind, item.index) else { return };
        let runs = term.agent_account().is_some_and(|(kind, _)| kind == item.kind);
        let current = Self::account_in(term, item.kind) == to;
        let toast = match (runs, current) {
            (true, true) => self.keep_account(item.pane, item.kind, &to),
            (true, false) => Some(self.restart_from_row(item.pane, item.kind, &to)),
            (false, true) => None,
            (false, false) => Some(self.respawn_from_row(item.pane, item.kind, &to)),
        };
        if let Some(toast) = toast {
            self.messages.toast = Some(toast);
        }
    }

    fn keep_account(&mut self, pane: u64, kind: Known, to: &str) -> Option<Toast> {
        let moving = self.cancel_move(pane);
        let waiting = self.restart_waiting(pane).is_some();
        self.cancel_restart(pane);
        let label = self.config.account_label(kind.name(), to);
        (moving || waiting).then(|| Toast::new(format!("{} stays on {label}", kind.name()), ui::ToastIcon::Check))
    }

    fn restart_from_row(&mut self, pane: u64, kind: Known, to: &str) -> Toast {
        let label = self.config.account_label(kind.name(), to).to_string();
        match self.ask_restart_for(pane, kind, to, None) {
            Ok(Restart::Now) => Toast::new(format!("restarting {} on {label}", kind.name()), ui::ToastIcon::Check),
            Ok(Restart::Waiting) => {
                Toast::new(format!("{} restarts on {label} after this turn", kind.name()), ui::ToastIcon::Check)
            }
            Err(why) => Toast::new(why, ui::ToastIcon::Bug),
        }
    }

    fn respawn_from_row(&mut self, pane: u64, kind: Known, to: &str) -> Toast {
        let label = self.config.account_label(kind.name(), to).to_string();
        match self.respawn_shell(pane, kind, to) {
            Ok(()) => Toast::new(format!("this pane now uses {label} for {}", kind.name()), ui::ToastIcon::Check),
            Err(why) => Toast::new(why, ui::ToastIcon::Bug),
        }
    }

    pub(super) fn manage_accounts(&mut self) {
        let kind = self.switchable().first().copied().unwrap_or(Known::Claude);
        self.open_settings_on(&settings::Row::System(kind));
    }
}
