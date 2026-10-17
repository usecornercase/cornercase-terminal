use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use rstest::rstest;

use super::*;
use crate::project::{Phase, Tab, Workspace};
use crate::term::Event;
use crate::test_util::spawn_term;

struct World {
    projects: Vec<Project>,
    _terms: Vec<Receiver<Event>>,
}

fn world(shape: &[&[usize]]) -> World {
    let (mut next, mut terms) = (1000, Vec::new());
    let mut projects = Vec::new();
    for (p, workspaces) in shape.iter().enumerate() {
        let mut project = Project::new(100 + p as u64, PathBuf::from("/tmp"), None);
        for (w, tabs) in workspaces.iter().enumerate() {
            let mut workspace = Workspace::new(200 + (p * 10 + w) as u64, PathBuf::from("/tmp"), None, false);
            for t in 0..*tabs {
                let (term, rx) = spawn_term(next, &[], None);
                terms.push(rx);
                workspace.tabs.push(Tab::new(300 + (p * 100 + w * 10 + t) as u64, None, term));
                next += 1;
            }
            project.workspaces.push(workspace);
        }
        projects.push(project);
    }
    World { projects, _terms: terms }
}

fn none() -> Shown {
    Shown::default()
}

fn ids(shown: &Shown) -> (Option<u64>, Option<u64>, Option<u64>) {
    (shown.project, shown.workspace, shown.tab)
}

#[test]
fn placing_descends_into_the_active_children() {
    let mut w = world(&[&[2, 3]]);
    w.projects[0].active = 1;
    w.projects[0].workspaces[1].active = 2;

    let shown = Shown::place(&w.projects, 0, None, None);

    assert_eq!(ids(&shown), (Some(100), Some(201), Some(312)));
}

#[test]
fn placing_on_a_workspace_without_tabs_shows_no_tab() {
    let w = world(&[&[0]]);

    assert_eq!(ids(&Shown::place(&w.projects, 0, Some(0), None)), (Some(100), Some(200), None));
}

#[rstest]
#[case::the_middle(1, 0)]
#[case::the_first(0, 0)]
#[case::the_last(2, 1)]
fn a_closed_tab_falls_to_the_one_before_it(#[case] closed: usize, #[case] expected: usize) {
    let mut w = world(&[&[3]]);
    let mut shown = Shown::place(&w.projects, 0, Some(0), Some(closed));
    w.projects[0].workspaces[0].tabs.remove(closed);

    let changed = shown.settle(&w.projects, none);

    let tabs = &w.projects[0].workspaces[0].tabs;
    assert!(changed);
    assert_eq!(shown.tab, Some(tabs[expected].id));
}

#[test]
fn a_moved_tab_keeps_the_selection_by_its_id() {
    let mut w = world(&[&[3]]);
    let mut shown = Shown::place(&w.projects, 0, Some(0), Some(2));
    let tab = w.projects[0].workspaces[0].tabs.remove(2);
    w.projects[0].workspaces[0].tabs.insert(0, tab);

    assert!(!shown.settle(&w.projects, none));
    assert_eq!(shown.tab, Some(w.projects[0].workspaces[0].tabs[0].id));
}

#[rstest]
#[case::closing_goes_to_the_one_before(2, 1)]
#[case::the_first_goes_to_the_next(0, 1)]
fn a_workspace_that_is_going_falls_to_the_nearest_open_one(#[case] going: usize, #[case] expected: usize) {
    let mut w = world(&[&[1, 1, 1]]);
    let mut shown = Shown::place(&w.projects, 0, Some(going), None);
    w.projects[0].workspaces[going].phase = Phase::Closing;

    assert!(shown.settle(&w.projects, none));

    assert_eq!(shown.workspace, Some(w.projects[0].workspaces[expected].id));
    assert_eq!(shown.tab, Some(w.projects[0].workspaces[expected].tabs[0].id));
}

#[test]
fn a_workspace_being_removed_is_left_even_when_it_is_gone() {
    let mut w = world(&[&[1, 1]]);
    let mut shown = Shown::place(&w.projects, 0, Some(1), None);
    w.projects[0].workspaces.remove(1);

    assert!(shown.settle(&w.projects, none));

    assert_eq!(shown.workspace, Some(200));
}

#[rstest]
#[case::the_previous_project(1, 0)]
#[case::the_next_when_the_first_closes(0, 1)]
fn a_closing_project_falls_to_a_neighbour(#[case] closing: usize, #[case] expected: usize) {
    let mut w = world(&[&[1], &[1]]);
    let mut shown = Shown::place(&w.projects, closing, None, None);
    w.projects[closing].closing = true;

    assert!(shown.settle(&w.projects, none));

    assert_eq!(shown.project, Some(w.projects[expected].id));
    assert_eq!(shown.tab, Some(w.projects[expected].workspaces[0].tabs[0].id));
}

#[test]
fn nothing_left_gives_an_empty_selection_that_adopts_the_window_later() {
    let mut w = world(&[&[1]]);
    let mut shown = Shown::place(&w.projects, 0, None, None);
    w.projects[0].closing = true;

    assert!(shown.settle(&w.projects, none));
    assert!(shown.is_empty());
    let window = Shown::place(&world(&[&[1]]).projects, 0, None, None);
    assert!(shown.settle(&w.projects, || window));
    assert_eq!(shown, window);
}

#[test]
fn an_unchanged_selection_is_not_a_change() {
    let w = world(&[&[2, 2], &[1]]);
    let mut shown = Shown::place(&w.projects, 0, Some(1), Some(1));

    assert!(!shown.settle(&w.projects, none));
}

#[test]
fn a_tab_that_appears_in_an_empty_workspace_is_adopted() {
    let mut w = world(&[&[0]]);
    let mut shown = Shown::place(&w.projects, 0, Some(0), None);
    let extra = world(&[&[1]]);
    w.projects[0].workspaces[0].tabs = extra.projects[0].workspaces[0].tabs.iter().map(|_| unreachable_tab()).collect();

    assert!(shown.settle(&w.projects, none));
    assert!(shown.tab.is_some());
}

fn unreachable_tab() -> Tab {
    let (term, rx) = spawn_term(9999, &[], None);
    std::mem::forget(rx);
    Tab::new(9999, None, term)
}

#[rstest]
#[case::the_window_by_default(None, &[], None)]
#[case::the_latest_view(None, &[(1, 5, 2), (2, 5, 3)], Some(2))]
#[case::a_view_on_another_tab_is_ignored(None, &[(1, 6, 9)], None)]
#[case::the_window_claim_beats_an_older_view(Some((5, 4)), &[(1, 5, 2), (2, 5, 3)], None)]
#[case::a_later_view_beats_the_window(Some((5, 4)), &[(1, 5, 2), (2, 5, 7)], Some(2))]
#[case::a_window_claim_elsewhere_does_not_count(Some((6, 99)), &[(1, 5, 2)], Some(1))]
#[case::no_claim_is_the_window(Some((5, 0)), &[(1, 5, 0)], None)]
fn one_controller_per_tab(
    #[case] window: Option<(u64, u64)>,
    #[case] claims: &[(u64, u64, u64)],
    #[case] expected: Option<u64>,
) {
    let claims = claims.iter().map(|(view, tab, seq)| Claim { view: *view, tab: *tab, seq: *seq });

    assert_eq!(controller(5, window, claims), expected);
}
