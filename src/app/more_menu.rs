use ratatui::layout::Position;

use super::App;
use super::menus::MenuAction;
use super::overlay::Overlay;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MoreItem {
    Update,
    Settings,
    Usage,
    Remote,
    Quit,
}

impl App {
    pub(super) fn open_more(&mut self, at: Position) {
        let update = self.update_label().map(|_| MoreItem::Update);
        let remote = self.remote_button().map(|_| MoreItem::Remote);
        let actions = update
            .into_iter()
            .chain([MoreItem::Settings, MoreItem::Usage])
            .chain(remote)
            .chain([MoreItem::Quit])
            .map(MenuAction::More)
            .collect();
        self.overlay = Some(Overlay::Menu { at, actions });
    }

    pub(super) fn more_menu_label(&self, action: MenuAction) -> Option<String> {
        let MenuAction::More(item) = action else { return None };
        Some(match item {
            MoreItem::Update => self.update_entry().unwrap_or_default(),
            MoreItem::Settings => "settings".into(),
            MoreItem::Usage => "usage".into(),
            MoreItem::Remote => "remote…".into(),
            MoreItem::Quit => "quit".into(),
        })
    }

    pub(super) fn more_action(&mut self, item: MoreItem, at: Position) {
        match item {
            MoreItem::Update => self.open_update(),
            MoreItem::Settings => self.open_settings(),
            MoreItem::Usage => self.open_usage(),
            MoreItem::Remote => self.open_remote_menu(at),
            MoreItem::Quit => self.outbox.detach(),
        }
    }
}
