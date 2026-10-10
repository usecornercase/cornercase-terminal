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
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::graphics::CellSize;

    const HI: u8 = 42;
    const NOTHING: &[u8] = b"";
    const RECT: Rect = Rect { x: 50, y: 3, width: 20, height: 10 };

    fn support(protocol: Protocol, tmux: Tmux) -> Support {
        Support {
            protocol: Some(protocol),
            missing: None,
            cell: Some(CellSize { width: 10, height: 20 }),
            tmux,
            id_hi: HI,
        }
    }

    const fn fit(cols: u16, rows: u16, width: u32, height: u32) -> Fit {
        Fit { cols, rows, width, height, cell: CellSize { width: 10, height: 20 } }
    }

    fn placed(graphics: &Graphics, picture: u64, fit: Fit, payload: Option<&[u8]>) -> Placed {
        let sight = graphics.sight(Rect::new(0, 0, 120, 40), Some(picture));
        let protocol = graphics.support.protocol.expect("a protocol");
        let kitty_id = if protocol == Protocol::Kitty { kitty::id(HI, sight.lo) } else { 0 };
        let key = Key { picture, protocol, tmux: graphics.support.tmux, fit, background: None, kitty_id };
        let rect = Rect { width: fit.cols, height: fit.rows, ..RECT };
        let drawn = payload.is_some() || sight.sent == Some(picture);
        Placed { key, rect, path: "logo.png".into(), payload: payload.map(|p| Arc::new(p.to_vec())), drawn }
    }

    mod kitty_images {
        use super::*;

        const FIT: Fit = fit(20, 10, 200, 200);
        const SMALLER: Fit = fit(10, 5, 100, 100);
        const BIGGER: Fit = fit(30, 15, 300, 300);

        fn kitty() -> Graphics {
            Graphics::new(1, support(Protocol::Kitty, Tmux::None), None)
        }

        #[test]
        fn sends_an_image_once() {
            let mut graphics = kitty();
            let now = Instant::now();
            let first = placed(&graphics, 7, FIT, Some(b"transmit"));

            assert_eq!(graphics.after(Some(7), Some(&first), now), b"transmit");
            let again = placed(&graphics, 7, FIT, Some(b"transmit"));
            assert_eq!(again.key, first.key);
            assert_eq!(graphics.after(Some(7), Some(&again), now), NOTHING);
        }

        #[test]
        fn a_smaller_room_only_places_it_again() {
            let mut graphics = kitty();
            let now = Instant::now();
            graphics.after(Some(7), Some(&placed(&graphics, 7, FIT, Some(b"transmit"))), now);

            let smaller = placed(&graphics, 7, SMALLER, None);

            assert!(smaller.drawn, "the cells show what the terminal already has");
            assert_eq!(graphics.after(Some(7), Some(&smaller), now), kitty::place(smaller.key.kitty_id, 10, 5));
        }

        #[test]
        fn a_bigger_room_sends_it_again_once_it_is_ready() {
            let mut graphics = kitty();
            let now = Instant::now();
            graphics.after(Some(7), Some(&placed(&graphics, 7, FIT, Some(b"transmit"))), now);

            let waiting = placed(&graphics, 7, BIGGER, None);
            assert_eq!(graphics.after(Some(7), Some(&waiting), now), kitty::place(waiting.key.kitty_id, 30, 15));
            let ready = placed(&graphics, 7, BIGGER, Some(b"bigger"));

            assert_eq!(graphics.after(Some(7), Some(&ready), now), b"bigger");
        }

        #[test]
        fn a_new_image_takes_the_other_id_and_the_old_one_goes() {
            let mut graphics = kitty();
            let now = Instant::now();
            let old = placed(&graphics, 7, FIT, Some(b"old"));
            graphics.after(Some(7), Some(&old), now);

            let new = placed(&graphics, 8, FIT, Some(b"new"));
            let sent = graphics.after(Some(8), Some(&new), now);

            assert_ne!(new.key.kitty_id, old.key.kitty_id);
            assert_eq!(sent, [b"new".to_vec(), kitty::delete(old.key.kitty_id)].concat());
            assert_eq!(placed(&graphics, 7, FIT, None).key.kitty_id, old.key.kitty_id, "ids alternate");
        }

        #[test]
        fn a_hidden_image_stays_in_the_terminal() {
            let mut graphics = kitty();
            let now = Instant::now();
            graphics.after(Some(7), Some(&placed(&graphics, 7, FIT, Some(b"transmit"))), now);

            assert_eq!(graphics.after(Some(7), None, now), NOTHING);
            assert_eq!(graphics.sight(RECT, Some(7)).sent, Some(7));
        }

        #[test]
        fn an_image_that_leaves_is_deleted() {
            let mut graphics = kitty();
            let now = Instant::now();
            let shown = placed(&graphics, 7, FIT, Some(b"transmit"));
            graphics.after(Some(7), Some(&shown), now);

            assert_eq!(graphics.after(None, None, now), kitty::delete(shown.key.kitty_id));
            assert_eq!(graphics.after(None, None, now), NOTHING);
        }

        #[test]
        fn inside_tmux_nothing_is_sent_right_after_the_screen_is_cleared() {
            let mut graphics = Graphics::new(1, support(Protocol::Kitty, Tmux::Wrap), None);
            let now = Instant::now();
            graphics.reset(now);
            let first = placed(&graphics, 7, FIT, Some(b"wrapped"));

            assert_eq!(graphics.after(Some(7), Some(&first), now), NOTHING);
            assert_eq!(graphics.wake(now), Some(AFTER_RESET));
            assert_eq!(graphics.after(Some(7), Some(&first), now + AFTER_RESET), b"wrapped");
            assert_eq!(graphics.wake(now + AFTER_RESET), None);
        }

        #[test]
        fn a_send_held_back_never_wakes_the_server_once_its_time_has_passed() {
            let mut graphics = Graphics::new(1, support(Protocol::Kitty, Tmux::Wrap), None);
            let now = Instant::now();
            graphics.reset(now);
            graphics.after(Some(7), Some(&placed(&graphics, 7, FIT, Some(b"wrapped"))), now);

            graphics.update(support(Protocol::Iterm, Tmux::None));

            assert_eq!(graphics.wake(now), None, "nothing is held back for a terminal that changed");
            graphics.update(support(Protocol::Kitty, Tmux::Wrap));
            graphics.reset(now);
            graphics.after(Some(7), Some(&placed(&graphics, 7, FIT, Some(b"wrapped"))), now);
            assert_eq!(graphics.wake(now + AFTER_RESET * 2), None, "a past deadline is no reason to wake");
        }
    }

    mod cell_images {
        use super::*;

        const FIT: Fit = fit(20, 10, 200, 200);

        fn iterm() -> Graphics {
            Graphics::new(1, support(Protocol::Iterm, Tmux::None), None)
        }

        #[test]
        fn clears_its_cells_then_draws_the_image_in_one_synchronized_update() {
            let rect = Rect::new(4, 2, 3, 2);

            let sent = String::from_utf8(emit(rect, b"PAYLOAD")).expect("text");

            assert_eq!(sent, "\x1b[?2026h\x1b7\x1b[0m\x1b[3;5H   \x1b[4;5H   \x1b[3;5HPAYLOAD\x1b8\x1b[?2026l");
        }

        #[test]
        fn sends_an_image_once_while_nothing_covers_it() {
            let mut graphics = iterm();
            let now = Instant::now();
            let shown = placed(&graphics, 7, FIT, Some(b"image"));

            assert_eq!(graphics.after(Some(7), Some(&shown), now), emit(shown.rect, b"image"));
            assert_eq!(graphics.after(Some(7), Some(&shown), now), NOTHING);
            assert_eq!(graphics.watched(), Some(shown.rect));
        }

        #[test]
        fn sends_it_again_once_what_covered_it_is_gone() {
            let mut graphics = iterm();
            let now = Instant::now();
            let shown = placed(&graphics, 7, FIT, Some(b"image"));
            graphics.after(Some(7), Some(&shown), now);

            graphics.damaged();
            let covered = Placed { drawn: false, ..shown.clone() };
            assert_eq!(graphics.after(Some(7), Some(&covered), now), NOTHING, "nothing while a menu covers it");

            assert_eq!(graphics.after(Some(7), Some(&shown), now), emit(shown.rect, b"image"));
        }

        #[test]
        fn a_cleared_screen_gets_the_image_again() {
            let mut graphics = iterm();
            let now = Instant::now();
            let shown = placed(&graphics, 7, FIT, Some(b"image"));
            graphics.after(Some(7), Some(&shown), now);

            graphics.reset(now);

            assert_eq!(graphics.after(Some(7), Some(&shown), now), emit(shown.rect, b"image"));
        }

        #[test]
        fn an_image_that_comes_back_is_sent_again() {
            let mut graphics = iterm();
            let now = Instant::now();
            let shown = placed(&graphics, 7, FIT, Some(b"image"));
            graphics.after(Some(7), Some(&shown), now);

            graphics.after(None, None, now);

            assert_eq!(graphics.watched(), None);
            assert_eq!(graphics.after(Some(7), Some(&shown), now), emit(shown.rect, b"image"));
        }

        #[test]
        fn a_terminal_without_images_gets_nothing() {
            let mut graphics = Graphics::new(1, Support::default(), None);

            assert_eq!(graphics.after(None, None, Instant::now()), NOTHING);
        }
    }
}
