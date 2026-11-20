use std::collections::BTreeSet;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rstest::rstest;

use super::*;

const WIDE: usize = 100;

fn row(id: u64, label: &str, state: State) -> Row {
    Row {
        id,
        when: "2026-10-15 09:00".into(),
        label: label.into(),
        first_line: String::new(),
        status: "pending".into(),
        origin: "claude · shop".into(),
        state,
        kind: Kind::Entry,
        live: false,
        unsent: false,
        routine: None,
        at: i64::try_from(id).expect("small") * 100,
        ran: 0,
    }
}

fn routine(id: u64, label: &str, at: i64) -> Row {
    Row { kind: Kind::Routine { paused: false }, at, ..row(id, label, State::Pending) }
}

fn run(id: u64, of: u64, ran: i64) -> Row {
    let state = State::Answered { session: None };
    Row { kind: Kind::Run, routine: Some(of), ran, ..row(id, "Review", state) }
}

fn ran(id: u64, label: &str, at: i64) -> Row {
    Row { ran: at, ..row(id, label, State::Answered { session: None }) }
}

fn dialog(rows: Vec<Row>) -> Dialog {
    with_folds(rows, &[])
}

fn with_folds(rows: Vec<Row>, folded: &[u64]) -> Dialog {
    let mut dialog = Dialog::new(None);
    dialog.set(rows, &folded.iter().copied().collect(), 0);
    dialog
}

fn ids(dialog: &Dialog) -> Vec<u64> {
    dialog.shown().iter().map(|shown| shown.row.id).collect()
}

fn press(dialog: &mut Dialog, code: KeyCode) -> Asked {
    dialog.key(KeyEvent::new(code, KeyModifiers::NONE), 10)
}

fn labels(dialog: &Dialog, width: usize) -> Vec<&'static str> {
    dialog.buttons(width).into_iter().map(Button::label).collect()
}

fn grouped() -> Vec<Row> {
    vec![run(11, 1, 500), routine(1, "Review", 900), row(2, "Fix the login", State::Pending), run(12, 1, 700)]
}

mod order {
    use super::*;

    #[test]
    fn upcoming_rows_come_by_time_then_paused_routines_then_what_ran_newest_first() {
        let paused = Row { kind: Kind::Routine { paused: true }, ..routine(3, "Triage", 100) };
        let rows = vec![
            ran(5, "Old", 1_000),
            paused,
            row(2, "Later", State::Pending),
            ran(6, "New", 2_000),
            routine(1, "Review", 150),
        ];
        assert_eq!(ids(&dialog(rows)), [1, 2, 3, 6, 5]);
    }

    #[test]
    fn a_routines_runs_follow_it_newest_first_wherever_they_were() {
        let shown: Vec<(u64, bool)> = dialog(grouped()).shown().iter().map(|shown| (shown.row.id, shown.run)).collect();
        assert_eq!(shown, [(2, false), (1, false), (12, true), (11, true)]);
    }

    #[test]
    fn a_run_whose_routine_is_gone_is_listed_as_a_one_off_that_ran() {
        let rows = vec![run(11, 99, 500), row(2, "Fix the login", State::Pending)];
        let dialog = dialog(rows);
        let shown: Vec<(u64, bool)> = dialog.shown().iter().map(|shown| (shown.row.id, shown.run)).collect();
        assert_eq!(shown, [(2, false), (11, false)]);
    }
}

mod folds {
    use super::*;

    #[test]
    fn only_a_routine_with_runs_has_a_fold_mark() {
        let mut rows = grouped();
        rows.push(routine(3, "Triage", 2_000));
        let dialog = dialog(rows);
        let folds: Vec<(u64, Option<bool>)> = dialog.shown().iter().map(|shown| (shown.row.id, shown.fold)).collect();
        assert_eq!(folds, [(2, None), (1, Some(false)), (12, None), (11, None), (3, None)]);
    }

    #[test]
    fn a_folded_routine_hides_its_runs_and_counts_them() {
        let dialog = with_folds(grouped(), &[1]);
        assert_eq!(ids(&dialog), [2, 1]);
        let shown = dialog.shown();
        assert_eq!((shown[1].fold, shown[1].hidden), (Some(true), 2));
    }

    #[test]
    fn left_on_a_run_selects_its_routine_and_asks_to_fold_it() {
        let mut dialog = dialog(grouped());
        dialog.select(11);
        assert_eq!(press(&mut dialog, KeyCode::Left), Asked::Fold(1));
        assert_eq!(dialog.selected().map(|row| row.id), Some(1));
    }

    #[test]
    fn left_folds_an_unfolded_routine_and_right_unfolds_a_folded_one() {
        let mut dialog = dialog(grouped());
        dialog.select(1);
        assert_eq!(press(&mut dialog, KeyCode::Right), Asked::None);
        assert_eq!(press(&mut dialog, KeyCode::Left), Asked::Fold(1));
        dialog.set(grouped(), &BTreeSet::from([1]), 0);
        assert_eq!(press(&mut dialog, KeyCode::Left), Asked::None);
        assert_eq!(press(&mut dialog, KeyCode::Right), Asked::Fold(1));
    }

    #[test]
    fn left_and_right_do_nothing_on_a_one_off_entry() {
        let mut dialog = dialog(grouped());
        dialog.select(2);
        assert_eq!(press(&mut dialog, KeyCode::Left), Asked::None);
        assert_eq!(press(&mut dialog, KeyCode::Right), Asked::None);
    }

    #[test]
    fn a_query_shows_a_matching_run_of_a_folded_routine_with_its_routine_and_no_mark() {
        let mut rows = grouped();
        rows[0].first_line = "the nightly needle".into();
        let mut dialog = with_folds(rows, &[1]);
        dialog.paste("needle");
        let shown: Vec<(u64, bool, Option<bool>)> =
            dialog.shown().iter().map(|shown| (shown.row.id, shown.run, shown.fold)).collect();
        assert_eq!(shown, [(1, false, None), (11, true, None)]);
        assert_eq!(press(&mut dialog, KeyCode::Left), Asked::None);
    }
}

mod selection {
    use super::*;

    #[rstest]
    #[case::a_run_arrives_above(vec![routine(1, "Review", 900), run(13, 1, 800), run(12, 1, 700), run(11, 1, 500), row(2, "Fix", State::Pending)])]
    #[case::a_row_above_is_removed(vec![run(11, 1, 500), row(2, "Fix", State::Pending)])]
    fn the_selection_stays_on_its_entry_when_rows_change_above_it(#[case] rows: Vec<Row>) {
        let mut dialog = dialog(grouped());
        dialog.select(2);
        press(&mut dialog, KeyCode::Down);
        press(&mut dialog, KeyCode::Down);
        press(&mut dialog, KeyCode::Down);
        assert_eq!(dialog.selected().map(|row| row.id), Some(11));
        dialog.set(rows, &BTreeSet::new(), 0);
        assert_eq!(dialog.selected().map(|row| row.id), Some(11));
    }

    #[test]
    fn a_selection_that_is_gone_falls_back_to_the_first_row() {
        let mut dialog = dialog(grouped());
        dialog.select(77);
        assert_eq!(dialog.index(), Some(0));
        assert_eq!(dialog.selected().map(|row| row.id), Some(2));
    }
}

mod buttons {
    use super::*;

    fn starting() -> Row {
        row(3, "Fix logout", State::Starting)
    }

    fn started() -> Row {
        row(2, "Update the docs", State::Started { pane: 7, server: "s".into() })
    }

    #[rstest]
    #[case::a_pending_entry(row(1, "Fix", State::Pending), &["new", "start now", "remove", "clear started", "done"][..])]
    #[case::a_started_entry(started(), &["new", "remove", "clear started", "done"])]
    #[case::a_starting_entry(starting(), &["new", "clear started", "done"])]
    #[case::a_routine(routine(1, "Review", 1), &["new", "start now", "pause", "remove", "clear started", "done"])]
    #[case::a_paused_routine(Row { kind: Kind::Routine { paused: true }, ..routine(1, "Review", 1) }, &["new", "start now", "resume", "remove", "clear started", "done"])]
    #[case::a_run(Row { state: State::Pending, ..run(1, 99, 0) }, &["new", "remove", "clear started", "done"])]
    fn the_buttons_follow_the_selected_row(#[case] selected: Row, #[case] expected: &[&str]) {
        let id = selected.id;
        let mut dialog = dialog(vec![selected, Row { id: 50, ..started() }]);
        dialog.select(id);
        assert_eq!(labels(&dialog, WIDE), expected);
    }

    #[test]
    fn clear_started_shows_for_a_removed_run_but_not_for_routines_alone() {
        let dialog = dialog(vec![routine(1, "Review", 1)]);
        assert!(!labels(&dialog, WIDE).contains(&"clear started"));
        let removed = Row { state: State::Removed, ..run(5, 1, 0) };
        let with_run = super::dialog(vec![routine(1, "Review", 1), removed]);
        assert!(labels(&with_run, WIDE).contains(&"clear started"));
    }

    #[test]
    fn a_narrow_row_drops_clear_started_only() {
        let dialog = dialog(vec![routine(1, "Review", 1), ran(2, "Done", 5)]);
        assert_eq!(labels(&dialog, 40), ["new", "start now", "pause", "remove", "done"]);
    }

    #[test]
    fn an_answered_entry_and_an_answered_run_are_clearable() {
        assert!(ran(1, "Reminder", 1).clearable());
        assert!(run(2, 1, 0).clearable());
        assert!(!row(3, "Reminder", State::Pending).clearable());
    }
}

mod filter {
    use super::*;

    fn typed(dialog: &mut Dialog, text: &str) {
        for c in text.chars() {
            press(dialog, KeyCode::Char(c));
        }
    }

    #[test]
    fn typing_filters_the_rows_and_selects_the_first() {
        let mut dialog = dialog(vec![row(1, "Fix the login", State::Pending), row(3, "Fix logout", State::Pending)]);
        dialog.select(1);
        typed(&mut dialog, "logout");
        assert_eq!(ids(&dialog), [3]);
        assert_eq!(press(&mut dialog, KeyCode::Enter), Asked::Open(3));
        press(&mut dialog, KeyCode::Backspace);
        assert_eq!(dialog.query(), "logou");
    }

    #[test]
    fn typing_finds_an_entry_by_a_word_its_cut_name_lost() {
        let mut long = row(2, "Update the docs", State::Pending);
        long.first_line = "Update the docs and the changelog for the failing checks".into();
        let mut dialog = dialog(vec![row(1, "Fix", State::Pending), long]);
        typed(&mut dialog, "failing");
        assert_eq!(ids(&dialog), [2]);
    }

    #[test]
    fn typing_matches_the_time_and_the_meta() {
        let mut dialog = dialog(vec![row(1, "Fix", State::Pending)]);
        typed(&mut dialog, "claude · shop");
        assert_eq!(ids(&dialog), [1]);
        dialog.paste("x");
        assert_eq!(dialog.shown().len(), 0);
        assert!(!dialog.is_empty());
    }
}

mod clicks {
    use super::*;

    #[test]
    fn a_click_on_a_row_selects_and_opens_it() {
        let mut dialog = dialog(grouped());
        assert_eq!(dialog.click(Hit::Row(2), WIDE), Asked::Open(12));
        assert_eq!(dialog.selected().map(|row| row.id), Some(12));
        assert_eq!(dialog.click(Hit::Row(9), WIDE), Asked::None);
    }

    #[test]
    fn a_click_on_the_mark_selects_and_folds() {
        let mut dialog = dialog(grouped());
        assert_eq!(dialog.click(Hit::Fold(1), WIDE), Asked::Fold(1));
        assert_eq!(dialog.selected().map(|row| row.id), Some(1));
    }

    #[test]
    fn the_buttons_act_on_the_selected_row_and_done_closes() {
        let mut dialog = dialog(grouped());
        dialog.select(1);
        let at = |dialog: &Dialog, label: &str| {
            labels(dialog, WIDE).iter().position(|shown| *shown == label).expect("the button")
        };
        for (label, asked) in [
            ("new", Asked::New),
            ("start now", Asked::Run(1)),
            ("pause", Asked::Pause(1)),
            ("remove", Asked::Remove(1)),
            ("clear started", Asked::Clear),
            ("done", Asked::Close),
        ] {
            assert_eq!(dialog.click(Hit::Button(at(&dialog, label)), WIDE), asked, "{label}");
        }
    }

    #[test]
    fn escape_closes_and_enter_opens_the_selected_row() {
        let mut dialog = dialog(grouped());
        assert_eq!(press(&mut dialog, KeyCode::Enter), Asked::Open(2));
        assert_eq!(press(&mut dialog, KeyCode::Esc), Asked::Close);
        assert_eq!(press(&mut Dialog::new(None), KeyCode::Enter), Asked::None);
    }
}
