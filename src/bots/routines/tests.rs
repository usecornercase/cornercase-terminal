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
        assert_eq!(dialog.list.selected, Some(11));
        for _ in 0..9 {
            dialog.key(press(KeyCode::Down), &IDS, 4);
        }
        assert_eq!(dialog.list.selected, Some(13));
        for _ in 0..9 {
            dialog.key(press(KeyCode::Up), &IDS, 4);
        }
        assert_eq!(dialog.list.selected, Some(10));
    }

    #[test]
    fn page_home_and_end_move_by_pages_and_to_the_ends() {
        let mut dialog = Dialog::new(1, None);
        dialog.key(press(KeyCode::PageDown), &IDS, 2);
        assert_eq!(dialog.list.selected, Some(12));
        dialog.key(press(KeyCode::End), &IDS, 2);
        assert_eq!(dialog.list.selected, Some(13));
        assert_eq!(dialog.list.scroll, 2);
        dialog.key(press(KeyCode::PageUp), &IDS, 2);
        assert_eq!(dialog.list.selected, Some(11));
        dialog.key(press(KeyCode::Home), &IDS, 2);
        assert_eq!(dialog.list.selected, Some(10));
        assert_eq!(dialog.list.scroll, 0);
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
        assert_eq!(dialog.list.selected, Some(10));
        assert_eq!(dialog.index(&IDS), Some(0));
    }
}

mod clicks {
    use super::*;

    #[test]
    fn a_click_on_a_row_selects_and_opens_it() {
        let mut dialog = Dialog::new(1, None);
        assert_eq!(dialog.click(Hit::Row(2), &IDS), Asked::Open(12));
        assert_eq!(dialog.list.selected, Some(12));
        assert_eq!(dialog.click(Hit::Row(9), &IDS), Asked::None);
        assert_eq!(dialog.list.selected, Some(12));
    }

    #[test]
    fn a_button_acts_on_its_own_row() {
        let mut dialog = Dialog::new(1, Some(10));
        assert_eq!(dialog.click(Hit::Button(3, Button::Pause), &IDS), Asked::Press(13, Button::Pause));
        assert_eq!(dialog.list.selected, Some(13));
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
    assert_eq!(dialog.list.scroll, 2);
    dialog.scroll_by(-9, 4, 2);
    assert_eq!(dialog.list.scroll, 0);
}

mod rules {
    use super::*;
    use crate::queue::repeat::Repeat;
    use crate::queue::{Entry, Place, State, Task};

    fn entry(state: State, repeat: Option<&str>) -> Entry {
        let task = Task {
            at: 1_800_000_000,
            zone: "UTC".into(),
            repeat: repeat.map(|text| Repeat::parse(text, "UTC", 1_800_000_000).expect("repeat")),
            place: Place { bot: Some("prs".into()), ..Place::default() },
            ..Task::default()
        };
        Entry::new(1, task, state)
    }

    #[test]
    fn a_routine_can_always_run_pause_edit_and_be_removed() {
        let buttons = super::super::buttons(&entry(State::Pending, Some("2h")));
        assert_eq!(buttons, [Button::RunNow, Button::Pause, Button::Edit, Button::Remove]);
        let mut paused = entry(State::Pending, Some("2h"));
        paused.task.paused = true;
        assert_eq!(super::super::buttons(&paused)[1], Button::Resume);
    }

    #[test]
    fn a_reminder_that_ran_can_only_be_removed_and_one_starting_nothing() {
        let answered = entry(State::Answered { session: None }, None);
        assert_eq!(super::super::buttons(&answered), [Button::Remove]);
        assert_eq!(super::super::buttons(&entry(State::Missed, None)), [Button::RunNow, Button::Edit, Button::Remove]);
        assert_eq!(super::super::buttons(&entry(State::Starting, None)), []);
    }

    #[test]
    fn only_a_run_that_started_left_a_block_in_the_chat() {
        assert!(super::super::left_a_block(&State::Starting));
        assert!(super::super::left_a_block(&State::Answered { session: None }));
        assert!(super::super::left_a_block(&State::Failed { reason: "x".into() }));
        assert!(!super::super::left_a_block(&State::Missed));
        assert!(!super::super::left_a_block(&State::Pending));
    }
}

#[test]
fn fit_brings_the_selection_into_view() {
    let ids: Vec<u64> = (0..12).collect();
    let mut dialog = Dialog::new(1, Some(11));
    dialog.fit(&ids, 10);
    assert_eq!(dialog.list.scroll, 2);
    let mut early = Dialog::new(1, Some(0));
    early.list.scroll = 5;
    early.fit(&ids, 10);
    assert_eq!(early.list.scroll, 0);
}

#[test]
fn a_wheel_scroll_stops_the_view_from_following_the_selection_until_a_move() {
    let ids: Vec<u64> = (0..14).collect();
    let mut dialog = Dialog::new(1, None);
    dialog.scroll_by(3, 14, 10);
    dialog.fit(&ids, 10);
    assert_eq!(dialog.list.scroll, 3);
    dialog.key(press(KeyCode::Down), &ids, 10);
    dialog.fit(&ids, 10);
    assert_eq!(dialog.list.scroll, 1);
    assert_eq!(dialog.list.selected, Some(1));
}
