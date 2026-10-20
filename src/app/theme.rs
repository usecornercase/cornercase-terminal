use super::App;
use super::control::{Handled, json};
use crate::control::contract::{Code, Refusal};
use crate::control::theme::{Asked, Scheme, Theme};
use crate::host::theme::HostTheme;
use crate::log;
use crate::project::Hierarchy;

impl App {
    pub(super) fn terminal_theme(&self) -> &HostTheme {
        self.themes.client_theme.as_ref().unwrap_or(&self.themes.theme)
    }

    pub(super) fn paint_terminals(&mut self) -> usize {
        let theme = self.themes.client_theme.as_ref().unwrap_or(&self.themes.theme);
        let mut panes = 0;
        for term in self.projects.terms_mut() {
            term.set_theme(theme);
            panes += 1;
        }
        panes
    }

    pub(super) fn give_theme(&mut self, theme: Option<HostTheme>) {
        self.themes.client_theme = theme;
        let panes = self.paint_terminals();
        let info = self.theme_info();
        log::info!(
            "app",
            "terminals take new colours",
            from = info.from.name(),
            scheme = info.scheme.map_or("unknown", |scheme| match scheme {
                Scheme::Light => "light",
                Scheme::Dark => "dark",
            }),
            panes = panes
        );
    }

    pub(super) fn theme_request(&mut self, theme: &Theme) -> Handled {
        match theme.asked().map_err(|message| Refusal::new(Code::Malformed, message))? {
            Asked::Read => {}
            Asked::Set(colours) => self.give_theme(Some(colours.clone())),
            Asked::Clear => self.give_theme(None),
        }
        Ok(Some(json(&self.theme_info())))
    }
}
