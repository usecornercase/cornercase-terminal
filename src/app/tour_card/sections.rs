use ratatui::layout::{Position, Rect};

use crate::app::App;
use crate::app::layout::Side;
use crate::app::menus::MenuAction;
use crate::app::overlay::Overlay;
use crate::issues::browser::Tab;
use crate::log;
use crate::project::search::Finder;
use crate::settings::Page;
use crate::tour::{self, Panel, Show, Target};

impl App {
    pub(super) fn show_section(&mut self) {
        let Some(active) = &self.messages.tour.active else { return };
        let want = active.step().show.filter(|_| active.card);
        if active.opened.is_some_and(|opened| Some(opened) != want) {
            self.hide_section();
        }
        let Some(want) = want.filter(|&want| !self.section_shown(want)) else { return };
        if self.open_section(want)
            && self.section_shown(want)
            && let Some(active) = &mut self.messages.tour.active
        {
            active.opened = Some(want);
        }
    }

    pub(super) fn hide_section(&mut self) {
        let Some(opened) = self.messages.tour.active.as_mut().and_then(|a| a.opened.take()) else { return };
        if !self.section_shown(opened) {
            return;
        }
        match opened {
            Show::Panel { .. } => self.close_panel(),
            _ if self.overlay.as_ref().is_some_and(Overlay::busy) => {}
            _ => self.overlay = None,
        }
    }

    pub(super) fn tour_panel(&self) -> Option<Panel> {
        match self.screen.side.filter(|_| self.panel_shown())? {
            Side::Changes => Some(Panel::Changes),
            Side::Files => Some(Panel::Files),
            Side::Todo => Some(Panel::Todo),
            Side::Flow => Some(Panel::Flow),
        }
    }

    fn section_shown(&self, show: Show) -> bool {
        match (show, &self.overlay) {
            (Show::Panel { panel }, _) => self.tour_panel() == Some(panel),
            (Show::Issues, Some(Overlay::Issues(_)))
            | (Show::Search, Some(Overlay::Search(_)))
            | (Show::Keys, Some(Overlay::Keys(_))) => true,
            (Show::Queue, Some(Overlay::Issues(b))) => b.current() == Tab::Queue,
            (Show::Settings { page: tour::Page::Flows }, Some(Overlay::Flows { dialog, .. })) => dialog.in_settings,
            (Show::Settings { page }, Some(Overlay::Settings(s))) => s.page == settings_page(page),
            (Show::HelpMenu, Some(Overlay::Menu { actions, .. })) => {
                matches!(actions.first(), Some(MenuAction::Help(_)))
            }
            (Show::RemoteMenu, Some(Overlay::Menu { actions, .. })) => {
                matches!(actions.first(), Some(MenuAction::Browser))
            }
            _ => false,
        }
    }

    fn open_section(&mut self, show: Show) -> bool {
        if let Show::Panel { panel } = show {
            return self.open_panel(panel);
        }
        let Some(area) = self.panels.queued.area().filter(|_| self.overlay.is_none()) else { return false };
        match show {
            Show::Issues | Show::Queue => self.open_issues_on(area, show == Show::Queue),
            Show::Search => self.overlay = Some(Overlay::Search(Finder::default())),
            Show::Keys => self.overlay = Some(Overlay::Keys(None)),
            Show::Settings { page } => {
                self.settings_page = settings_page(page);
                self.open_settings();
            }
            Show::HelpMenu | Show::RemoteMenu | Show::Panel { .. } => return self.open_menu(area, show),
        }
        true
    }

    fn open_panel(&mut self, panel: Panel) -> bool {
        if self.screen.side.is_some() {
            return false;
        }
        self.show_side(Some(match panel {
            Panel::Changes => Side::Changes,
            Panel::Files => Side::Files,
            Panel::Todo => Side::Todo,
            Panel::Flow => Side::Flow,
        }));
        if !self.panel_shown() {
            self.show_side(None);
        }
        true
    }

    fn open_issues_on(&mut self, area: Rect, queue: bool) {
        if let Err(e) = self.open_issues(area) {
            log::warning!("tour", "cannot open issues", error = e);
        }
        if queue && let Some(Overlay::Issues(b)) = &mut self.overlay {
            b.tab_to(Tab::Queue);
        }
    }

    fn open_menu(&mut self, area: Rect, show: Show) -> bool {
        let help = show == Show::HelpMenu;
        let target = if help { Target::Help } else { Target::Remote };
        let (at, _) = self.tour_target(&self.layout(area).shown(self.screen.nav), target);
        if at.is_empty() {
            return false;
        }
        let at = Position::new(at.x, at.y);
        if help {
            self.open_help(at);
        } else {
            self.open_remote_menu(at);
        }
        true
    }
}

const fn settings_page(page: tour::Page) -> Page {
    match page {
        tour::Page::Agents => Page::Agents,
        tour::Page::Accounts => Page::Accounts,
        tour::Page::Flows => Page::Flows,
        tour::Page::Ui => Page::Ui,
        tour::Page::Remote => Page::Remote,
        tour::Page::Plugins => Page::Plugins,
    }
}
