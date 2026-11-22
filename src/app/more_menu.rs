use ratatui::layout::Position;

use super::App;
use super::menus::MenuAction;
use super::overlay::Overlay;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MoreItem {
    Update,
    Settings,
    Usage,
    Processes,
    Ports,
    NewBot,
    Queue,
    Remote,
    Quit,
}

impl App {
    pub(super) fn open_more(&mut self, at: Position) {
        let update = self.update_label().map(|_| MoreItem::Update);
        let remote = self.remote_button().map(|_| MoreItem::Remote);
        let actions = update
            .into_iter()
            .chain([
                MoreItem::NewBot,
                MoreItem::Queue,
                MoreItem::Settings,
                MoreItem::Usage,
                MoreItem::Processes,
                MoreItem::Ports,
            ])
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
            MoreItem::NewBot => "new bot…".into(),
            MoreItem::Queue => "queue".into(),
            MoreItem::Settings => "settings".into(),
            MoreItem::Usage => "usage".into(),
            MoreItem::Processes => "processes".into(),
            MoreItem::Ports => "ports".into(),
            MoreItem::Remote => "remote…".into(),
            MoreItem::Quit => "quit".into(),
        })
    }

    pub(super) fn more_action(&mut self, item: MoreItem, at: Position) {
        match item {
            MoreItem::Update => self.open_update(),
            MoreItem::NewBot => self.open_bot_form(None),
            MoreItem::Queue => self.open_queue(None),
            MoreItem::Settings => self.open_settings(),
            MoreItem::Usage => self.open_usage(),
            MoreItem::Processes => self.open_processes(),
            MoreItem::Ports => self.open_shown_ports(),
            MoreItem::Remote => self.open_remote_menu(at),
            MoreItem::Quit => self.outbox.detach(),
        }
    }
}
