use std::time::{Duration, Instant};

use ratatui::layout::Rect;

use super::App;
use super::control::{Asker, Handled};
use super::dialogs::confirm_view;
use super::overlay::{Overlay, REMOVE_SUBMIT};
use super::toast::Toast;
use crate::agents::Known;
use crate::control::AccountRemove;
use crate::settings::{Listed, Order, Page};
use crate::ui;

pub(super) const CHECK_EVERY: Duration = Duration::from_secs(30);

impl App {
    pub(super) fn accounts_listed(&self) -> Listed {
        Listed { info: self.accounts_info(None), checking: self.checking_logins() }
    }

    pub(super) fn refresh_accounts_page(&mut self) {
        if !matches!(&self.overlay, Some(Overlay::Settings(s)) if s.page == Page::Accounts) {
            return;
        }
        let listed = self.accounts_listed();
        if let Some(Overlay::Settings(s)) = &mut self.overlay {
            s.accounts = listed;
        }
    }

    pub(super) fn watch_accounts_page(&mut self, now: Instant) {
        if !matches!(&self.overlay, Some(Overlay::Settings(s)) if s.page == Page::Accounts) {
            self.accounts.logins.page_closed();
            return;
        }
        if let Some(first) = self.accounts.logins.check_page(now, CHECK_EVERY) {
            if first {
                self.relist_accounts();
            }
            self.check_logins();
        }
        self.refresh_accounts_page();
    }

    pub(super) fn account_order(&mut self, order: &Order, area: Rect) {
        let (result, done) = match order {
            Order::Add(add) => (
                self.account_add_request(Asker::default(), add, Some(area), Instant::now()),
                Some(format!("log in to {} in the tab {} login", add.provider, add.name)),
            ),
            Order::Rename(rename) => {
                let was = self.config.account_label(&rename.provider, &rename.account).to_string();
                let result = self.account_rename_request(rename);
                let now = self.config.account_label(&rename.provider, &rename.account);
                (result, Some(format!("{}: {was} is now called {now}", rename.provider)))
            }
            Order::Use(wanted) => (self.account_use_request(wanted), None),
        };
        if self.account_answered(result, done.clone())
            && let (Order::Add(_), Some(done)) = (order, done)
        {
            self.overlay = None;
            self.messages.toast = Some(Toast::new(done, ui::ToastIcon::Check));
        }
    }

    pub(super) fn account_answered(&mut self, result: Handled, done: Option<String>) -> bool {
        if let Err(refusal) = result {
            match &mut self.overlay {
                Some(Overlay::Settings(s)) => s.notice = Some(refusal.message),
                _ => self.messages.toast = Some(Toast::new(refusal.message, ui::ToastIcon::Bug)),
            }
            return false;
        }
        self.settings_notice(done);
        true
    }

    fn settings_notice(&mut self, notice: Option<String>) {
        self.refresh_accounts_page();
        match &mut self.overlay {
            Some(Overlay::Settings(s)) => s.notice = notice,
            _ => {
                if let Some(notice) = notice {
                    self.messages.toast = Some(Toast::new(notice, ui::ToastIcon::Check));
                }
            }
        }
    }

    pub(super) fn remove_account_view(kind: Known, name: &str) -> ui::Overlay {
        let message = format!(
            "Remove the {} account {name}? Its folder, history and name are deleted, and new tabs that used it \
             use the system login. A login kept in the system keychain stays: log out of {} in its tab first.",
            kind.name(),
            kind.title()
        );
        confirm_view("Remove account", message, REMOVE_SUBMIT, None)
    }

    pub(super) fn submit_remove_account(&mut self, kind: Known, name: &str) -> Overlay {
        let remove = AccountRemove { provider: kind.name().to_string(), name: name.to_string() };
        let notice = match self.account_remove_request(&remove) {
            Ok(_) => format!("the {} account {name} was removed", kind.name()),
            Err(refusal) => refusal.message,
        };
        let mut settings = self.settings_model(Page::Accounts);
        settings.notice = Some(notice);
        Overlay::Settings(Box::new(settings))
    }
}
