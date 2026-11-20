use ratatui::layout::Position;

use super::acting::Aim;
use super::acting::dialog::{Dialog, Kind};
use super::rows::status;
use crate::app::App;
use crate::app::menus::{CLOSE_TAB_LABEL, MenuAction, RENAME_TAB_LABEL};
use crate::editor::Editor;
use crate::machines::act::{Act, Scope};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) struct TabAim {
    pub(in crate::app) aim: Aim,
    pub(in crate::app) tab: u64,
}

impl App {
    pub(in crate::app) fn remote_new_tab(&mut self, aim: Aim) {
        self.send_act(aim, Act::NewTab);
    }

    pub(in crate::app) fn remote_close_tab(&mut self, at: TabAim, compact: bool) {
        if let Err(why) = self.aimed(at.aim, Scope::Tab(at.tab)) {
            return self.not_sent(&why);
        }
        let agent = self.remote_tab_info(at.aim, at.tab).is_some_and(|tab| status(tab.status.as_deref()).is_some());
        if compact || agent {
            self.overlay = Some(Dialog::open(at.aim, Kind::CloseTab { tab: at.tab }));
        } else {
            self.send_act(at.aim, Act::CloseTab(at.tab));
        }
    }

    pub(in crate::app) fn remote_rename_tab(&mut self, at: TabAim) {
        if let Err(why) = self.aimed(at.aim, Scope::Tab(at.tab)) {
            return self.not_sent(&why);
        }
        let Some(tab) = self.remote_tab_info(at.aim, at.tab) else { return };
        let input = Editor::new(tab.custom_name.as_ref().unwrap_or(&tab.name));
        self.overlay = Some(Dialog::open(at.aim, Kind::RenameTab { tab: at.tab, input }));
    }

    pub(in crate::app) fn remote_tab_menu(&mut self, at: TabAim, pos: Position) {
        self.noted_menu(at.aim.link, vec![MenuAction::RemoteTabRename(at), MenuAction::RemoteTabClose(at)], pos);
    }

    pub(super) fn remote_tab_disabled(&self, action: MenuAction) -> bool {
        let (MenuAction::RemoteTabRename(at) | MenuAction::RemoteTabClose(at)) = action else { return false };
        let acting = &self.machines.acting;
        !self.linked(at.aim.link)
            || acting.busy_tab(at.aim.link, at.tab)
            || acting.removing(at.aim.link, at.aim.workspace)
    }

    pub(super) fn remote_tab_label(action: MenuAction) -> Option<String> {
        match action {
            MenuAction::RemoteTabRename(_) => Some(RENAME_TAB_LABEL.into()),
            MenuAction::RemoteTabClose(_) => Some(CLOSE_TAB_LABEL.into()),
            _ => None,
        }
    }

    pub(super) fn remote_tab_action(&mut self, action: MenuAction) -> Option<MenuAction> {
        let disabled = self.remote_tab_disabled(action);
        match action {
            MenuAction::RemoteTabRename(at) if !disabled => self.remote_rename_tab(at),
            MenuAction::RemoteTabClose(at) if !disabled => self.remote_close_tab(at, false),
            MenuAction::RemoteTabRename(_) | MenuAction::RemoteTabClose(_) => {}
            other => return Some(other),
        }
        None
    }
}
