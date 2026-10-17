use rstest::rstest;

use super::*;
use crate::control::Layout;
use crate::control::terminal::Moved;
use crate::project::Tab;
use crate::project::split::{Dir, Node};

fn split(app: &mut App, pane: u64) -> u64 {
    done(Response::Ok(ok(app, "split", &json!({"pane": pane})))).ids.pane.expect("the new pane")
}

fn split_down(app: &mut App, pane: u64) -> u64 {
    done(Response::Ok(ok(app, "split", &json!({"pane": pane, "down": true})))).ids.pane.expect("the new pane")
}

fn tab_of(app: &App, pane: u64) -> &Tab {
    app.projects.tab_with_pane(pane).expect("the tab").1
}

fn leaves(app: &App, pane: u64) -> Vec<u64> {
    tab_of(app, pane).layout.ids()
}

fn moved(app: &mut App, pane: u64, target: u64, place: &str) -> Moved {
    serde_json::from_value(ok(app, "move-pane", &json!({"pane": pane, "target": target, "place": place})))
        .expect("an answer")
}

#[test]
fn a_pane_swaps_with_another_and_both_keep_running() {
    let (mut app, _rx) = app();
    let one = first(&app);
    let two = split(&mut app, one);
    let ptys: Vec<u64> = tab_of(&app, one).panes.iter().map(|t| t.id).collect();
    assert_eq!(leaves(&app, one), [one, two]);

    let answer = moved(&mut app, one, two, "swap");

    assert_eq!(leaves(&app, one), [two, one]);
    assert_eq!(tab_of(&app, one).pane().expect("a pane").id, one);
    assert_eq!(tab_of(&app, one).panes.iter().map(|t| t.id).collect::<Vec<_>>(), ptys);
    assert_eq!(answer.tab, tab_of(&app, one).id);
    assert_eq!(answer.layout, Layout::from(&tab_of(&app, one).layout));
}

#[test]
fn a_pane_moved_to_an_edge_splits_the_target_there_and_the_terminals_follow() {
    let (mut app, _rx) = app();
    let one = first(&app);
    let two = split(&mut app, one);
    let three = split_down(&mut app, two);
    app.resize(AREA);
    let before = pane(&app, three).emulator.size().expect("a size");

    moved(&mut app, three, one, "above");

    let tab = tab_of(&app, one);
    let Node::Split { first: left, .. } = &tab.layout else { panic!("not a split: {:?}", tab.layout) };
    let Node::Split { dir, .. } = left.as_ref() else { panic!("not a split: {left:?}") };
    assert_eq!((*dir, tab.layout.ids()), (Dir::Down, vec![three, one, two]));
    assert_ne!(pane(&app, three).emulator.size().expect("a size"), before);
    assert_eq!(tab.pane().expect("a pane").id, three);
}

#[test]
fn a_move_without_room_is_refused_and_changes_nothing() {
    let (mut app, _rx) = app();
    let one = first(&app);
    let two = split(&mut app, one);
    let tab = tab_of(&app, one).id;
    ok(&mut app, "view", &json!({"cols": 40, "rows": 6, "tab": tab}));
    let before = tab_of(&app, one).layout.clone();

    let refused = refused(&mut app, "move-pane", &json!({"pane": one, "target": two, "place": "below"}));

    assert_eq!(refused.code, "conflict", "{}", refused.message);
    assert!(refused.message.contains("40×6"), "{}", refused.message);
    assert_eq!(tab_of(&app, one).layout, before);
    ok(&mut app, "move-pane", &json!({"pane": one, "target": two, "place": "swap"}));
    assert_eq!(leaves(&app, one), [two, one]);
}

#[test]
fn panes_of_two_tabs_do_not_move() {
    let (mut app, _rx) = app();
    let one = first(&app);
    let other = done(Response::Ok(ok(&mut app, "new-tab", &json!({})))).ids.pane.expect("a pane");

    let refused = refused(&mut app, "move-pane", &json!({"pane": one, "target": other, "place": "left"}));

    assert_eq!(refused.code, "conflict");
}

#[rstest]
#[case::onto_itself(|one, _| json!({"pane": one, "target": one, "place": "swap"}), "malformed")]
#[case::no_place(|one, two| json!({"pane": one, "target": two}), "malformed")]
#[case::no_target(|one, _| json!({"pane": one, "place": "swap"}), "malformed")]
#[case::no_pane(|_, two| json!({"target": two, "place": "swap"}), "malformed")]
#[case::an_unknown_pane(|_, two| json!({"pane": 999, "target": two, "place": "swap"}), "not-found")]
#[case::an_unknown_target(|one, _| json!({"pane": one, "target": 999, "place": "swap"}), "not-found")]
#[case::a_place_that_is_none(|one, two| json!({"pane": one, "target": two, "place": "middle"}), "malformed")]
fn a_pane_move_that_makes_no_sense_is_refused(#[case] build: fn(u64, u64) -> Value, #[case] code: &str) {
    let (mut app, _rx) = app();
    let one = first(&app);
    let two = split(&mut app, one);
    let before = leaves(&app, one);

    let refused = refused(&mut app, "move-pane", &build(one, two));

    assert_eq!(refused.code, code, "{}", refused.message);
    assert_eq!(leaves(&app, one), before);
}
