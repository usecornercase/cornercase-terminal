use super::*;
use crate::control::tour::{ActiveInfo, ProgressInfo, TourState, tours};

#[test]
fn a_move_prints_where_the_tour_is() {
    let state = TourState {
        active: Some(ActiveInfo { tour: "core".into(), step: "tabs".into(), index: 2, card: true }),
        ..TourState::default()
    };
    assert_eq!(text(&TourAnswer { state, tours: Vec::new() }, false), "on screen: core step 3 (tabs)\n");
}

#[test]
fn the_list_marks_what_was_done() {
    let state = TourState {
        progress: vec![ProgressInfo { tour: "core".into(), done: vec!["projects".into()], finished: false }],
        ..TourState::default()
    };
    let printed = text(&TourAnswer { state, tours: tours() }, true);
    assert!(
        printed.starts_with("no tour on screen\ncore  Get started 1/7  https://usecornercase.dev/docs/first-steps/\n")
    );
    assert!(printed.contains("  ✓ projects  Projects\n  · workspaces  Workspaces\n"));
}

#[test]
fn no_action_starts_the_core_tour() {
    assert_eq!(request(None).action, TourAction::Start);
    assert_eq!(request(Some(&TourCommand::Reset { tour: None })).action, TourAction::Reset);
}
