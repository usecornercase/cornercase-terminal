use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::mouse::{Pointed, cell, clicked, divider_at, there};
use super::stream::Watched;
use crate::app::App;
use crate::control::Ratio;
use crate::control::terminal::{Point, Selection};
use crate::machines::stream::orders::Order;
use crate::machines::stream::screen::{RemotePane, RemoteScreen};
use crate::project::split::{self, Dir};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) enum RemoteDrag {
    Divider { tab: u64, path: Vec<bool> },
    Selection { pane: u64, from: Position, to: Option<Position> },
    Mouse { pane: u64 },
}

impl App {
    pub(in crate::app) fn drag_remote(&mut self, drag: RemoteDrag, ev: MouseEvent, area: Rect) {
        if matches!(drag, RemoteDrag::Divider { .. }) && matches!(ev.kind, MouseEventKind::Drag(_)) {
            self.screen.clicks.forget_divider();
        }
        let pointed = match self.watched().and_then(Watched::live) {
            Some((_, screen)) => dragged(screen, drag, ev, area),
            None => Pointed::default(),
        };
        self.point(pointed);
    }

    pub(in crate::app) fn remote_divider(&self, path: &[bool]) -> Option<Dir> {
        let (_, screen) = self.watched()?.live()?;
        screen.dividers.iter().find(|divider| divider.path == path).map(|divider| divider.dir)
    }

    pub(in crate::app) fn remote_divider_at(&self, pos: Position, area: Rect) -> Option<Dir> {
        let (_, screen) = self.watched()?.live()?;
        divider_at(screen, pos, area).map(|divider| divider.dir)
    }

    pub(super) fn copy_remote(&mut self, text: Option<&str>) {
        if let Some(text) = text.filter(|text| !text.is_empty()) {
            self.copy(text);
        }
    }
}

pub(super) fn half(tab: u64, path: Vec<bool>) -> Order {
    Order::Ratio(Ratio { tab: Some(tab), path, ratio: f64::from(split::HALF) })
}

fn dragged(screen: &RemoteScreen, drag: RemoteDrag, ev: MouseEvent, area: Rect) -> Pointed {
    let pane = |id: u64| screen.panes.iter().find(|pane| pane.pane == id);
    match drag {
        RemoteDrag::Divider { tab, path } if ev.kind == MouseEventKind::Drag(MouseButton::Left) => {
            let at = there(Position::new(ev.column, ev.row), area);
            let ratio = screen.dividers.iter().find(|divider| divider.path == path).map(|divider| {
                Order::Ratio(Ratio {
                    tab: Some(tab),
                    path: path.clone(),
                    ratio: f64::from(split::ratio_at(divider, at)),
                })
            });
            Pointed { orders: ratio.into_iter().collect(), drag: Some(RemoteDrag::Divider { tab, path }) }
        }
        RemoteDrag::Selection { pane: id, from, to } => {
            pane(id).map_or_else(Pointed::default, |pane| selecting(pane, (from, to), ev, area))
        }
        RemoteDrag::Mouse { pane: id } if matches!(ev.kind, MouseEventKind::Drag(_) | MouseEventKind::Up(_)) => {
            let order = pane(id).and_then(|pane| clicked(pane, ev, area));
            let held = matches!(ev.kind, MouseEventKind::Drag(_));
            Pointed { orders: order.into_iter().collect(), drag: held.then_some(RemoteDrag::Mouse { pane: id }) }
        }
        RemoteDrag::Divider { .. } | RemoteDrag::Mouse { .. } => Pointed::default(),
    }
}

fn selecting(pane: &RemotePane, (from, to): (Position, Option<Position>), ev: MouseEvent, area: Rect) -> Pointed {
    let Some(at) = cell(pane, ev, area) else { return Pointed::default() };
    let picked = Selection {
        pane: Some(pane.pane),
        size: Some(pane.size),
        from: Some(point(from)),
        to: Some(point(at)),
        ..Selection::default()
    };
    match (ev.kind, to) {
        (MouseEventKind::Drag(MouseButton::Left), _) => Pointed {
            orders: vec![Order::Select(picked)],
            drag: Some(RemoteDrag::Selection { pane: pane.pane, from, to: Some(at) }),
        },
        (_, Some(_)) => Pointed::sent(Some(Order::Copy(picked))),
        (_, None) => {
            Pointed::sent(Some(Order::Select(Selection { pane: Some(pane.pane), clear: true, ..Selection::default() })))
        }
    }
}

fn point(at: Position) -> Point {
    Point { x: at.x, y: at.y }
}

#[cfg(test)]
mod tests;
