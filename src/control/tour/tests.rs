use super::*;

#[test]
fn every_step_is_data_with_its_target_and_completion() {
    let listed = tours();
    assert_eq!(listed.len(), TOURS.len());
    let first = &listed[0];
    assert_eq!(first.id, tour::CORE);
    let value = serde_json::to_value(&first.steps[0]).expect("json");
    assert_eq!(value["target"], "new-project");
    assert_eq!(value["done"], serde_json::json!({"kind": "project-opened"}));
    assert!(first.docs.starts_with(tour::DOCS));
}

#[test]
fn the_request_and_the_state_keep_their_shape() {
    let request: TourRequest =
        serde_json::from_str(crate::test_util::fixture!("control/tour/v1/request.json")).expect("request");
    assert_eq!(request.action, TourAction::Start);
    assert_eq!(request.tour.as_deref(), Some("todo"));
    let text = crate::test_util::fixture!("control/tour/v1/state.json");
    let state: TourState = serde_json::from_str(text).expect("state");
    let written = serde_json::to_string_pretty(&state).expect("json");
    assert_eq!(written.trim_end(), text.trim_end());
}

#[test]
fn unknown_tours_and_steps_name_the_known_ones() {
    assert!(find(Some("nope")).expect_err("unknown").contains("core"));
    let (_, core) = find(None).expect("core");
    assert!(step(core, "nope").expect_err("unknown").contains("projects"));
}

#[test]
fn the_state_lists_every_tour() {
    let state = TourState::of(&Saved::default(), None);
    assert_eq!(state.progress.len(), TOURS.len());
}
