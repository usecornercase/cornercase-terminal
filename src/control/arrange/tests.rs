use serde_json::{Value, json};

use super::*;
use crate::control::contract::unknown_fields;
use crate::control::{Command, Request, command_name};
use crate::test_util::fixture;

fn request(command: &str, args: &Value) -> Request {
    serde_json::from_value(json!({"command": command, "args": args})).expect("a request")
}

#[test]
fn every_request_reads_from_its_documented_json() {
    assert_eq!(
        request("move", &json!({"item": {"tab": 9}, "before": 11})).command,
        Command::Move(Move { item: Item::Tab(9), before: Some(11), into: None, ungroup: false })
    );
    assert_eq!(
        request("move", &json!({"item": {"project": 5}, "into": 3})).command,
        Command::Move(Move { item: Item::Project(5), before: None, into: Some(3), ungroup: false })
    );
    assert_eq!(
        request("move-pane", &json!({"pane": 4, "target": 7, "place": "left"})).command,
        Command::MovePane(MovePane { pane: Some(4), target: Some(7), place: Some(Place::Left) })
    );
    assert_eq!(
        request("group-style", &json!({"group": 3, "icon": "★", "colour": 99})).command,
        Command::GroupStyle(GroupStyle { group: Some(3), icon: Some("★".into()), colour: Some(99) })
    );
    assert_eq!(
        request("group-delete", &json!({"group": 3, "close_projects": true})).command,
        Command::GroupDelete(GroupDelete { group: Some(3), close_projects: true })
    );
    assert_eq!(
        request("removal-check", &json!({"workspace": 7})).command,
        Command::RemovalCheck(RemovalCheck { workspace: Some(7) })
    );
    assert_eq!(
        request("close-agent", &json!({"pane": 4, "check": true, "delete_branch": true})).command,
        Command::CloseAgent(CloseAgent { pane: 4, check: true, delete_branch: true, force: false, unlock: false })
    );
}

#[test]
fn close_agent_needs_the_pane() {
    let parsed = serde_json::from_value::<Request>(json!({"command": "close-agent", "args": {"check": true}}));

    assert!(parsed.is_err());
}

#[test]
fn an_older_server_refuses_close_agent_by_name() {
    let asked = Request {
        caller: None,
        server: None,
        contract: None,
        boot: None,
        view: None,
        command: Command::CloseAgent(CloseAgent { pane: 4, ..CloseAgent::default() }),
    };
    let sent = serde_json::to_value(asked).expect("json");

    let added = Command::NAMES.iter().position(|name| *name == "close-agent").expect("close-agent is a name");
    let older = &Command::NAMES[..added];
    assert_eq!(command_name(&sent), Some("close-agent"));
    assert!(!older.contains(&"close-agent") && sent["args"].get("item").is_none(), "{sent}");
}

#[test]
fn a_move_needs_the_item() {
    let parsed = serde_json::from_value::<Request>(json!({"command": "move", "args": {"before": 1}}));

    assert!(parsed.is_err());
}

#[test]
fn the_strict_check_knows_every_field_of_a_request() {
    let all = [
        ("move", json!({"item": {"tab": 9}, "before": 1, "into": 2, "ungroup": true})),
        ("move-pane", json!({"pane": 4, "target": 7, "place": "swap"})),
        ("collapse", json!({"item": {"group": 3}})),
        ("group-new", json!({"name": "work"})),
        ("group-style", json!({"group": 3, "icon": "★", "colour": 99})),
        ("group-delete", json!({"group": 3, "close_projects": true})),
        ("removal-check", json!({"workspace": 7})),
        ("close-agent", json!({"pane": 4, "check": true, "delete_branch": true, "force": true, "unlock": true})),
    ];

    for (command, args) in all {
        let raw = json!({"command": command, "args": args});
        let parsed: Request = serde_json::from_value(raw.clone()).expect("a request");
        assert_eq!(unknown_fields(&raw, &parsed), None, "{command}");
    }
}

#[test]
fn every_place_maps_to_the_splits() {
    use crate::project::split::Place as Split;

    let places = [Place::Swap, Place::Left, Place::Right, Place::Above, Place::Below];
    let splits = [Split::Swap, Split::Left, Split::Right, Split::Above, Split::Below];

    assert_eq!(places.map(Split::from), splits);
}

#[test]
fn the_fields_name_ids_and_choices_never_texts() {
    let moving = Move { item: Item::Project(5), before: Some(2), into: None, ungroup: false };
    let mut fields = Fields::default();
    moving.fields(&mut fields);
    assert_eq!(
        fields.0,
        [("item", "project-5".to_string()), ("before", "2".to_string()), ("ungroup", "false".to_string()),]
    );

    let mut fields = Fields::default();
    MovePane { pane: Some(4), target: Some(7), place: Some(Place::Below) }.fields(&mut fields);
    assert_eq!(fields.0, [("pane", "4".to_string()), ("target", "7".to_string()), ("place", "below".to_string())]);
}

#[test]
fn a_removal_without_a_lock_writes_every_key() {
    let value = serde_json::to_value(Removal { workspace: 7, ..Removal::default() }).expect("json");

    for key in ["workspace", "path", "branch", "changed", "lock", "roles", "teardown"] {
        assert!(value.get(key).is_some(), "{key}: {value}");
    }
    assert_eq!(value["lock"], Value::Null);
    assert_eq!(value["branch"], Value::Null);
}

fn removal_of_a_run() -> Removal {
    let locked = Locked {
        reason: "on a usb disk".into(),
        path: "/w/wt".into(),
        role: None,
        pid: Some(41),
        running: Some(false),
    };
    Removal {
        workspace: 3,
        path: "/w/wt".into(),
        branch: Some("wt".into()),
        changed: true,
        lock: Some(locked),
        roles: vec![RoleTree { role: "reviewer".into(), path: "/w/wt-reviewer".into(), branch: "wt-reviewer".into() }],
        teardown: true,
    }
}

fn agent_closing() -> AgentClosing {
    let fate = |path: &str, branch: &str, lost: bool| BranchFate {
        path: path.into(),
        branch: Some(branch.into()),
        lost,
        deletes: true,
        deleted: !lost,
    };
    let pane = |pane: u64, agent: Option<&str>, workspace: u64, tab: u64| PaneRef {
        pane,
        agent: agent.map(str::to_string),
        project: 1,
        workspace,
        tab,
    };
    AgentClosing {
        pane: 7,
        agent: Some("claude".into()),
        project: 1,
        workspace: 3,
        removes: Some(removal_of_a_run()),
        branches: vec![fate("/w/wt", "wt", false), fate("/w/wt-reviewer", "wt-reviewer", true)],
        tabs: vec![5, 6],
        stops: vec![pane(7, Some("claude"), 3, 5), pane(8, None, 3, 6), pane(11, Some("codex"), 4, 9)],
        keeps: vec![pane(12, Some("codex"), 2, 10)],
        run: Some(2),
        closed: true,
        warning: Some("could not delete the branch wt-reviewer: not fully merged".into()),
    }
}

fn written(closing: &AgentClosing) -> String {
    serde_json::to_string_pretty(closing).expect("json")
}

#[test]
fn the_agent_closing_is_written_as_its_fixture() {
    assert_eq!(written(&agent_closing()), fixture!("control/close-agent/answer.json").trim_end());
}

#[test]
fn the_agent_closing_fixture_reads_back() {
    let read: AgentClosing = serde_json::from_str(fixture!("control/close-agent/answer.json")).expect("an answer");

    assert_eq!(read, agent_closing());
}

#[test]
fn an_agent_closing_from_a_newer_server_reads_with_its_extra_keys() {
    let read: AgentClosing =
        serde_json::from_value(json!({"pane": 7, "stops": [{"pane": 7, "title": "x"}], "newer": true})).expect("read");

    assert_eq!((read.pane, read.stops.len(), read.removes), (7, 1, None));
}
