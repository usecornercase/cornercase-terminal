use serde_json::json;

use super::*;

mod describing {
    use super::*;

    #[test]
    fn every_command_is_an_operation_in_order() {
        let descriptor = describe("1-2", &[]);

        let names: Vec<&str> = descriptor.operations.iter().map(|operation| operation.name.as_str()).collect();

        assert_eq!(names, Command::NAMES);
        assert!(descriptor.operations.iter().all(|operation| operation.available && operation.reason.is_none()));
    }

    #[test]
    fn watching_is_an_available_operation() {
        let descriptor = describe("1-2", &[]);

        let watch = descriptor.operations.iter().find(|operation| operation.name == "watch").expect("watch");

        assert!(watch.available && watch.reason.is_none());
    }

    #[test]
    fn it_names_the_version_the_contract_and_the_boot() {
        let descriptor = describe("1-2", &[]);

        assert_eq!(descriptor.version, crate::version::CURRENT);
        assert_eq!(descriptor.contract, CONTRACT);
        assert_eq!(descriptor.boot, "1-2");
        assert_eq!(descriptor.features, FEATURES);
        let wanted =
            ["theme", "notices", "todo-panel", "close-unlock", "agent-folds", "issues", "todo-panel", "changes"];
        let missing: Vec<&str> =
            wanted.into_iter().filter(|name| !descriptor.features.iter().any(|feature| feature == name)).collect();
        assert!(missing.is_empty(), "missing features {missing:?}");
    }

    #[test]
    fn the_events_watch_surface_view_app_changes_and_flow_streams_are_version_one() {
        let descriptor = describe("1-2", &[]);

        let streams = vec![
            Stream { name: "events".into(), version: 1 },
            Stream { name: "watch".into(), version: 1 },
            Stream { name: "surface".into(), version: 1 },
            Stream { name: "view".into(), version: 1 },
            Stream { name: "app".into(), version: 1 },
            Stream { name: "changes".into(), version: 1 },
            Stream { name: "flow-runs".into(), version: 1 },
            Stream { name: "flow-messages".into(), version: 1 },
        ];
        assert_eq!(descriptor.streams, streams);
    }

    #[test]
    fn an_unavailable_operation_says_why_and_the_others_stay_available() {
        let descriptor = describe("1-2", &[("search", "the index is off")]);

        let search = descriptor.operations.iter().find(|operation| operation.name == "search").expect("search");
        assert_eq!(
            search,
            &Operation { name: "search".into(), available: false, reason: Some("the index is off".into()) }
        );
        let others = descriptor.operations.iter().filter(|operation| operation.name != "search");
        assert!(others.clone().all(|operation| operation.available && operation.reason.is_none()));
        assert!(others.clone().any(|operation| operation.name == "status"));
    }

    #[test]
    fn it_serializes_with_every_key() {
        let sent = serde_json::to_value(describe("1-2", &[("search", "off")])).expect("json");

        let keys: Vec<&str> = sent.as_object().expect("an object").keys().map(String::as_str).collect();
        assert_eq!(keys, ["boot", "contract", "features", "operations", "streams", "version"]);
        assert_eq!(
            sent["streams"],
            json!([{"name": "events", "version": 1}, {"name": "watch", "version": 1}, {"name": "surface", "version": 1}, {"name": "view", "version": 1}, {"name": "app", "version": 1}, {"name": "changes", "version": 1}, {"name": "flow-runs", "version": 1}, {"name": "flow-messages", "version": 1}])
        );
        assert_eq!(sent["operations"][0], json!({"name": "status", "available": true}));
        let search = sent["operations"].as_array().expect("a list").iter().find(|o| o["name"] == "search").cloned();
        assert_eq!(search, Some(json!({"name": "search", "available": false, "reason": "off"})));
    }

    #[test]
    fn it_reads_back_what_it_wrote() {
        let descriptor = describe("1-2", &[("search", "off")]);

        let read: Descriptor = serde_json::from_value(serde_json::to_value(&descriptor).expect("json")).expect("read");

        assert_eq!(read, descriptor);
    }
}

mod refusing {
    use super::*;

    #[test]
    fn a_plain_message_is_a_failure() {
        let refusal = Refusal::from("tab 9 has no pane".to_string());

        assert_eq!(refusal, Refusal::new(Code::Failed, "tab 9 has no pane"));
    }

    #[test]
    fn codes_are_written_in_kebab_case() {
        let codes = [
            Code::Unsupported,
            Code::Malformed,
            Code::StaleBoot,
            Code::NotFound,
            Code::UnknownField,
            Code::Unavailable,
            Code::Failed,
            Code::Busy,
            Code::Conflict,
        ];

        let written: Vec<Value> = codes.iter().map(|code| serde_json::to_value(code).expect("json")).collect();

        assert_eq!(
            written,
            [
                "unsupported",
                "malformed",
                "stale-boot",
                "not-found",
                "unknown-field",
                "unavailable",
                "failed",
                "busy",
                "conflict"
            ]
        );
    }
}

mod rendering {
    use super::*;

    #[test]
    fn a_legacy_answer_is_what_servers_always_wrote() {
        let refusal = Refusal::new(Code::NotFound, "no pane 9");

        assert_eq!(render(Ok(json!({"pane": 3})), false).as_deref(), Some(r#"{"ok":{"pane":3}}"#));
        assert_eq!(render(Err(refusal), false).as_deref(), Some(r#"{"error":"no pane 9"}"#));
    }

    #[test]
    fn a_structured_error_carries_its_code() {
        let refusal = Refusal::new(Code::StaleBoot, "another boot");

        assert_eq!(render(Ok(json!({})), true).as_deref(), Some(r#"{"ok":{}}"#));
        assert_eq!(
            render(Err(refusal), true).as_deref(),
            Some(r#"{"error":{"code":"stale-boot","message":"another boot"}}"#)
        );
    }
}

mod finding_unknown_fields {
    use rstest::rstest;

    use super::*;

    fn stray_in(raw: &Value) -> Option<String> {
        let parsed: Request = serde_json::from_value(raw.clone()).expect("a request");
        unknown_fields(raw, &parsed)
    }

    fn with(mut raw: Value, path: &[&str], key: &str) -> Value {
        let mut place = &mut raw;
        for step in path {
            place = &mut place[*step];
        }
        place.as_object_mut().expect("an object").insert(key.into(), json!(1));
        raw
    }

    #[rstest]
    #[case::status(json!({"command": "status", "args": {}}))]
    #[case::send(json!({"caller": 4, "server": "1-2", "contract": 1, "boot": "1-2", "command": "send",
        "args": {"pane": 3, "tab": null, "text": "hi", "enter": true, "wait": true, "until": "turn-over",
        "timeout": 5.0, "force": false}}))]
    #[case::start_with(json!({"command": "start-with", "args": {"agent": "claude", "model": "opus",
        "mode": "plan", "worktree": "fix", "prompt": "go", "until": "idle", "focus": true}}))]
    #[case::wait_text(json!({"command": "wait", "args": {"pane": 3, "until": {"text": "done"}, "timeout": 2}}))]
    #[case::wait_quiet(json!({"command": "wait", "args": {"tab": 2, "until": {"quiet": 1.5}}}))]
    #[case::wait_several(json!({"command": "wait-several", "args": {"panes": [3, 4], "tabs": [2], "all": true}}))]
    #[case::close(json!({"command": "close", "args": {"item": {"workspace": 2}, "remove_worktree": true}}))]
    #[case::focus(json!({"command": "focus", "args": {"item": {"pane": 3}}}))]
    #[case::rename(json!({"command": "rename", "args": {"item": {"tab": 2}, "name": "api"}}))]
    #[case::queue_add(json!({"command": "queue", "args": {"action": "add",
        "entry": {"at": "14:30", "zone": "UTC", "agent": "codex", "prompt": "go", "submit": true}}}))]
    #[case::queue_edit(json!({"command": "queue", "args": {"action": "edit",
        "entry": {"id": 7, "add": {"at": "in 2h", "zone": "UTC"}}}}))]
    #[case::queue_remove(json!({"command": "queue", "args": {"action": "remove", "entry": 7}}))]
    #[case::queue_list(json!({"command": "queue", "args": {"action": "list"}}))]
    #[case::todo_add(json!({"command": "todo", "args": {"add": "write the docs"}}))]
    #[case::todo_list(json!({"command": "todo", "args": "list"}))]
    #[case::app(json!({"command": "app", "args": {}}))]
    #[case::update_check(json!({"command": "update-check", "args": {}}))]
    #[case::update_install(json!({"command": "update-install", "args": {}}))]
    #[case::todo_edit(json!({"command": "todo", "args": {"edit": {"id": 3, "text": "again"}}}))]
    #[case::todo_move(json!({"command": "todo", "args": {"move": {"id": 3, "before": 5}}}))]
    #[case::todo_move_last(json!({"command": "todo", "args": {"move": {"id": 3}}}))]
    #[case::todo_clear_done(json!({"command": "todo", "args": "clear-done"}))]
    #[case::todo_restore(json!({"command": "todo", "args": {"restore": 7}}))]
    #[case::events(json!({"command": "events", "args": {"panes": [3]}}))]
    #[case::flow_import(json!({"command": "flow-import", "args": {"text": "name = 'x'", "place": "yours",
        "replace": true}}))]
    #[case::search(json!({"command": "search", "args": {"query": "token"}}))]
    #[case::contract(json!({"contract": 1, "command": "contract", "args": {}}))]
    #[case::watch(json!({"contract": 1, "boot": "1-2", "command": "watch", "args": {}}))]
    fn a_request_the_server_knows_has_none(#[case] raw: Value) {
        assert_eq!(stray_in(&raw), None);
    }

    #[rstest]
    #[case::status(json!({"command": "status", "args": {}}))]
    #[case::send(json!({"command": "send", "args": {"pane": 3, "text": "hi"}}))]
    #[case::todo_list(json!({"command": "todo", "args": "list"}))]
    #[case::todo_edit(json!({"command": "todo", "args": {"edit": {"id": 3, "text": "again"}}}))]
    #[case::todo_move(json!({"command": "todo", "args": {"move": {"id": 3, "before": 5}}}))]
    #[case::todo_move_last(json!({"command": "todo", "args": {"move": {"id": 3}}}))]
    #[case::todo_clear_done(json!({"command": "todo", "args": "clear-done"}))]
    #[case::todo_restore(json!({"command": "todo", "args": {"restore": 7}}))]
    #[case::queue_remove(json!({"command": "queue", "args": {"action": "remove", "entry": 7}}))]
    #[case::contract(json!({"contract": 1, "boot": "1-2", "command": "contract", "args": {}}))]
    #[case::watch(json!({"contract": 1, "boot": "1-2", "command": "watch", "args": {}}))]
    fn an_extra_key_beside_the_command_is_named(#[case] raw: Value) {
        assert_eq!(stray_in(&with(raw, &[], "extra")).as_deref(), Some("extra"));
    }

    #[rstest]
    #[case::status(json!({"command": "status", "args": {}}))]
    #[case::send(json!({"command": "send", "args": {"pane": 3, "text": "hi"}}))]
    #[case::wait(json!({"command": "wait", "args": {"pane": 3, "until": {"text": "done"}}}))]
    #[case::close(json!({"command": "close", "args": {"item": {"pane": 3}}}))]
    #[case::focus(json!({"command": "focus", "args": {"item": {"tab": 2}}}))]
    #[case::queue_list(json!({"command": "queue", "args": {"action": "list"}}))]
    #[case::flow_run(json!({"command": "flow-run", "args": {"name": "Quick", "prompt": "go"}}))]
    #[case::contract(json!({"command": "contract", "args": {}}))]
    #[case::watch(json!({"command": "watch", "args": {}}))]
    fn an_extra_argument_is_named(#[case] raw: Value) {
        assert_eq!(stray_in(&with(raw, &["args"], "bogus")).as_deref(), Some("args.bogus"));
    }

    #[test]
    fn an_extra_key_inside_a_queue_entry_is_named_by_its_path() {
        let raw = json!({"command": "queue", "args": {"action": "add", "entry": {"at": "14:30", "zone": "UTC"}}});

        assert_eq!(stray_in(&with(raw, &["args", "entry"], "repeat")).as_deref(), Some("args.entry.repeat"));
    }

    #[test]
    fn arguments_left_out_are_not_unknown() {
        let bare = json!({"command": "send", "args": {}});
        let empty = json!({"command": "start", "args": {}});
        let nulls = json!({"caller": null, "server": null, "command": "read", "args": {"pane": null, "lines": null}});

        assert_eq!((stray_in(&bare), stray_in(&empty), stray_in(&nulls)), (None, None, None));
    }

    #[test]
    fn a_request_that_is_not_an_object_has_none() {
        let parsed: Request = serde_json::from_value(json!({"command": "status", "args": {}})).expect("a request");

        assert_eq!(unknown_fields(&json!("status"), &parsed), None);
    }
}
