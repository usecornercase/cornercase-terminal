use std::time::Instant;

use crossterm::event::{KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::drag::{RemoteDrag, half};
use super::stream::Watched;
use crate::app::App;
use crate::app::input::wheel;
use crate::app::interaction::Drag;
use crate::app::panes::{Wheel, pane_cell, within_size};
use crate::control::surface;
use crate::control::terminal::{End, InputEvent, Key, Mouse};
use crate::control::{Input, Scroll};
use crate::machines::stream::orders::Order;
use crate::machines::stream::screen::{RemotePane, RemoteScreen};
use crate::project::split::Divider;
use crate::ui;

#[derive(Debug, Default, PartialEq)]
pub(super) struct Pointed {
    pub(super) orders: Vec<Order>,
    pub(super) drag: Option<RemoteDrag>,
}

impl Pointed {
    pub(super) fn sent(order: Option<Order>) -> Self {
        Self { orders: order.into_iter().collect(), drag: None }
    }
}

impl App {
    pub(in crate::app) fn machine_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> bool {
        if self.shown_remote().is_none() {
            return false;
        }
        let acts = wheel(ev.kind).is_some() || matches!(ev.kind, MouseEventKind::Down(_));
        let Some((stream, screen)) = self.watched().and_then(Watched::live) else {
            if acts && area.contains(pos) {
                self.nothing_sent();
            }
            return true;
        };
        let mut pointed = pointed(screen, ev, pos, area, stream.tab());
        if let Some(RemoteDrag::Divider { tab, path }) = &pointed.drag
            && self.screen.clicks.divider_pressed(*tab, path, Instant::now())
        {
            pointed = Pointed::sent(Some(half(*tab, path.clone())));
        }
        self.point(pointed);
        true
    }

    pub(super) fn point(&mut self, pointed: Pointed) {
        self.screen.drag = pointed.drag.map(Drag::Remote);
        let dropped = self.machines.watched.as_mut().is_some_and(|watched| {
            pointed.orders.into_iter().map(|order| watched.order(order)).filter(|dropped| *dropped).count() > 0
        });
        self.typed_dropped(dropped);
    }
}

fn pointed(screen: &RemoteScreen, ev: MouseEvent, pos: Position, area: Rect, tab: u64) -> Pointed {
    let under = under(screen, pos, area);
    if let Some(delta) = wheel(ev.kind) {
        return Pointed::sent(under.and_then(|pane| wheeled(pane, ev, area, delta)));
    }
    match ev.kind {
        MouseEventKind::Down(_) => pressed(screen, ev, pos, area, tab),
        MouseEventKind::Moved if !under.is_some_and(|pane| pane.active) => Pointed::default(),
        _ => Pointed::sent(screen.panes.iter().find(|pane| pane.active).and_then(|pane| clicked(pane, ev, area))),
    }
}

fn pressed(screen: &RemoteScreen, ev: MouseEvent, pos: Position, area: Rect, tab: u64) -> Pointed {
    let left = ev.kind == MouseEventKind::Down(MouseButton::Left);
    if left && let Some(divider) = divider_at(screen, pos, area) {
        let path = divider.path.clone();
        return Pointed { orders: Vec::new(), drag: Some(RemoteDrag::Divider { tab, path }) };
    }
    let Some(pane) = under(screen, pos, area) else { return Pointed::default() };
    let badge = ui::remote_rect(pane.rect, area).and_then(|rect| ui::scrollback_badge(rect, pane.snapshot.below));
    if left && badge.is_some_and(|badge| badge.contains(pos)) {
        return Pointed::sent(Some(Order::Scroll(Scroll {
            pane: Some(pane.pane),
            lines: None,
            to: Some(End::Bottom),
        })));
    }
    let reads = pane.modes.mouse != surface::Mouse::None;
    if !pane.active {
        let right = ev.kind == MouseEventKind::Down(MouseButton::Right);
        return match (left, right && reads) {
            (true, _) => Pointed::sent(Some(Order::Focus(pane.pane))),
            (false, true) => {
                let mut press = held(pane, ev, area);
                press.orders.insert(0, Order::Focus(pane.pane));
                press
            }
            (false, false) => Pointed::default(),
        };
    }
    if reads {
        return held(pane, ev, area);
    }
    let from = cell(pane, ev, area).filter(|_| left);
    Pointed { orders: Vec::new(), drag: from.map(|from| RemoteDrag::Selection { pane: pane.pane, from, to: None }) }
}

fn held(pane: &RemotePane, ev: MouseEvent, area: Rect) -> Pointed {
    let order = clicked(pane, ev, area);
    let drag = order.is_some().then_some(RemoteDrag::Mouse { pane: pane.pane });
    Pointed { orders: order.into_iter().collect(), drag }
}

fn under(screen: &RemoteScreen, pos: Position, area: Rect) -> Option<&RemotePane> {
    screen.panes.iter().find(|pane| ui::remote_rect(pane.rect, area).is_some_and(|rect| rect.contains(pos)))
}

pub(super) fn divider_at(screen: &RemoteScreen, pos: Position, area: Rect) -> Option<&Divider> {
    let at = area.contains(pos).then(|| there(pos, area))?;
    screen.dividers.iter().find(|divider| divider.grab().contains(at))
}

pub(super) fn there(pos: Position, area: Rect) -> Position {
    Position::new(pos.x.saturating_sub(area.x), pos.y.saturating_sub(area.y))
}

fn wheeled(pane: &RemotePane, ev: MouseEvent, area: Rect, delta: isize) -> Option<Order> {
    let modes = &pane.modes;
    match Wheel::of(modes.mouse != surface::Mouse::None, modes.alternate_screen, modes.alternate_scroll, delta) {
        Wheel::Program => clicked(pane, ev, area),
        Wheel::Scroll(rows) => {
            let lines = i64::try_from(rows).ok();
            Some(Order::Scroll(Scroll { pane: Some(pane.pane), lines, to: None }))
        }
        Wheel::Keys(code, times) => {
            let key = Key::of(&KeyEvent::new(code, KeyModifiers::NONE))?;
            Some(typed(pane.pane, vec![InputEvent::Key(key); times]))
        }
        Wheel::Nothing => None,
    }
}

pub(super) fn clicked(pane: &RemotePane, ev: MouseEvent, area: Rect) -> Option<Order> {
    let mode = pane.modes.mouse;
    if mode == surface::Mouse::None || (ev.kind == MouseEventKind::Moved && mode != surface::Mouse::AnyMotion) {
        return None;
    }
    let at = cell(pane, ev, area)?;
    Some(input(pane, vec![InputEvent::Mouse(Mouse::of(ev, at.x, at.y))]))
}

pub(super) fn cell(pane: &RemotePane, ev: MouseEvent, area: Rect) -> Option<Position> {
    pane_cell(within_size(ui::remote_rect(pane.rect, area)?, pane.size.cols, pane.size.rows), ev)
}

pub(super) fn typed(pane: u64, events: Vec<InputEvent>) -> Order {
    Order::Input(Input { pane: Some(pane), popup: false, size: None, events })
}

fn input(pane: &RemotePane, events: Vec<InputEvent>) -> Order {
    Order::Input(Input { pane: Some(pane.pane), popup: false, size: Some(pane.size), events })
}

#[cfg(test)]
mod tests;
