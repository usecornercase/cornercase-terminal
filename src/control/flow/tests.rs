use super::*;
use crate::agents::Known;
use crate::control::contract::unknown_fields;
use crate::control::{Command, Request};
use crate::flows::{ORCHESTRATOR, builtin};
use crate::test_util::fixture;

fn role(name: &str, model: Option<&str>, edits: bool, prompt: &str) -> Role {
    Role {
        name: name.into(),
        agent: "claude".into(),
        model: model.map(str::to_string),
        edits,
        prompt: prompt.into(),
        ..Role::default()
    }
}

fn example() -> Flow {
    let coder = Role {
        effort: Some("high".into()),
        args: vec!["--verbose".into()],
        env: BTreeMap::from([("NODE_ENV".to_string(), "test".to_string())]),
        worktree: Worktree::Own,
        ..role("coder", None, true, "Write it.")
    };
    Flow {
        name: "Pair".into(),
        notes: "Keep it small.".into(),
        orchestrator: role(ORCHESTRATOR, Some("opus"), false, "Be brief."),
        roles: vec![coder, role("reviewer", Some("sonnet"), false, "Review it.")],
        stages: vec![
            Stage { roles: vec!["coder".into()], back: None },
            Stage { roles: vec!["reviewer".into()], back: Some(Back { to: 1, when: "findings".into(), at_most: 2 }) },
        ],
        ..Flow::default()
    }
}

fn back_and_forth(flow: &Flow) -> Flow {
    let text = serde_json::to_string(&FlowSpec::from(flow)).expect("json");
    serde_json::from_str::<FlowSpec>(&text).expect("a spec").into_flow()
}

#[test]
fn a_flow_survives_json_and_back() {
    let mut every: Vec<Flow> = builtin::flows(&Known::ALL, Some(Known::Claude));
    every.push(example());
    for flow in every {
        assert_eq!(back_and_forth(&flow), flow, "{}", flow.name);
    }
}

#[test]
fn a_spec_names_the_orchestrator_whatever_it_says() {
    let mut spec = FlowSpec::from(&example());
    spec.orchestrator.name = "boss".into();
    assert_eq!(spec.into_flow().orchestrator.name, ORCHESTRATOR);
}

#[test]
fn the_flow_json_is_pinned() {
    let written = serde_json::to_string_pretty(&FlowSpec::from(&example())).expect("json");
    assert_eq!(written, fixture!("control/flows/flow.json").trim_end());
    let read: FlowSpec = serde_json::from_str(fixture!("control/flows/flow.json")).expect("a spec");
    assert_eq!(read.into_flow(), example());
}

#[test]
fn every_key_of_a_spec_is_written() {
    let value = serde_json::to_value(FlowSpec::default()).expect("json");
    let mut keys: Vec<&str> = value.as_object().expect("an object").keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["description", "end", "name", "notes", "orchestrator", "roles", "stages"]);
    let role = serde_json::to_value(RoleSpec::default()).expect("json");
    assert_eq!(role.as_object().expect("an object").len(), 9);
}

fn request(flow: &Flow, extra: &str) -> String {
    let flow = serde_json::to_string(&FlowSpec::from(flow)).expect("json");
    format!(
        r#"{{"contract":1,"command":"flow-save","args":{{"flow":{flow},"place":"yours","path":null,"check":true{extra}}}}}"#
    )
}

#[test]
fn a_save_request_with_the_whole_flow_has_no_unknown_field() {
    let text = request(&example(), "");
    let raw: serde_json::Value = serde_json::from_str(&text).expect("json");
    let parsed: Request = serde_json::from_value(raw.clone()).expect("a request");
    assert!(matches!(parsed.command, Command::FlowSave(_)));
    assert_eq!(unknown_fields(&raw, &parsed), None);
}

#[test]
fn a_stray_key_inside_a_role_is_reported_with_its_path() {
    let text = request(&example(), "").replace(r#""name":"coder""#, r#""name":"coder","bogus":1"#);
    let raw: serde_json::Value = serde_json::from_str(&text).expect("json");
    let parsed: Request = serde_json::from_value(raw.clone()).expect("a request");
    assert_eq!(unknown_fields(&raw, &parsed).as_deref(), Some("args.flow.roles.0.bogus"));
}

#[test]
fn the_log_fields_never_carry_text() {
    let mut flow = example();
    flow.roles[0].prompt = "SECRET-PROMPT".into();
    flow.roles[0].env.insert("TOKEN".into(), "SECRET-ENV".into());
    let save = Command::FlowSave(FlowSave { flow: FlowSpec::from(&flow), ..FlowSave::default() });
    let shown = format!("{:?}", save.fields());
    assert!(!shown.contains("SECRET"), "{shown}");
    assert!(shown.contains("Pair"), "{shown}");
}
