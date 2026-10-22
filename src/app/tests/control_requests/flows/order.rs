use serde_json::{Value, json};

use super::*;
use crate::control::{FlowDefaultSet, FlowOrdered, FlowSaved, FlowSpec, Flows};

fn listed(team: &mut Team) -> Flows {
    serde_json::from_value(ok(&mut team.app, "flow-list", &json!({}))).expect("flows")
}

fn defaulted(team: &mut Team, name: &Value) -> FlowDefaultSet {
    serde_json::from_value(ok(&mut team.app, "flow-default", &json!({"name": name}))).expect("a default")
}

fn ordered(team: &mut Team, names: &[&str]) -> FlowOrdered {
    serde_json::from_value(ok(&mut team.app, "flow-order", &json!({"names": names}))).expect("an order")
}

fn names(flows: &Flows) -> Vec<&str> {
    flows.flows.iter().map(|flow| flow.name.as_str()).collect()
}

fn saved(team: &mut Team, name: &str, path: Option<&Path>) -> FlowSaved {
    let flow = Flow { name: name.into(), ..pair() };
    let args =
        json!({"flow": serde_json::to_value(FlowSpec::from(&flow)).expect("a spec"), "place": "yours", "path": path});
    serde_json::from_value(ok(&mut team.app, "flow-save", &args)).expect("a save")
}

fn saved_config(team: &Team) -> Config {
    crate::config::load(&team.config.path().join("config.json"))
}

#[test]
fn flow_default_sets_marks_and_clears_and_says_when_nothing_changed() {
    let mut team = Team::new();

    let set = defaulted(&mut team, &json!(" quick "));
    let again = defaulted(&mut team, &json!("Quick"));

    assert_eq!((set.name.as_deref(), set.changed), (Some("Quick"), true));
    assert!(!again.changed);
    assert_eq!(saved_config(&team).default_flow, "Quick");
    let flows = listed(&mut team);
    let marked: Vec<&str> = flows.flows.iter().filter(|flow| flow.default).map(|flow| flow.name.as_str()).collect();
    assert_eq!(marked, ["Quick"]);

    let cleared = defaulted(&mut team, &Value::Null);
    let nothing = defaulted(&mut team, &Value::Null);

    assert!(cleared.changed && cleared.name.is_none());
    assert!(!nothing.changed);
    assert!(listed(&mut team).flows.iter().all(|flow| !flow.default));
}

#[test]
fn flow_default_refuses_an_unknown_a_deleted_or_a_blank_name() {
    let mut team = Team::new();
    ok(&mut team.app, "flow-hide", &json!({"name": "Quick"}));

    let unknown = refused(&mut team.app, "flow-default", &json!({"name": "Nope"}));
    let deleted = refused(&mut team.app, "flow-default", &json!({"name": "Quick"}));
    let blank = refused(&mut team.app, "flow-default", &json!({"name": "  "}));

    assert_eq!(
        (unknown.code.as_str(), deleted.code.as_str(), blank.code.as_str()),
        ("not-found", "not-found", "malformed")
    );
    assert!(unknown.message.contains("the flows are"), "{}", unknown.message);
    assert!(deleted.message.contains("was deleted"), "{}", deleted.message);
    assert_eq!(team.app.config.default_flow, "");
}

#[test]
fn flow_order_orders_the_list_and_says_when_nothing_changed() {
    let mut team = Team::new();
    let before = names(&listed(&mut team)).iter().map(ToString::to_string).collect::<Vec<_>>();
    let last = before.last().expect("flows").clone();

    let first = ordered(&mut team, &[&last.to_lowercase()]);
    let again = ordered(&mut team, &[&last]);

    assert!(first.changed && !again.changed);
    assert_eq!(first.names[0], last);
    assert_eq!(first.names.len(), before.len());
    assert_eq!(names(&listed(&mut team))[0], last);
    assert_eq!(saved_config(&team).flow_order, std::slice::from_ref(&last));

    let cleared: FlowOrdered =
        serde_json::from_value(ok(&mut team.app, "flow-order", &json!({"names": []}))).expect("an order");
    assert!(cleared.changed);
    assert_eq!(cleared.names, before);
    assert_eq!(team.app.config.flow_order, Vec::<String>::new());
}

#[test]
fn flow_order_refuses_unknown_and_repeated_names() {
    let mut team = Team::new();

    let unknown = refused(&mut team.app, "flow-order", &json!({"names": ["Quick", "Nope"]}));
    let repeated = refused(&mut team.app, "flow-order", &json!({"names": ["Quick", "quick"]}));
    let blank = refused(&mut team.app, "flow-order", &json!({"names": [" "]}));

    assert_eq!(unknown.code, "not-found");
    assert!(unknown.message.contains("`Nope`"), "{}", unknown.message);
    assert_eq!((repeated.code.as_str(), blank.code.as_str()), ("malformed", "malformed"));
    assert_eq!(team.app.config.flow_order, Vec::<String>::new());
}

#[test]
fn an_open_flows_dialog_follows_both_requests() {
    let mut team = Team::new();
    team.app.open_flows(None, None);
    defaulted(&mut team, &json!("Quick"));
    ordered(&mut team, &["Quick"]);
    let Some(Overlay::Flows { dialog, .. }) = &team.app.overlay else { panic!("no flows dialog") };
    assert_eq!(dialog.default.as_deref(), Some("Quick"));
    assert_eq!(dialog.flows[0].flow.name, "Quick");
}

#[test]
fn a_rename_through_flow_save_moves_the_default_and_its_place() {
    let mut team = Team::new();
    let path = saved(&mut team, "Mine", None).path.expect("a path");
    defaulted(&mut team, &json!("Mine"));
    ordered(&mut team, &["Mine"]);

    saved(&mut team, "Mine renamed", Some(&path));

    let config = saved_config(&team);
    assert_eq!(
        (config.default_flow.as_str(), config.flow_order.as_slice()),
        ("Mine renamed", ["Mine renamed".to_string()].as_slice())
    );
    let flows = listed(&mut team);
    assert!(flows.flows.iter().any(|flow| flow.name == "Mine renamed" && flow.default && flow.origin == "user"));
}

#[test]
fn deleting_the_default_flow_clears_it() {
    let mut team = Team::new();
    let path = saved(&mut team, "Mine", None).path.expect("a path");
    defaulted(&mut team, &json!("Mine"));

    ok(&mut team.app, "flow-delete", &json!({"path": path}));

    assert_eq!(saved_config(&team).default_flow, "");
}
