use rstest::rstest;

use super::*;

fn pane(id: u64, parent: Option<u64>, workspace: u64, tab: u64) -> Pane {
    Pane { id, parent, workspace, tab, agent: true }
}

#[test]
fn an_agent_alone_closes_its_tab() {
    let panes = [pane(1, None, 10, 100), pane(2, None, 10, 101)];
    let closing = closing(&panes, 1, false).expect("known agent");
    assert_eq!(closing, Closing { tabs: vec![100], inside: vec![], elsewhere: vec![] });
}

#[test]
fn children_in_the_workspace_close_with_their_tabs() {
    let mut shell = pane(3, Some(1), 10, 102);
    shell.agent = false;
    let panes = [pane(1, None, 10, 100), pane(2, Some(1), 10, 101), shell, pane(4, None, 10, 103)];
    let closing = closing(&panes, 1, false).expect("known agent");
    assert_eq!(closing.tabs, vec![100, 101, 102]);
    assert_eq!(closing.inside, vec![2, 3]);
    assert_eq!(closing.elsewhere, Vec::<u64>::new());
}

#[test]
fn children_elsewhere_keep_running() {
    let panes = [pane(1, None, 10, 100), pane(2, Some(1), 20, 200)];
    let closing = closing(&panes, 1, false).expect("known agent");
    assert_eq!(closing.tabs, vec![100]);
    assert_eq!(closing.inside, Vec::<u64>::new());
    assert_eq!(closing.elsewhere, vec![2]);
}

#[test]
fn a_grandchild_inside_through_a_child_elsewhere_closes() {
    let panes = [pane(1, None, 10, 100), pane(2, Some(1), 20, 200), pane(3, Some(2), 10, 101)];
    let closing = closing(&panes, 1, false).expect("known agent");
    assert_eq!(closing.tabs, vec![100, 101]);
    assert_eq!(closing.inside, vec![3]);
    assert_eq!(closing.elsewhere, vec![2]);
}

#[rstest]
#[case::from_the_root(1)]
#[case::from_a_child(2)]
fn the_whole_workspace_takes_every_tab(#[case] agent: u64) {
    let panes = [pane(1, None, 10, 100), pane(2, Some(1), 10, 101), pane(3, None, 10, 100), pane(4, None, 20, 200)];
    let closing = closing(&panes, agent, true).expect("known agent");
    assert_eq!(closing.tabs, vec![100, 101]);
}

#[test]
fn a_cycle_of_parents_ends() {
    let panes =
        [pane(1, Some(3), 10, 100), pane(2, Some(1), 10, 101), pane(3, Some(2), 10, 102), pane(4, Some(4), 10, 103)];
    let closing = closing(&panes, 1, false).expect("known agent");
    assert_eq!(closing.inside, vec![2, 3]);
    assert_eq!(closing.tabs, vec![100, 101, 102]);
}

#[test]
fn an_unknown_pane_is_none() {
    assert_eq!(closing(&[pane(1, None, 10, 100)], 9, false), None);
}
