use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::list::{row, scan_of};
use crate::worktrees::builds::{Folder, Size};
use crate::worktrees::{Action, Dialog, Hit, Kind, List, Live, Order, Step};

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn dialog() -> Dialog {
    let mut list = List::new(vec![row("p", "a", Kind::Linked), row("p", "b", Kind::Linked)]);
    list.scanned(&PathBuf::from("/wt/p/a"), Ok(scan_of(Some(Duration::from_secs(60)), false)));
    list.scanned(&PathBuf::from("/wt/p/b"), Ok(scan_of(Some(Duration::from_secs(120)), false)));
    Dialog::new(None, 1, list)
}

#[test]
fn space_ticks_and_enter_confirms() {
    let mut d = dialog();
    assert_eq!(d.key(press(KeyCode::Char(' '))), Action::Toggle(0));
    assert!(d.list.rows[0].ticked);
    assert_eq!(d.key(press(KeyCode::Enter)), Action::Rescan(vec![0]));
    assert!(matches!(d.step, Step::Confirm { .. }));
    assert_eq!(d.checking(), 1);
}

#[test]
fn enter_does_nothing_with_nothing_ticked() {
    let mut d = dialog();
    assert_eq!(d.key(press(KeyCode::Enter)), Action::None);
    assert_eq!(d.step, Step::List);
}

#[test]
fn confirm_rescans_the_ticked_rows_and_waits() {
    let mut d = dialog();
    d.toggle(0);
    d.toggle(1);
    assert_eq!(d.confirm(), Action::Rescan(vec![0, 1]));
    assert_eq!(d.checking(), 2);
    assert!(matches!(d.step, Step::Confirm { branches: false, .. }));
    assert!(!d.can_remove());
}

#[test]
fn remove_is_refused_until_the_rescan_answers() {
    let mut d = dialog();
    d.toggle(0);
    d.confirm();
    assert_eq!(d.key(press(KeyCode::Enter)), Action::None);
    let path = d.list.rows[0].found.path.clone();
    d.rescanned(&path, Ok(scan_of(None, false)));
    assert!(d.can_remove());
    assert_eq!(d.key(press(KeyCode::Enter)), Action::Remove { branches: false });
    assert_eq!(d.list.rows[0].scan, Some(Ok(scan_of(None, false))));
}

#[test]
fn esc_goes_back_and_keeps_the_ticks() {
    let mut d = dialog();
    d.toggle(1);
    d.confirm();
    assert_eq!(d.key(press(KeyCode::Esc)), Action::Back);
    assert_eq!(d.step, Step::List);
    assert_eq!(d.list.ticked(), vec![1]);
}

#[test]
fn esc_clears_the_filter_before_closing() {
    let mut d = dialog();
    d.key(press(KeyCode::Char('a')));
    assert_eq!(d.list.filter, "a");
    assert_eq!(d.key(press(KeyCode::Esc)), Action::None);
    assert_eq!(d.list.filter, "");
    assert_eq!(d.key(press(KeyCode::Esc)), Action::Close);
}

#[test]
fn branches_are_off_by_default() {
    let mut d = dialog();
    d.toggle(0);
    d.confirm();
    let path = d.list.rows[0].found.path.clone();
    d.rescanned(&path, Ok(scan_of(None, false)));
    assert_eq!(d.checking(), 0);
    assert!(matches!(d.step, Step::Confirm { branches: false, .. }));
    d.key(press(KeyCode::Char(' ')));
    assert_eq!(d.key(press(KeyCode::Enter)), Action::Remove { branches: true });
}

fn live() -> Live {
    Live {
        project: 1,
        workspace: 2,
        running: Vec::new(),
        removing: false,
        role_of_run: false,
        run_roles: 0,
        pull_merged: false,
    }
}

fn ticked_prunable(branch: Option<&str>) -> (Dialog, Action, usize) {
    let mut stale = row("p", "stale", Kind::Prunable);
    stale.found.branch = branch.map(str::to_string);
    let mut list = List::new(vec![stale, row("p", "a", Kind::Linked)]);
    list.scanned(&PathBuf::from("/wt/p/a"), Ok(scan_of(None, false)));
    let mut d = Dialog::new(None, 1, list);
    let prunable = d.list.rows.iter().position(|r| r.found.kind == Kind::Prunable).expect("row");
    d.toggle(prunable);
    let action = d.confirm();
    (d, action, prunable)
}

#[test]
fn a_prunable_rows_branch_is_counted_again_before_removing() {
    let (d, action, prunable) = ticked_prunable(Some("stale"));
    assert_eq!((action, d.can_remove()), (Action::Rescan(vec![prunable]), false));
}

#[test]
fn a_prunable_row_without_a_branch_needs_no_rescan() {
    let (mut d, action, _) = ticked_prunable(None);
    assert_eq!((action, d.can_remove()), (Action::Rescan(vec![]), true));
    assert_eq!(d.key(press(KeyCode::Enter)), Action::Remove { branches: false });
}

#[test]
fn a_stray_or_repeated_answer_does_not_enable_remove() {
    let mut d = dialog();
    d.toggle(0);
    d.toggle(1);
    d.confirm();
    let first = d.list.rows[0].found.path.clone();
    d.rescanned(&first, Ok(scan_of(None, false)));
    d.rescanned(&first, Ok(scan_of(None, false)));
    d.rescanned(&PathBuf::from("/nowhere"), Ok(scan_of(None, false)));
    assert_eq!(d.checking(), 1);
    assert!(!d.can_remove());
    let second = d.list.rows[1].found.path.clone();
    d.rescanned(&second, Ok(scan_of(None, false)));
    assert!(d.can_remove());
}

#[test]
fn remove_is_refused_when_every_ticked_row_went_away() {
    let mut d = dialog();
    d.toggle(0);
    d.confirm();
    let path = d.list.rows[0].found.path.clone();
    d.rescanned(&path, Ok(scan_of(None, false)));
    assert!(d.can_remove());
    d.list.rows[0].gone = true;
    assert!(!d.can_remove());
    assert_eq!(d.key(press(KeyCode::Enter)), Action::None);
}

#[test]
fn confirming_twice_keeps_the_first_confirmation() {
    let mut d = dialog();
    d.toggle(0);
    d.confirm();
    d.toggle(1);
    assert_eq!(d.confirm(), Action::None);
    assert_eq!(d.checking(), 1);
}

#[test]
fn confirm_drops_ticks_on_rows_that_became_untickable() {
    let mut d = dialog();
    d.toggle(0);
    d.toggle(1);
    d.list.rows[0].live = Some(Live { removing: true, ..live() });
    assert_eq!(d.confirm(), Action::Rescan(vec![1]));
    assert!(!d.list.rows[0].ticked);
}

#[test]
fn an_untickable_row_cannot_be_ticked() {
    let mut d = dialog();
    d.list.rows[0].live = Some(Live { role_of_run: true, ..live() });
    d.toggle(0);
    assert!(!d.list.rows[0].ticked);
}

#[test]
fn a_rescan_does_not_reorder() {
    let minute = Duration::from_secs(60);
    let mut list = List::new(vec![row("p", "a", Kind::Linked), row("p", "b", Kind::Linked)]);
    list.scanned(&PathBuf::from("/wt/p/a"), Ok(scan_of(Some(minute), false)));
    assert!(!list.sorted);
    let mut d = Dialog::new(None, 1, list);
    d.list.scanned(&PathBuf::from("/wt/p/b"), Ok(scan_of(Some(2 * minute), false)));
    assert!(d.list.sorted);
    let before: Vec<_> = d.list.rows.iter().map(|r| r.found.path.clone()).collect();
    assert_eq!(before[0], PathBuf::from("/wt/p/b"));
    d.toggle(0);
    d.confirm();
    d.rescanned(&before[0], Ok(scan_of(Some(Duration::ZERO), false)));
    let after: Vec<_> = d.list.rows.iter().map(|r| r.found.path.clone()).collect();
    assert_eq!(before, after);
}

#[test]
fn a_row_that_goes_away_while_checking_does_not_block_remove() {
    let mut d = dialog();
    d.toggle(0);
    d.toggle(1);
    d.confirm();
    let kept = d.list.rows[1].found.path.clone();
    d.rescanned(&kept, Ok(scan_of(None, false)));
    assert_eq!(d.checking(), 1);
    assert!(!d.can_remove());
    d.list.rows[0].gone = true;
    assert_eq!(d.checking(), 0);
    assert!(d.can_remove());
}

#[test]
fn ticking_is_ignored_while_confirming() {
    let mut d = dialog();
    d.toggle(0);
    d.confirm();
    d.toggle(1);
    assert_eq!(d.click(Hit::Tick(1)), Action::None);
    assert!(!d.list.rows[1].ticked);
}

#[test]
fn clicks_map_to_actions_by_step() {
    let mut d = dialog();
    assert_eq!(d.click(Hit::Row(1)), Action::Toggle(1));
    assert_eq!((d.selected, d.list.rows[1].ticked), (Some(1), true));
    assert_eq!(d.click(Hit::Changes(0)), Action::Changes(0));
    assert_eq!(d.click(Hit::Remove), Action::Rescan(vec![1]));
    d.click(Hit::Branches);
    assert!(matches!(d.step, Step::Confirm { branches: true, .. }));
    assert_eq!(d.click(Hit::Back), Action::Back);
    assert_eq!(d.click(Hit::Done), Action::Close);
}

#[test]
fn a_paste_goes_into_the_filter_only_in_the_list() {
    let mut d = dialog();
    d.type_filter("b\n");
    assert_eq!((d.list.filter.as_str(), d.list.shown().len()), ("b", 1));
    d.list.filter.clear();
    d.toggle(0);
    d.confirm();
    d.type_filter("x");
    assert_eq!(d.list.filter, "");
}

#[test]
fn the_selection_stays_in_view() {
    let rows = (0..6).map(|i| row("p", &format!("b{i}"), Kind::Linked)).collect();
    let mut d = Dialog::new(None, 1, List::new(rows));
    for _ in 0..4 {
        d.key(press(KeyCode::Down));
        d.reveal(2);
    }
    assert_eq!((d.selected, d.scroll), (Some(4), 3));
    d.scroll_by(-10, 2, 0);
    assert_eq!(d.scroll, 0);
}

fn with_builds(d: &mut Dialog, rows: &[usize]) {
    for &i in rows {
        let folder = Folder { path: PathBuf::from("node_modules"), bytes: 100 };
        d.list.rows[i].size = Some(Ok(Size { bytes: 200, builds: vec![folder], nested: Vec::new() }));
    }
}

#[test]
fn the_row_button_opens_the_free_step_for_that_row_only_and_keeps_the_ticks() {
    let mut d = dialog();
    with_builds(&mut d, &[0, 1]);
    d.toggle(1);
    assert_eq!(d.click(Hit::Free(0)), Action::Free);
    assert_eq!(d.step, Step::Free { paths: vec![d.list.rows[0].found.path.clone()] });
    assert!(d.list.rows[1].ticked);
    assert_eq!(d.key(press(KeyCode::Esc)), Action::Back);
    assert_eq!(d.step, Step::List);
    assert!(d.list.rows[1].ticked);
}

#[test]
fn a_row_without_builds_does_not_open_the_free_step() {
    let mut d = dialog();
    assert_eq!(d.click(Hit::Free(0)), Action::None);
    assert_eq!(d.step, Step::List);
}

#[test]
fn the_bottom_button_takes_only_the_ticked_rows_that_can_free() {
    let mut d = dialog();
    with_builds(&mut d, &[1]);
    assert_eq!(d.click(Hit::FreeTicked), Action::None);
    d.toggle(0);
    d.toggle(1);
    assert_eq!(d.click(Hit::FreeTicked), Action::Free);
    assert_eq!(d.step, Step::Free { paths: vec![d.list.rows[1].found.path.clone()] });
}

#[test]
fn enter_frees_in_the_free_step_and_only_while_something_can_still_be_freed() {
    let mut d = dialog();
    with_builds(&mut d, &[0]);
    d.click(Hit::Free(0));
    assert_eq!(d.key(press(KeyCode::Enter)), Action::FreeBuilds);
    d.list.rows[0].size = Some(Ok(Size::default()));
    assert_eq!(d.key(press(KeyCode::Enter)), Action::None);
    assert_eq!(d.click(Hit::Back), Action::Back);
}

#[test]
fn a_row_being_freed_cannot_be_ticked() {
    let mut d = dialog();
    d.list.rows[0].freeing = true;
    d.toggle(0);
    assert!(!d.list.rows[0].ticked);
}

#[test]
fn the_order_tabs_change_the_order_and_restart_the_selection() {
    let mut d = dialog();
    d.selected = Some(1);
    assert_eq!(d.click(Hit::Order(Order::Size)), Action::None);
    assert_eq!(d.list.order, Order::Size);
    assert_eq!(d.selected, Some(0));
}
