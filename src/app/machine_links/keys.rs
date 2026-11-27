use ratatui::layout::Rect;

use super::acting::Aim;
use super::acting::dialog::{Dialog, Kind};
use super::stream::Watched;
use super::tabs::TabAim;
use crate::app::App;
use crate::machines::act::{Act, Scope};
use crate::machines::stream::orders::Order;
use crate::machines::stream::screen::RemoteScreen;
use crate::project::split::{self, Dir, Place};
use crate::shortcuts::Action;

const UNLISTED_PANE: &str = "has not listed that pane yet";

impl App {
    pub(in crate::app) fn machine_shortcut(&mut self, action: Action) -> bool {
        let Some(shown) = self.shown_remote() else { return false };
        let Some(aim) = self.aim_at(shown.link, shown.workspace) else { return false };
        match action {
            Action::NewTab => self.remote_new_tab(aim),
            Action::RenameTab => {
                if let Some(tab) = shown.tab {
                    self.remote_rename_tab(TabAim { aim, tab });
                }
            }
            Action::Split(dir) => self.split_there(aim, dir),
            Action::ClosePane => self.close_pane_there(aim),
            Action::Pane(side) => self.focus_there(side),
            Action::RenameWorkspace => self.remote_rename(aim),
            Action::CloseWorkspace => self.remote_close(aim),
            _ => return false,
        }
        self.screen.nav = None;
        true
    }

    pub(super) fn live_pane(&self) -> Option<(&RemoteScreen, u64)> {
        let (_, screen) = self.watched().filter(|watched| !watched.moving()).and_then(Watched::live)?;
        Some((screen, screen.panes.iter().find(|pane| pane.active)?.pane))
    }

    fn split_there(&mut self, aim: Aim, dir: Dir) {
        let Some((_, pane)) = self.live_pane() else { return self.nothing_sent() };
        self.send_act(aim, Act::Split { pane, down: dir == Dir::Down });
    }

    fn close_pane_there(&mut self, aim: Aim) {
        let Some((screen, pane)) = self.live_pane() else { return self.nothing_sent() };
        let (tab, framed) = (screen.tab, screen.panes.len());
        let listed =
            self.remote_tab_info(aim, tab).map(|info| (info.panes.len(), info.panes.iter().any(|p| p.id == pane)));
        let Some((panes, true)) = listed else {
            let machine = self.held_by(aim.link).map(|held| held.name.clone()).unwrap_or_default();
            return self.not_sent(&format!("{machine} {UNLISTED_PANE}"));
        };
        if panes.max(framed) == 1 {
            return self.remote_close_tab(TabAim { aim, tab }, true);
        }
        match self.aimed(aim, Scope::Pane(pane)) {
            Ok(_) => self.overlay = Some(Dialog::open(aim, Kind::ClosePane { tab, pane })),
            Err(why) => self.not_sent(&why),
        }
    }

    fn focus_there(&mut self, side: Place) {
        let Some((screen, pane)) = self.live_pane() else { return self.nothing_sent() };
        let shown: Vec<(u64, Rect)> = screen.panes.iter().map(|pane| (pane.pane, pane.rect)).collect();
        let beside = split::beside(&shown, pane, side);
        if let Some((watched, id)) = self.machines.watched.as_mut().zip(beside) {
            watched.order(Order::Focus(id));
        }
    }
}
