use std::io::Write as _;
use std::time::{Duration, Instant};

use ratatui::layout::Rect;

use crate::app::{Placed, Sight};
use crate::graphics::encode::{Fit, Key};
use crate::graphics::{KITTY_LO, Protocol, Support, Tmux, kitty, tmux};
use crate::log;

const AFTER_RESET: Duration = Duration::from_millis(100);
const BEGIN: &[u8] = b"\x1b[?2026h\x1b7\x1b[0m";
const END: &[u8] = b"\x1b8\x1b[?2026l";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Slot {
    picture: u64,
    sent: (u32, u32),
    placed: (u16, u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shown {
    key: Key,
    rect: Rect,
    intact: bool,
}

#[derive(Debug, Default)]
pub struct Graphics {
    client: u64,
    pub support: Support,
    pub background: Option<(u8, u8, u8)>,
    slots: [Option<Slot>; 2],
    current: Option<usize>,
    shown: Option<Shown>,
    hold: Option<Instant>,
    waiting: bool,
    logged: Option<u64>,
}

pub fn emit(rect: Rect, payload: &[u8]) -> Vec<u8> {
    let mut out = BEGIN.to_vec();
    let blank = " ".repeat(usize::from(rect.width));
    for y in rect.top()..rect.bottom() {
        let _ = write!(out, "\x1b[{};{}H{blank}", y + 1, rect.x + 1);
    }
    let _ = write!(out, "\x1b[{};{}H", rect.y + 1, rect.x + 1);
    out.extend_from_slice(payload);
    out.extend_from_slice(END);
    out
}

impl Graphics {
    pub fn new(client: u64, support: Support, background: Option<(u8, u8, u8)>) -> Self {
        Self { client, support, background, ..Self::default() }
    }

    pub fn update(&mut self, support: Support) {
        self.support = support;
        self.hold = None;
        self.waiting = false;
        self.damaged();
    }

    fn id(&self, slot: usize) -> u32 {
        kitty::id(self.support.id_hi, KITTY_LO[slot])
    }

    fn wrapped(&self, sequence: &[u8]) -> Vec<u8> {
        match self.support.tmux {
            Tmux::Wrap => tmux::wrap(sequence),
            Tmux::None => sequence.to_vec(),
        }
    }

    fn slot_for(&self, picture: Option<u64>) -> usize {
        if let Some(i) = self.slots.iter().position(|s| s.is_some_and(|s| Some(s.picture) == picture)) {
            return i;
        }
        match self.current {
            Some(i) => 1 - i,
            None => self.slots.iter().position(Option::is_none).unwrap_or(0),
        }
    }

    pub fn sight(&self, visible: Rect, picture: Option<u64>) -> Sight {
        let slot = self.slot_for(picture);
        let sent = self.slots[slot].map(|s| s.picture).filter(|p| Some(*p) == picture);
        Sight { support: self.support.clone(), visible, background: self.background, lo: KITTY_LO[slot], sent }
    }

    pub fn watched(&self) -> Option<Rect> {
        self.shown.map(|s| s.rect)
    }

    pub fn damaged(&mut self) {
        if let Some(shown) = &mut self.shown {
            shown.intact = false;
        }
    }

    pub fn reset(&mut self, now: Instant) {
        self.damaged();
        if self.support.tmux == Tmux::Wrap {
            self.hold = Some(now + AFTER_RESET);
        }
    }

    pub fn wake(&self, now: Instant) -> Option<Duration> {
        self.hold.filter(|_| self.waiting)?.checked_duration_since(now)
    }

    pub fn after(&mut self, picture: Option<u64>, placed: Option<&Placed>, now: Instant) -> Vec<u8> {
        match self.support.protocol {
            Some(Protocol::Kitty) => self.kitty(picture, placed, now),
            Some(Protocol::Iterm | Protocol::Sixel { .. }) => self.cells(placed),
            None => Vec::new(),
        }
    }

    fn sent(&mut self, placed: &Placed, bytes: usize) {
        let (client, protocol, path) = (self.client, placed.key.protocol.id(), &placed.path);
        let Fit { cols, rows, .. } = placed.key.fit;
        if self.logged.replace(placed.key.picture) == Some(placed.key.picture) {
            log::debug!("images", "image sent again", client = client, cols = cols, rows = rows, bytes = bytes);
        } else {
            log::info!("images", "image shown", client = client, protocol = protocol, path = path);
            log::debug!("images", "payload", client = client, cols = cols, rows = rows, bytes = bytes);
        }
    }

    fn forget(&mut self) -> Vec<u8> {
        self.current = None;
        self.logged = None;
        let mut out = Vec::new();
        for slot in 0..KITTY_LO.len() {
            if self.slots[slot].take().is_some() {
                out.extend(self.wrapped(&kitty::delete(self.id(slot))));
            }
        }
        out
    }

    fn kitty(&mut self, picture: Option<u64>, placed: Option<&Placed>, now: Instant) -> Vec<u8> {
        self.waiting = false;
        let Some(picture) = picture else { return self.forget() };
        let Some(placed) = placed.filter(|p| p.drawn && p.key.picture == picture) else { return Vec::new() };
        let Some(slot) = (0..KITTY_LO.len()).find(|&i| self.id(i) == placed.key.kitty_id) else { return Vec::new() };
        let Fit { cols, rows, width, height, .. } = placed.key.fit;
        let have = self.slots[slot].filter(|s| s.picture == picture);
        let enough = have.is_some_and(|s| s.sent.0 >= width && s.sent.1 >= height);
        let mut out = Vec::new();
        match (have, &placed.payload) {
            (_, Some(payload)) if !enough => {
                if self.hold.is_some_and(|until| now < until) {
                    self.waiting = true;
                    return out;
                }
                out.extend_from_slice(payload);
                self.slots[slot] = Some(Slot { picture, sent: (width, height), placed: (cols, rows) });
                self.sent(placed, payload.len());
            }
            (Some(was), _) if was.placed != (cols, rows) => {
                out.extend(self.wrapped(&kitty::place(placed.key.kitty_id, cols, rows)));
                self.slots[slot] = Some(Slot { placed: (cols, rows), ..was });
            }
            (Some(_), _) => {}
            (None, _) => return out,
        }
        let other = 1 - slot;
        if self.slots[other].take().is_some() {
            out.extend(self.wrapped(&kitty::delete(self.id(other))));
        }
        self.current = Some(slot);
        out
    }

    fn cells(&mut self, placed: Option<&Placed>) -> Vec<u8> {
        let Some(placed) = placed else {
            self.shown = None;
            self.logged = None;
            return Vec::new();
        };
        let Some(payload) = placed.payload.as_ref().filter(|_| placed.drawn) else { return Vec::new() };
        let shown = Shown { key: placed.key, rect: placed.rect, intact: true };
        if self.shown == Some(shown) {
            return Vec::new();
        }
        self.shown = Some(shown);
        self.sent(placed, payload.len());
        emit(placed.rect, payload)
    }
}

#[cfg(test)]
mod tests;
