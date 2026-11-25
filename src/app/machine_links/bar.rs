use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::RemoteAt;
use super::clicks::Button;
use crate::app::App;
use crate::app::interaction::Grab;
use crate::app::layout::List;
use crate::control::{PaneInfo, TabInfo};
use crate::ui::{self, tab_bar::Hit};

fn aimed(hit: Hit, kind: MouseEventKind) -> Option<(Option<usize>, Option<Button>)> {
    let left = matches!(kind, MouseEventKind::Down(MouseButton::Left));
    let right = matches!(kind, MouseEventKind::Down(MouseButton::Right));
    match hit {
        Hit::Tab(t) if left => Some((Some(t), None)),
        Hit::Close(t) if left => Some((Some(t), Some(Button::Close))),
        Hit::Menu(t) if left => Some((Some(t), Some(Button::Menu))),
        Hit::Tab(t) | Hit::Close(t) | Hit::Menu(t) if right => Some((Some(t), Some(Button::Menu))),
        Hit::New if left => Some((None, Some(Button::NewTab))),
        _ => None,
    }
}

impl App {
    fn shown_tabs(&self) -> Option<&[TabInfo]> {
        self.shown_place().map(|(_, workspace)| workspace.tabs.as_slice())
    }

    pub(in crate::app) fn remote_bar_tabs(&self) -> Option<Vec<ui::TabEntry>> {
        let link = self.shown_remote()?.link;
        Some(self.shown_tabs()?.iter().map(|tab| self.remote_tab(link, tab)).collect())
    }

    pub(in crate::app) fn remote_bar_active(&self) -> Option<usize> {
        let shown = self.shown_remote()?.tab?;
        self.shown_tabs()?.iter().position(|tab| tab.id == shown)
    }

    pub(in crate::app) fn remote_bar_view(&self) -> Option<ui::tab_bar::TabBar> {
        let tabs = self.remote_bar_tabs()?;
        let link = self.shown_remote()?.link;
        let machine = self.remote_projects().into_iter().find(|remote| remote.link == link).map(|remote| remote.tag);
        let active = self.remote_bar_active();
        let tab = active.and_then(|i| self.shown_tabs()?.get(i));
        let details = tab.and_then(|tab| self.bar_pane(tab)).map(|pane| self.remote_pane_details(pane));
        Some(ui::tab_bar::TabBar {
            tabs,
            active,
            details: details.unwrap_or_default(),
            scroll: self.scroll.of(List::TabBar),
            machine,
        })
    }

    fn bar_pane<'a>(&self, tab: &'a TabInfo) -> Option<&'a PaneInfo> {
        let live = self.live_pane().filter(|(screen, _)| screen.tab == tab.id).map(|(_, pane)| pane);
        let framed = live.and_then(|id| tab.panes.iter().find(|pane| pane.id == id));
        framed.or_else(|| tab.panes.iter().find(|pane| pane.active)).or_else(|| tab.panes.first())
    }

    fn remote_pane_details(&self, pane: &PaneInfo) -> ui::Details {
        let subagents = usize::try_from(pane.subagents).unwrap_or(usize::MAX);
        let details = self.context_details((pane.model.clone(), pane.context), pane.memory, subagents, None);
        ui::Details { compacted: self.shows_compaction(pane.compacted), ..details }
    }

    pub(in crate::app) fn remote_bar_press(
        &mut self,
        strip: &ui::tab_bar::Strip,
        hit: Hit,
        kind: MouseEventKind,
        (pos, area): (Position, Rect),
    ) {
        let Some(shown) = self.shown_remote() else { return };
        let Some((tab, button)) = aimed(hit, kind) else { return };
        let at = match tab {
            Some(t) => {
                let Some(id) = self.shown_tabs().and_then(|tabs| tabs.get(t)).map(|tab| tab.id) else { return };
                RemoteAt::Tab { link: shown.link, workspace: shown.workspace, id }
            }
            None => RemoteAt::Workspace { link: shown.link, id: shown.workspace },
        };
        if self.press_remote_button(at, button, (pos, false)) {
            return;
        }
        if let (Some(t), None) = (tab, button) {
            self.grab_with(Grab::Remote(at), strip.item(t), area, false);
        }
    }
}
