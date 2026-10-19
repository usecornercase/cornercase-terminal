use super::*;

fn tour(app: &mut App, args: &Value) -> Value {
    ok(app, "tour", args)
}

#[test]
fn list_gives_every_tour_as_data_with_the_progress() {
    let (mut app, _rx) = app();
    let answer = tour(&mut app, &json!({"action": "list"}));
    let tours = answer["tours"].as_array().expect("tours");
    assert_eq!(tours.len(), crate::tour::TOURS.len());
    assert_eq!(tours[0]["steps"][3]["done"], json!({"kind": "prompt-confirmed"}));
    assert_eq!(tours[0]["steps"][3]["offer"], "prompt");
    assert_eq!(answer["state"]["active"], Value::Null);
}

#[test]
fn start_next_back_and_end_move_the_windows_card() {
    let (mut app, _rx) = app();
    let started = tour(&mut app, &json!({"action": "start"}));
    assert_eq!(started["state"]["active"]["step"], "projects");
    assert!(app.tour_card(AREA).is_some());
    let next = tour(&mut app, &json!({"action": "next"}));
    assert_eq!(next["state"]["active"]["step"], "workspaces");
    assert_eq!(next["state"]["progress"][0]["done"], json!(["projects"]));
    let back = tour(&mut app, &json!({"action": "back"}));
    assert_eq!(back["state"]["active"]["index"], 0);
    let ended = tour(&mut app, &json!({"action": "end"}));
    assert_eq!(ended["state"]["active"], Value::Null);
    assert!(app.tour_card(AREA).is_none());
}

#[test]
fn a_ui_of_its_own_moves_the_tour_without_the_card() {
    let (mut app, _rx) = app();
    tour(&mut app, &json!({"action": "start", "tour": "todo", "step": "add", "card": false}));
    assert!(app.tour_card(AREA).is_none());
    let done = tour(&mut app, &json!({"action": "complete", "tour": "todo", "step": "add"}));
    assert_eq!(done["state"]["active"]["step"], "send");
}

#[test]
fn reset_forgets_progress() {
    let (mut app, _rx) = app();
    tour(&mut app, &json!({"action": "complete", "tour": "core", "step": "tabs"}));
    let reset = tour(&mut app, &json!({"action": "reset", "tour": "core"}));
    assert_eq!(reset["state"]["progress"][0]["done"], json!([]));
}

#[test]
fn refusals_name_what_exists() {
    let (mut app, _rx) = app();
    assert_eq!(refused(&mut app, "tour", &json!({"action": "next"})).code, "conflict");
    let unknown = refused(&mut app, "tour", &json!({"action": "start", "tour": "nope"}));
    assert_eq!(unknown.code, "not-found");
    assert!(unknown.message.contains("core"));
    let step = refused(&mut app, "tour", &json!({"action": "start", "step": "nope"}));
    assert_eq!(step.code, "not-found");
    assert_eq!(refused(&mut app, "tour", &json!({"action": "complete"})).code, "malformed");
}

#[test]
fn the_app_feed_carries_the_tour() {
    let (mut app, _rx) = app();
    tour(&mut app, &json!({"action": "start"}));
    let state = app.app_state();
    assert_eq!(state.tour.active.map(|a| a.step), Some("projects".to_string()));
}
