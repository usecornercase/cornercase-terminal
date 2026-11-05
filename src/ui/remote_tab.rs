use std::sync::Arc;

use ratatui::Frame;
use ratatui::layout::Rect;

use super::View;
use super::pane::{draw_dividers, draw_screen, draw_scrollback_badge};
use super::remote_card::draw_remote_card;
use crate::machines::stream::screen::RemoteScreen;
use crate::project::split::Divider;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RemoteTab {
    pub(crate) screen: Arc<RemoteScreen>,
    pub(crate) dim_inactive: bool,
    pub(crate) down: bool,
    pub(crate) dragging: Option<Vec<bool>>,
}

pub(super) fn draw_remote(f: &mut Frame, view: &View, area: Rect) -> bool {
    if let Some(tab) = &view.remote_tab {
        draw_remote_tab(f, view, tab, area);
    }
    if let Some(card) = &view.remote_card {
        draw_remote_card(f, view, card, area);
    }
    view.remote_tab.is_some() || view.remote_card.is_some()
}

pub(super) fn draw_remote_tab(f: &mut Frame, view: &View, tab: &RemoteTab, area: Rect) {
    let split = tab.screen.panes.len() > 1;
    for pane in &tab.screen.panes {
        let Some(rect) = remote_rect(pane.rect, area) else { continue };
        let dim = tab.down || (split && !pane.active && tab.dim_inactive);
        let cursor = pane.active && !tab.down && view.overlay.is_none();
        draw_screen(f, &pane.snapshot, rect, cursor, dim);
        draw_scrollback_badge(f, view, rect, pane.snapshot.below);
    }
    let dividers: Vec<Divider> = tab.screen.dividers.iter().filter_map(|divider| moved(divider, area)).collect();
    draw_dividers(f, view, &dividers, tab.dragging.as_deref());
}

pub(crate) fn remote_rect(rect: Rect, area: Rect) -> Option<Rect> {
    let moved = Rect { x: area.x.saturating_add(rect.x), y: area.y.saturating_add(rect.y), ..rect };
    Some(moved.intersection(area)).filter(|rect| !rect.is_empty())
}

fn moved(divider: &Divider, area: Rect) -> Option<Divider> {
    let line = remote_rect(divider.line, area)?;
    let around = remote_rect(divider.area, area)?;
    Some(divider.placed(around, line))
}
