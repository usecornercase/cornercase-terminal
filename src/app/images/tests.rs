use super::*;
use crate::graphics::{CellSize, Tmux};

fn key(picture: u64) -> Key {
    let fit = encode::Fit { cols: 4, rows: 2, width: 40, height: 40, cell: CellSize { width: 10, height: 20 } };
    Key { picture, protocol: Protocol::Iterm, tmux: Tmux::None, fit, background: None, kitty_id: 0 }
}

#[test]
fn keeps_a_few_payloads_and_drops_the_oldest() {
    let mut payloads = Payloads::default();
    for picture in 0..=u64::try_from(MAX_PAYLOADS).expect("fits") {
        assert!(payloads.start(key(picture)));
        payloads.answered(key(picture), Some(vec![1]));
    }

    assert!(payloads.get(&key(0)).is_none(), "the oldest went first");
    assert!(payloads.get(&key(1)).is_some());
}

#[test]
fn a_payload_used_again_is_kept_longer() {
    let mut payloads = Payloads::default();
    for picture in 0..u64::try_from(MAX_PAYLOADS).expect("fits") {
        payloads.answered(key(picture), Some(vec![1]));
    }
    payloads.get(&key(0));

    payloads.answered(key(99), Some(vec![1]));

    assert!(payloads.get(&key(0)).is_some() && payloads.get(&key(1)).is_none());
}

#[test]
fn big_payloads_push_out_older_ones_but_never_the_last() {
    let mut payloads = Payloads::default();
    payloads.answered(key(1), Some(vec![0; MAX_PAYLOAD_BYTES / 2 + 1]));
    payloads.answered(key(2), Some(vec![0; MAX_PAYLOAD_BYTES / 2 + 1]));
    payloads.answered(key(3), Some(vec![0; MAX_PAYLOAD_BYTES + 1]));

    assert_eq!((payloads.get(&key(2)).is_none(), payloads.get(&key(3)).is_some()), (true, true));
}

#[test]
fn runs_few_jobs_at_once_and_never_the_same_twice() {
    let mut payloads = Payloads::default();

    assert!(payloads.start(key(1)));
    assert!(!payloads.start(key(1)));
    assert!(payloads.start(key(2)));
    assert!(!payloads.start(key(3)), "two jobs at most");
    payloads.answered(key(1), None);
    assert!(payloads.failed(&key(1)) && payloads.start(key(3)));
}

#[test]
fn a_cell_drawn_over_the_markers_is_written_and_the_image_is_not_owned() {
    let rect = Rect::new(1, 1, 3, 2);
    let mut buf = Buffer::empty(Rect::new(0, 0, 6, 4));
    for p in rect.positions() {
        buf[p].set_symbol(MARKER).set_diff_option(CellDiffOption::Skip);
    }
    assert!(own(&mut buf, rect, Protocol::Iterm));

    buf[(2, 1)].set_symbol("m");

    assert!(!own(&mut buf, rect, Protocol::Iterm));
    assert_eq!((buf[(2, 1)].diff_option, buf[(1, 1)].diff_option), (CellDiffOption::None, CellDiffOption::Skip));
}
