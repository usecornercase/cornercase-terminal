use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::git::worktree::Lock;
use crate::worktrees::{Found, Kind, List, Live, Order, Row, Scan};

pub(super) fn found(project: &str, branch: &str, kind: Kind) -> Found {
    Found {
        repo: PathBuf::from("/repo"),
        project: project.to_string(),
        path: PathBuf::from(format!("/wt/{project}/{branch}")),
        branch: Some(branch.to_string()),
        admin: None,
        kind,
    }
}

pub(super) fn row(project: &str, branch: &str, kind: Kind) -> Row {
    Row {
        found: found(project, branch, kind),
        live: None,
        scan: None,
        size: None,
        outside: Vec::new(),
        elsewhere: Vec::new(),
        ticked: false,
        removing: false,
        freeing: false,
        gone: false,
    }
}

pub(super) fn scan_of(age: Option<Duration>, merged: bool) -> Scan {
    Scan {
        last_changed: age.map(|a| SystemTime::now() - a),
        changed: 0,
        unpushed: 0,
        upstream_gone: false,
        merged,
        has_upstream: true,
        branch: None,
        lock: None,
    }
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

fn order(list: &List) -> Vec<&str> {
    list.rows.iter().filter_map(|r| r.found.branch.as_deref()).collect()
}

const HOUR: Duration = Duration::from_hours(1);
const DAY: Duration = Duration::from_hours(24);

#[test]
fn rows_keep_their_order_until_every_scan_answers() {
    let mut list =
        List::new(vec![row("b", "x", Kind::Linked), row("a", "z", Kind::Linked), row("a", "y", Kind::Linked)]);
    assert_eq!(order(&list), ["y", "z", "x"]);
    assert_eq!(list.pending(), 3);
    list.scanned(&PathBuf::from("/wt/b/x"), Ok(scan_of(Some(HOUR), false)));
    list.scanned(&PathBuf::from("/wt/a/y"), Ok(scan_of(Some(10 * DAY), false)));
    assert_eq!(order(&list), ["y", "z", "x"]);
    assert!(!list.sorted);
    list.scanned(&PathBuf::from("/wt/a/z"), Ok(scan_of(Some(30 * DAY), false)));
    assert!(list.sorted);
    assert_eq!(order(&list), ["z", "y", "x"]);
}

#[test]
fn prunable_then_merged_then_oldest_first() {
    let mut list = List::new(vec![
        row("p", "recent", Kind::Linked),
        row("p", "old", Kind::Linked),
        row("p", "merged", Kind::Linked),
        row("p", "stale", Kind::Prunable),
    ]);
    list.scanned(&PathBuf::from("/wt/p/recent"), Ok(scan_of(Some(2 * DAY), false)));
    list.scanned(&PathBuf::from("/wt/p/old"), Ok(scan_of(Some(30 * DAY), false)));
    list.scanned(&PathBuf::from("/wt/p/merged"), Ok(scan_of(Some(HOUR), true)));
    assert_eq!(order(&list), ["stale", "merged", "old", "recent"]);
}

#[test]
fn a_merged_pull_request_counts_as_merged() {
    let mut r = row("p", "a", Kind::Linked);
    r.live = Some(Live { pull_merged: true, ..live() });
    r.scan = Some(Ok(scan_of(Some(HOUR), false)));
    assert!(r.merged());
    assert_eq!(r.band(), 1);
}

#[test]
fn a_branch_in_its_base_without_an_upstream_has_no_commits_and_sorts_with_the_rest() {
    let mut r = row("p", "fresh", Kind::Linked);
    r.scan = Some(Ok(Scan { has_upstream: false, ..scan_of(Some(HOUR), true) }));
    assert_eq!((r.merged(), r.no_commits(), r.band()), (false, true, 2));
    r.live = Some(Live { pull_merged: true, ..live() });
    assert_eq!((r.merged(), r.no_commits(), r.band()), (true, false, 1));
}

#[test]
fn the_branch_is_the_one_the_last_scan_saw() {
    let mut r = row("p", "listed", Kind::Linked);
    assert_eq!(r.branch(), Some("listed"));
    r.scan = Some(Ok(Scan { branch: Some("switched".into()), ..scan_of(None, false) }));
    assert_eq!(r.branch(), Some("switched"));
    r.scan = Some(Ok(Scan { branch: None, ..scan_of(None, false) }));
    assert_eq!(r.branch(), None);
    r.scan = Some(Err("boom".into()));
    assert_eq!(r.branch(), Some("listed"));
}

#[test]
fn a_process_in_another_cornercase_tab_is_risky() {
    let mut r = row("p", "a", Kind::Linked);
    r.scan = Some(Ok(scan_of(None, false)));
    r.elsewhere = vec!["zsh".into()];
    assert!(r.risky());
}

#[test]
fn unknown_dates_go_last_in_their_band() {
    let mut list = List::new(vec![
        row("p", "none", Kind::Linked),
        row("p", "dated", Kind::Linked),
        row("p", "failed", Kind::Linked),
    ]);
    list.scanned(&PathBuf::from("/wt/p/none"), Ok(scan_of(None, false)));
    list.scanned(&PathBuf::from("/wt/p/dated"), Ok(scan_of(Some(HOUR), false)));
    list.scanned(&PathBuf::from("/wt/p/failed"), Err("boom".into()));
    assert_eq!(order(&list), ["dated", "failed", "none"]);
}

#[test]
fn sorting_happens_once() {
    let mut list = List::new(vec![row("p", "a", Kind::Linked), row("p", "b", Kind::Linked)]);
    list.scanned(&PathBuf::from("/wt/p/a"), Ok(scan_of(Some(HOUR), false)));
    list.scanned(&PathBuf::from("/wt/p/b"), Ok(scan_of(Some(DAY), false)));
    assert_eq!(order(&list), ["b", "a"]);
    list.scanned(&PathBuf::from("/wt/p/a"), Ok(scan_of(Some(30 * DAY), false)));
    assert_eq!(order(&list), ["b", "a"]);
}

#[test]
fn filter_matches_project_or_branch_ignoring_case() {
    let mut list = List::new(vec![
        row("Shop", "feat-a", Kind::Prunable),
        row("api", "FIX-b", Kind::Prunable),
        row("api", "c", Kind::Prunable),
    ]);
    list.filter = "shop".into();
    assert_eq!(list.shown().len(), 1);
    list.filter = "fix".into();
    assert_eq!(list.shown().len(), 1);
    list.filter = "API".into();
    assert_eq!(list.shown().len(), 2);
    list.rows[0].gone = true;
    list.filter.clear();
    assert_eq!(list.shown().len(), 2);
}

#[test]
fn role_worktrees_of_a_live_run_and_rows_being_removed_cannot_be_ticked() {
    let mut r = row("p", "a", Kind::Linked);
    assert!(r.tickable());
    r.live = Some(Live { role_of_run: true, ..live() });
    assert!(!r.tickable());
    r.live = Some(Live { removing: true, ..live() });
    assert!(!r.tickable());
    r.live = Some(live());
    r.gone = true;
    assert!(!r.tickable());
}

#[test]
fn a_failed_scan_is_risky() {
    let mut r = row("p", "a", Kind::Linked);
    assert!(r.risky());
    r.scan = Some(Err("boom".into()));
    assert!(r.risky());
}

#[test]
fn a_clean_unlocked_idle_worktree_is_not_risky() {
    let mut r = row("p", "a", Kind::Linked);
    r.scan = Some(Ok(scan_of(Some(HOUR), false)));
    assert!(!r.risky());
    r.scan = Some(Ok(Scan { unpushed: 3, ..scan_of(Some(HOUR), false) }));
    assert!(!r.risky());
    r.scan = Some(Ok(Scan { changed: 1, ..scan_of(Some(HOUR), false) }));
    assert!(r.risky());
    r.scan =
        Some(Ok(Scan { lock: Some(Lock { reason: "x".into(), holder: None, role: None }), ..scan_of(None, false) }));
    assert!(r.risky());
    r.scan = Some(Ok(scan_of(None, false)));
    r.outside = vec!["vim".into()];
    assert!(r.risky());
    r.outside.clear();
    r.live = Some(Live { running: vec!["claude".into()], ..live() });
    assert!(r.risky());
}

fn size_of(bytes: u64, builds: u64) -> crate::worktrees::builds::Size {
    use crate::worktrees::builds::{Folder, Size};
    let folder = Folder { path: PathBuf::from("node_modules"), bytes: builds };
    Size { bytes, builds: if builds > 0 { vec![folder] } else { Vec::new() }, nested: Vec::new() }
}

fn sized_list() -> List {
    let mut list =
        List::new(vec![row("p", "a", Kind::Linked), row("p", "b", Kind::Linked), row("p", "c", Kind::Linked)]);
    for (branch, age) in [("a", 30), ("b", 20), ("c", 1)] {
        list.scanned(&PathBuf::from(format!("/wt/p/{branch}")), Ok(scan_of(Some(age * DAY), false)));
    }
    list
}

#[test]
fn the_size_order_waits_for_every_size_then_sorts_largest_first_with_unknown_last() {
    let mut list = sized_list();
    list.order_by(Order::Size);
    assert_eq!(order(&list), ["a", "b", "c"]);
    list.sized(&PathBuf::from("/wt/p/a"), Ok(size_of(10, 0)));
    list.sized(&PathBuf::from("/wt/p/b"), Err("gave up".into()));
    assert_eq!(order(&list), ["a", "b", "c"]);
    list.sized(&PathBuf::from("/wt/p/c"), Ok(size_of(500, 400)));
    assert_eq!(order(&list), ["c", "a", "b"]);
    list.sized(&PathBuf::from("/wt/p/a"), Ok(size_of(900, 0)));
    assert_eq!(order(&list), ["c", "a", "b"]);
}

#[test]
fn going_back_to_the_safe_order_sorts_by_age_again() {
    let mut list = sized_list();
    for (branch, bytes) in [("a", 10), ("b", 30), ("c", 20)] {
        list.sized(&PathBuf::from(format!("/wt/p/{branch}")), Ok(size_of(bytes, 0)));
    }
    list.order_by(Order::Size);
    assert_eq!(order(&list), ["b", "c", "a"]);
    list.order_by(Order::Safe);
    assert_eq!(order(&list), ["a", "b", "c"]);
}

#[test]
fn the_total_counts_every_known_size_and_idle_builds_start_at_seven_days() {
    let mut list = sized_list();
    list.rows[0].scan = Some(Ok(scan_of(Some(7 * DAY + HOUR), false)));
    list.rows[1].scan = Some(Ok(scan_of(Some(6 * DAY + 23 * HOUR), false)));
    list.rows[2].scan = Some(Ok(scan_of(Some(DAY), false)));
    list.sized(&PathBuf::from("/wt/p/a"), Ok(size_of(1000, 600)));
    list.sized(&PathBuf::from("/wt/p/b"), Ok(size_of(2000, 900)));
    assert_eq!(list.total(), (3000, 2));
    assert_eq!(list.idle_builds(SystemTime::now()), (600, 1));
    list.rows[1].gone = true;
    assert_eq!(list.total(), (1000, 1));
}

#[test]
fn only_a_linked_row_with_builds_that_is_tickable_can_free() {
    let mut r = row("p", "a", Kind::Linked);
    assert!(!r.can_free());
    r.size = Some(Ok(size_of(100, 0)));
    assert!(!r.can_free());
    r.size = Some(Ok(size_of(100, 50)));
    assert!(r.can_free());
    r.freeing = true;
    assert!(!r.can_free() && !r.tickable());
    r.freeing = false;
    r.found.kind = Kind::Orphan;
    assert!(!r.can_free());
}
