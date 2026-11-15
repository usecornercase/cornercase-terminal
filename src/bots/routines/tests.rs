use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{Asked, Button, Dialog, Hit};

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

const IDS: [u64; 4] = [10, 11, 12, 13];

mod keys {
    use super::*;

    #[test]
    fn down_and_up_move_by_entry_and_stop_at_the_ends() {
        let mut dialog = Dialog::new(1, None);
        assert_eq!(dialog.index(&IDS), Some(0));
        dialog.key(press(KeyCode::Down), &IDS, 4);
        assert_eq!(dialog.selected, Some(11));
        for _ in 0..9 {
            dialog.key(press(KeyCode::Down), &IDS, 4);
        }
        assert_eq!(dialog.selected, Some(13));
        for _ in 0..9 {
            dialog.key(press(KeyCode::Up), &IDS, 4);
        }
        assert_eq!(dialog.selected, Some(10));
    }

    #[test]
    fn page_home_and_end_move_by_pages_and_to_the_ends() {
        let mut dialog = Dialog::new(1, None);
        dialog.key(press(KeyCode::PageDown), &IDS, 2);
        assert_eq!(dialog.selected, Some(12));
        dialog.key(press(KeyCode::End), &IDS, 2);
        assert_eq!(dialog.selected, Some(13));
        assert_eq!(dialog.scroll, 2);
        dialog.key(press(KeyCode::PageUp), &IDS, 2);
        assert_eq!(dialog.selected, Some(11));
        dialog.key(press(KeyCode::Home), &IDS, 2);
        assert_eq!(dialog.selected, Some(10));
        assert_eq!(dialog.scroll, 0);
    }

    #[test]
    fn enter_opens_the_selected_entry_and_esc_closes() {
        let mut dialog = Dialog::new(1, Some(12));
        assert_eq!(dialog.key(press(KeyCode::Enter), &IDS, 4), Asked::Open(12));
        assert_eq!(dialog.key(press(KeyCode::Esc), &IDS, 4), Asked::Close);
        assert_eq!(dialog.key(press(KeyCode::Enter), &[], 4), Asked::None);
        assert_eq!(dialog.key(press(KeyCode::Char('x')), &IDS, 4), Asked::None);
    }

    #[test]
    fn the_selection_stays_on_its_entry_when_the_order_changes() {
        let mut dialog = Dialog::new(1, Some(11));
        let reordered = [11, 10, 13, 12];
        assert_eq!(dialog.index(&reordered), Some(0));
        dialog.key(press(KeyCode::Down), &reordered, 4);
        assert_eq!(dialog.selected, Some(10));
        assert_eq!(dialog.index(&IDS), Some(0));
    }
}

mod clicks {
    use super::*;

    #[test]
    fn a_click_on_a_row_selects_and_opens_it() {
        let mut dialog = Dialog::new(1, None);
        assert_eq!(dialog.click(Hit::Row(2), &IDS), Asked::Open(12));
        assert_eq!(dialog.selected, Some(12));
        assert_eq!(dialog.click(Hit::Row(9), &IDS), Asked::None);
        assert_eq!(dialog.selected, Some(12));
    }

    #[test]
    fn a_button_acts_on_its_own_row() {
        let mut dialog = Dialog::new(1, Some(10));
        assert_eq!(dialog.click(Hit::Button(3, Button::Pause), &IDS), Asked::Press(13, Button::Pause));
        assert_eq!(dialog.selected, Some(13));
    }

    #[test]
    fn new_and_done() {
        let mut dialog = Dialog::new(1, None);
        assert_eq!(dialog.click(Hit::New, &IDS), Asked::New);
        assert_eq!(dialog.click(Hit::Done, &IDS), Asked::Close);
    }
}

#[test]
fn index_falls_back_to_the_first_row_when_its_entry_is_gone() {
    let dialog = Dialog::new(1, Some(99));
    assert_eq!(dialog.index(&IDS), Some(0));
    assert_eq!(dialog.index(&[]), None);
}

#[test]
fn scroll_by_stays_inside_the_list() {
    let mut dialog = Dialog::new(1, None);
    dialog.scroll_by(5, 4, 2);
    assert_eq!(dialog.scroll, 2);
    dialog.scroll_by(-9, 4, 2);
    assert_eq!(dialog.scroll, 0);
}
