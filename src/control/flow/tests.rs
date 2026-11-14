use super::*;
use crate::agents::Known;
use crate::control::contract::unknown_fields;
use crate::control::{Command, Request, Snapshot};
use crate::flows::progress::Edge;
use crate::flows::{ORCHESTRATOR, builtin};
use crate::test_util::fixture;
use serde_json::Value;

#[test]
fn an_import_answer_from_an_older_server_has_no_roles_or_risks() {
    let imported: Imported =
        serde_json::from_str(r#"{"name":"Mine","origin":"user","path":null,"replaced":false,"future":true}"#)
            .expect("an older answer with an unknown field");
    assert_eq!((imported.name.as_str(), imported.roles, imported.check, imported.note), ("Mine", vec![], None, None));
}

#[test]
fn a_save_answer_counts_its_own_problems_even_when_there_are_none() {
    let value = serde_json::to_value(FlowSaved { own_problems: Some(0), ..FlowSaved::default() }).expect("json");
    assert_eq!(value["own_problems"], 0);
}

#[test]
fn a_save_answer_from_an_older_server_has_no_count() {
    let saved: FlowSaved = serde_json::from_str(
        r#"{"name":"Mine","origin":"user","path":null,"saved":false,"problems":["x"],"check":null,"future":true}"#,
    )
    .expect("an older answer with an unknown field");
    assert_eq!((saved.problems, saved.own_problems), (vec!["x".to_string()], None));
}

fn member(name: &str, prompt: &str) -> Role {
    let mut role = Role { name: name.into(), agent: "claude".into(), ..Role::default() };
    role.prompt = prompt.into();
    role
}

fn example() -> Flow {
    let mut orchestrator = member(ORCHESTRATOR, "Be brief.");
    orchestrator.model = Some("opus".into());
    let mut coder = member("coder", "Write it.");
    coder.edits = true;
    coder.effort = Some("high".into());
    coder.args.push("--verbose".into());
    coder.env.insert("NODE_ENV".into(), "test".into());
    coder.worktree = Worktree::Own;
    let mut reviewer = member("reviewer", "Review it.");
    reviewer.model = Some("sonnet".into());
    let back = Back { to: 1, when: "findings".into(), at_most: 2 };
    let stages = [("coder", None), ("reviewer", Some(back))];
    let mut flow = Flow { name: "Pair".into(), notes: "Keep it small.".into(), ..Flow::default() };
    flow.orchestrator = orchestrator;
    flow.roles = vec![coder, reviewer];
    flow.stages = stages.into_iter().map(|(role, back)| Stage { roles: vec![role.into()], back }).collect();
    flow
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
    let raw: Value = serde_json::from_str(&text).expect("json");
    let parsed: Request = serde_json::from_value(raw.clone()).expect("a request");
    assert!(matches!(parsed.command, Command::FlowSave(_)));
    assert_eq!(unknown_fields(&raw, &parsed), None);
}

#[test]
fn a_stray_key_inside_a_role_is_reported_with_its_path() {
    let text = request(&example(), "").replace(r#""name":"coder""#, r#""name":"coder","bogus":1"#);
    let raw: Value = serde_json::from_str(&text).expect("json");
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

#[test]
fn a_role_says_it_exited_only_when_it_did() {
    let quiet = serde_json::to_value(RoleState::default()).expect("json");
    assert!(quiet.get("exited").is_none());

    let gone = RoleState { role: "coder".into(), exited: true, ..RoleState::default() };
    let sent = serde_json::to_value(&gone).expect("json");
    assert_eq!(sent["exited"], serde_json::json!(true));
    assert_eq!(serde_json::from_value::<RoleState>(sent).expect("back"), gone);
}

fn example_run() -> RunState {
    let role = |name: &str, pane: Option<u64>, tab: Option<u64>, status: Option<&str>, closed: bool| RoleState {
        role: name.into(),
        pane,
        tab,
        status: status.map(str::to_string),
        closed,
        ..RoleState::default()
    };
    let coder = RoleState {
        worktree: Some("/w/fix--coder".into()),
        branch: Some("fix--coder".into()),
        ..role("coder", Some(11), Some(21), Some("working"), false)
    };
    RunState {
        run: 7,
        project: 1,
        workspace: 3,
        flow: FlowSpec::from(&example()),
        task: "Fix the login form".into(),
        dir: Some("/state/runs/2026-10-18-fix".into()),
        roles: vec![
            role("orchestrator", Some(10), Some(20), Some("pending"), false),
            coder,
            role("reviewer", Some(12), None, None, true),
        ],
        stage: Some(0),
        rounds: vec![0, 1],
        holder: Some("coder".into()),
        last: Some(StepInfo {
            edge: RunEdge::Loop { stage: 1 },
            from: Some("reviewer".into()),
            to: "coder".into(),
            time: "2026-10-18T09:30:00.000Z".into(),
        }),
        outside: Some(RunEdge::Outside { from: Some(0), to: 1 }),
        ended: true,
        messages: 4,
        ..RunState::default()
    }
}

fn message(seq: u64, to: &str, kind: &str, text: Option<&str>, edge: Option<RunEdge>) -> FlowMessage {
    FlowMessage {
        run: 7,
        workspace: 3,
        seq,
        time: format!("2026-10-18T09:{}:00.000Z", 20 + seq),
        from: "orchestrator".into(),
        to: to.into(),
        kind: kind.into(),
        text: text.map(str::to_string),
        bytes: text.map_or(0, str::len),
        edge,
    }
}

#[test]
fn not_run_is_counted_from_0_and_left_out_when_empty() {
    let changed = RunState { not_run: vec![2, 3], ..example_run() };
    let value = serde_json::to_value(&changed).expect("json");
    assert_eq!(value["not_run"], serde_json::json!([2, 3]));
    assert_eq!(serde_json::from_value::<RunState>(value).expect("read"), changed);
    assert!(serde_json::to_value(example_run()).expect("json").get("not_run").is_none());
}

#[test]
fn the_runs_snapshot_is_pinned() {
    let snapshot = Snapshot {
        revision: 3,
        report: FlowRuns { version: "0.12.147".into(), boot: "123-456".into(), runs: vec![example_run()] },
    };
    let written = serde_json::to_string_pretty(&snapshot).expect("json");
    assert_eq!(written, fixture!("control/flow-runs/v1/snapshot.json").trim_end());
    let read: Snapshot<FlowRuns> =
        serde_json::from_str(fixture!("control/flow-runs/v1/snapshot.json")).expect("a snapshot");
    assert_eq!(read, snapshot);
}

#[test]
fn a_streamed_snapshot_is_one_line_holding_it_under_ok() {
    let snapshot = Snapshot { revision: 1, report: FlowRuns::default() };
    let line = snapshot.streamed().expect("a line");
    assert!(!line.contains('\n') && line.starts_with(r#"{"ok":{"#), "{line}");
}

#[test]
fn messages_are_pinned() {
    let backlog = FlowBacklog {
        boot: "123-456".into(),
        run: 7,
        workspace: 3,
        messages: vec![
            message(1, "coder", "send", Some("Write it."), None),
            message(2, "reviewer", "send", Some("Review it."), Some(RunEdge::Forward { stage: 0 })),
            message(3, "reviewer", "read", None, None),
        ],
    };
    let written = serde_json::to_string_pretty(&backlog).expect("json");
    assert_eq!(written, fixture!("control/flow-messages/v1/backlog.json").trim_end());
    let read: FlowBacklog = serde_json::from_str(fixture!("control/flow-messages/v1/backlog.json")).expect("a backlog");
    assert_eq!(read, backlog);
    let next = message(4, "coder", "send", Some("Fix the findings."), Some(RunEdge::Loop { stage: 1 }));
    let written = serde_json::to_string_pretty(&next).expect("json");
    assert_eq!(written, fixture!("control/flow-messages/v1/message.json").trim_end());
    let read: FlowMessage = serde_json::from_str(fixture!("control/flow-messages/v1/message.json")).expect("a message");
    assert_eq!(read, next);
}

#[test]
fn the_new_requests_never_log_a_text() {
    let messages = Command::FlowMessages(FlowMessages { workspace: Some(3) });
    let files = Command::FlowFiles(FlowFiles { workspace: None });
    assert_eq!(messages.fields(), [("workspace", "3".to_string())]);
    assert_eq!(files.fields(), []);
    assert!(Command::chatty("flow-files") && !Command::chatty("flow-messages"));
}

#[test]
fn an_edge_names_its_kind_and_its_stages() {
    let edges = [Edge::Forward(1), Edge::Loop(2), Edge::Outside { from: None, to: 3 }];
    let written: Vec<Value> =
        edges.iter().map(|edge| serde_json::to_value(RunEdge::from(*edge)).expect("json")).collect();
    assert_eq!(
        written,
        [
            serde_json::json!({"kind": "forward", "stage": 1}),
            serde_json::json!({"kind": "loop", "stage": 2}),
            serde_json::json!({"kind": "outside", "from": null, "to": 3}),
        ]
    );
}

#[test]
fn a_closed_run_says_so_and_a_running_one_has_no_state_key() {
    let running = serde_json::to_value(example_run()).expect("json");
    let closed = serde_json::to_value(RunState { state: Some("closed".into()), ..example_run() }).expect("json");

    assert!(running.get("state").is_none());
    assert_eq!(closed["state"], "closed");
}

#[test]
fn the_default_and_order_requests_write_every_field() {
    for (command, args) in
        [("flow-default", r#"{"name": null, "project": null}"#), ("flow-order", r#"{"names": [], "project": null}"#)]
    {
        let text = format!(r#"{{"command": "{command}", "args": {args}}}"#);
        let raw: Value = serde_json::from_str(&text).expect("json");
        let parsed: Request = serde_json::from_value(raw.clone()).expect("a request");
        assert_eq!(unknown_fields(&raw, &parsed), None, "{command}");
    }
}

#[test]
fn they_log_the_flow_and_a_count() {
    let default = Command::FlowDefault(FlowDefault { name: Some("Quick".into()), project: Some(3) });
    let order = Command::FlowOrder(FlowOrder { names: vec!["A".into(), "B".into()], project: None });
    assert_eq!(default.fields(), [("flow", "Quick".to_string()), ("project", "3".to_string())]);
    assert_eq!(order.fields(), [("count", "2".to_string())]);
    assert_eq!(Command::FlowDefault(FlowDefault::default()).fields(), []);
}

#[test]
fn a_run_answer_has_a_projection_only_when_a_view_moved() {
    let none = serde_json::to_string(&RunInfo::default()).expect("json");
    assert!(!none.contains("projection"), "{none}");
    let moved = serde_json::to_string(&RunInfo { projection: Some(2), ..RunInfo::default() }).expect("json");
    assert!(moved.contains("\"projection\":2"), "{moved}");
    let older: RunInfo = serde_json::from_str(r#"{"run": 1}"#).expect("an older answer");
    assert_eq!(older.projection, None);
}

mod close_when_merged {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_run_that_closes_when_merged_says_so_and_others_have_no_key() {
        let plain = serde_json::to_value(example_run()).expect("json");
        assert!(plain.get("close_when_merged").is_none());
        let opted = RunState { close_when_merged: true, ..example_run() };
        assert_eq!(serde_json::to_value(opted).expect("json")["close_when_merged"], json!(true));
    }

    #[test]
    fn a_flow_run_request_writes_the_key_only_when_set() {
        let plain = serde_json::to_value(FlowRun::default()).expect("json");
        assert!(plain.get("close_when_merged").is_none());
        let opted = FlowRun { close_when_merged: true, ..FlowRun::default() };
        assert_eq!(serde_json::to_value(&opted).expect("json")["close_when_merged"], json!(true));
        assert!(Command::FlowRun(opted).fields().contains(&("close_when_merged", "true".into())));
    }

    #[test]
    fn the_run_answer_has_the_key_only_when_set() {
        assert!(serde_json::to_value(RunInfo::default()).expect("json").get("close_when_merged").is_none());
        let opted = RunInfo { close_when_merged: true, ..RunInfo::default() };
        assert_eq!(serde_json::to_value(opted).expect("json")["close_when_merged"], json!(true));
    }

    #[test]
    fn free_builds_is_written_only_when_set_by_every_type_that_carries_it() {
        let key = |value: Value| value.get("free_builds").cloned();
        assert_eq!(key(serde_json::to_value(FlowRun::default()).expect("json")), None);
        assert_eq!(key(serde_json::to_value(RunInfo::default()).expect("json")), None);
        assert_eq!(key(serde_json::to_value(example_run()).expect("json")), None);
        assert_eq!(key(serde_json::to_value(FlowCloseWhenMerged::default()).expect("json")), None);
        assert_eq!(key(serde_json::to_value(CloseWhenMergedSet::default()).expect("json")), None);
        let (run, info) =
            (FlowRun { free_builds: true, ..FlowRun::default() }, RunInfo { free_builds: true, ..RunInfo::default() });
        let (state, close) = (
            RunState { free_builds: true, ..example_run() },
            FlowCloseWhenMerged { free_builds: true, ..FlowCloseWhenMerged::default() },
        );
        let set = CloseWhenMergedSet { free_builds: true, ..CloseWhenMergedSet::default() };
        for value in [json_of(&run), json_of(&info), json_of(&state), json_of(&close), json_of(&set)] {
            assert_eq!(key(value), Some(json!(true)));
        }
        let fields = |command: Command| command.fields().contains(&("free_builds", "true".into()));
        assert!(fields(Command::FlowRun(run)));
        assert!(fields(Command::FlowCloseWhenMerged(close)));
    }

    fn json_of(value: &impl Serialize) -> Value {
        serde_json::to_value(value).expect("json")
    }

    fn asked() -> Command {
        Command::FlowCloseWhenMerged(FlowCloseWhenMerged { workspace: Some(7), off: false, free_builds: false })
    }

    fn answered() -> CloseWhenMergedSet {
        CloseWhenMergedSet {
            run: 9,
            project: 2,
            workspace: 7,
            close_when_merged: true,
            free_builds: false,
            changed: true,
        }
    }

    #[test]
    fn the_request_writes_every_field_as_its_fixture() {
        let written = serde_json::to_string_pretty(&asked()).expect("json");
        assert_eq!(written, fixture!("control/flow-close-when-merged/v1/request.json").trim_end());
        let raw: Value =
            serde_json::from_str(fixture!("control/flow-close-when-merged/v1/request.json")).expect("json");
        let parsed: Request = serde_json::from_value(raw.clone()).expect("a request");
        assert_eq!(unknown_fields(&raw, &parsed), None);
        assert_eq!(parsed.command, asked());
    }

    #[test]
    fn the_answer_is_its_fixture_and_reads_leniently() {
        let written = serde_json::to_string_pretty(&answered()).expect("json");
        assert_eq!(written, fixture!("control/flow-close-when-merged/v1/answer.json").trim_end());
        let read: CloseWhenMergedSet =
            serde_json::from_str(fixture!("control/flow-close-when-merged/v1/answer.json")).expect("answer");
        assert_eq!(read, answered());
        let later: CloseWhenMergedSet = serde_json::from_value(
            json!({"run": 9, "project": 2, "workspace": 7, "close_when_merged": true, "changed": true, "later": 1}),
        )
        .expect("a later answer");
        assert_eq!(later, answered());
    }
}
