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
        nerd_font: false,
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
