use serde_json::json;

use super::*;
use crate::control::Request;

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
    fn showing_a_bot_in_a_view_is_a_feature() {
        assert!(FEATURES.contains(&"view-bot"));
    }

    #[test]
    fn it_lists_the_feature_of_a_runs_not_run() {
        assert!(FEATURES.contains(&"flow-not-run"));
    }

    #[test]
    fn cli_endpoint_is_a_feature() {
        assert!(FEATURES.contains(&"cli-endpoint"));
    }

    #[test]
    fn bot_routines_is_a_feature() {
        assert!(FEATURES.contains(&"bot-routines"));
        assert!(FEATURES.contains(&"bot-reads"));
    }

    #[test]
    fn bot_follow_is_a_feature() {
        assert!(FEATURES.contains(&"bot-follow"));
    }

    #[test]
    fn prompt_unsent_is_a_feature() {
        assert!(FEATURES.contains(&"prompt-unsent"));
    }

    #[test]
    fn reading_what_a_bot_is_for_is_a_feature() {
        assert!(FEATURES.contains(&"bot-about-read"));
    }

    #[test]
    fn keeping_a_bots_secrets_is_a_feature() {
        assert!(FEATURES.contains(&"bot-secrets"));
    }

    #[test]
    fn a_bot_asking_for_its_secrets_is_a_feature() {
        assert!(FEATURES.contains(&"bot-asks"));
    }

    #[test]
    fn bot_setup_is_a_feature() {
        assert!(FEATURES.contains(&"bot-setup"));
    }

    #[test]
    fn it_lists_the_feature_of_a_run_that_stopped_moving() {
        assert!(FEATURES.contains(&"flow-stopped"));
    }

    #[test]
    fn it_names_the_version_the_contract_and_the_boot() {
        let descriptor = describe("1-2", &[]);

        assert_eq!(descriptor.version, crate::version::CURRENT);
        assert_eq!(descriptor.contract, CONTRACT);
        assert_eq!(descriptor.boot, "1-2");
        assert_eq!(descriptor.features, FEATURES);
        let wanted = [
            "theme",
            "notices",
            "todo-panel",
            "close-unlock",
            "agent-folds",
            "issues",
            "flow-issue-prompt",
            "todo-panel",
            "changes",
            "files",
            "machines",
            "machine-usage",
            "stats-cached",
            "machine-workspaces",
            "machine-start",
            "machine-retry-at",
            "against-base",
            "notice-subagent",
            "mode-default",
            "held-after-draft",
            "flow-close-when-merged",
            "fresh-branches",
            "notice-messages",
            "surface-wrapped",
            "account-login-tab",
            "config-agents",
            "tunnels",
            "tunnels-remembered",
            "tunnel-hold",
            "agent-models",
        ];
        let missing: Vec<&str> =
            wanted.into_iter().filter(|name| !descriptor.features.iter().any(|feature| feature == name)).collect();
        assert!(missing.is_empty(), "missing features {missing:?}");
    }

    #[test]
    fn taking_tunnel_orders_is_a_feature() {
        assert!(FEATURES.contains(&"tunnel-orders"));
    }

    #[test]
    fn reverse_tunnels_are_a_feature() {
        assert!(FEATURES.contains(&"tunnel-reverse"));
    }

    #[test]
    fn a_machine_s_tunnels_and_the_end_of_a_hold_are_features() {
        assert!(FEATURES.contains(&"machine-tunnels"));
        assert!(FEATURES.contains(&"tunnel-hold-ended"));
    }

    #[test]
    fn a_machine_that_is_not_shown_and_has_a_command_is_a_feature() {
        assert!(FEATURES.contains(&"machine-shown"));
    }

    #[test]
    fn hiding_and_showing_a_machine_is_a_feature() {
        assert!(FEATURES.contains(&"machine-hide"));
    }

    #[test]
    fn merged_builds_is_a_feature() {
        assert!(FEATURES.contains(&"merged-builds"));
    }

    #[test]
    fn the_boot_of_a_machine_s_ids_is_a_feature() {
        assert!(FEATURES.contains(&"machine-boot"));
    }

    #[test]
    fn an_exited_flow_role_is_a_feature() {
        assert!(FEATURES.contains(&"flow-exited"));
    }

    #[test]
    fn a_failed_message_is_a_feature() {
        assert!(FEATURES.contains(&"message-failed"));
    }

    #[test]
    fn a_flow_exited_message_is_a_feature() {
        assert!(FEATURES.contains(&"message-flow-exited"));
    }

    #[test]
    fn the_shown_bot_and_what_a_bot_is_for_are_features() {
        assert!(FEATURES.contains(&"shown-bot"));
        assert!(FEATURES.contains(&"bot-about"));
    }

    #[test]
    fn the_events_watch_surface_view_app_changes_flow_and_files_streams_are_version_one() {
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
            Stream { name: "files".into(), version: 1 },
            Stream { name: "config-pair".into(), version: 1 },
            Stream { name: "tunnel-orders".into(), version: 1 },
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
            json!([{"name": "events", "version": 1}, {"name": "watch", "version": 1}, {"name": "surface", "version": 1}, {"name": "view", "version": 1}, {"name": "app", "version": 1}, {"name": "changes", "version": 1}, {"name": "flow-runs", "version": 1}, {"name": "flow-messages", "version": 1}, {"name": "files", "version": 1}, {"name": "config-pair", "version": 1}, {"name": "tunnel-orders", "version": 1}])
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
    #[case::queue_routine(json!({"command": "queue", "args": {"action": "add",
        "entry": {"repeat": "day 09:00", "zone": "UTC", "flow": "Quick", "cleanup": true, "prompt": "go"}}}))]
    #[case::queue_pause(json!({"command": "queue", "args": {"action": "pause", "entry": 7}}))]
    #[case::queue_resume(json!({"command": "queue", "args": {"action": "resume", "entry": 7}}))]
    #[case::queue_list(json!({"command": "queue", "args": {"action": "list"}}))]
    #[case::queue_show(json!({"command": "queue-show", "args": {"id": 7, "conversation": false}}))]
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
    #[case::agent_models(json!({"command": "agent-models", "args": {"agent": "opencode"}}))]
    #[case::tunnel_orders(json!({"command": "tunnel-orders", "args": {}}))]
    #[case::tunnel_failed(json!({"command": "tunnel-failed", "args": {"port": 3000, "error": "refused"}}))]
    #[case::contract(json!({"contract": 1, "command": "contract", "args": {}}))]
    #[case::watch(json!({"contract": 1, "boot": "1-2", "command": "watch", "args": {}}))]
    #[case::persona_import_without_its_catalogue(json!({"command": "persona-import", "args": {"personas": [],
        "replace": false, "check": true, "catalogue": false}}))]
    fn a_request_the_server_knows_has_none(#[case] raw: Value) {
        assert_eq!(stray_in(&raw), None);
    }

    #[test]
    fn an_explicit_false_on_a_key_the_request_does_not_know_is_named() {
        let elsewhere = json!({"command": "persona-list", "args": {"catalogue": false}});
        let on_another_import = json!({"command": "flow-import", "args": {"text": "x", "catalogue": false}});
        let other_false = json!({"command": "persona-import", "args": {"personas": [], "listed": false}});

        assert_eq!(stray_in(&elsewhere).as_deref(), Some("args.catalogue"));
        assert_eq!(stray_in(&on_another_import).as_deref(), Some("args.catalogue"));
        assert_eq!(stray_in(&other_false).as_deref(), Some("args.listed"));
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
    #[case::agent_models(json!({"command": "agent-models", "args": {"agent": "opencode"}}))]
    #[case::tunnel_orders(json!({"command": "tunnel-orders", "args": {}}))]
    #[case::tunnel_failed(json!({"command": "tunnel-failed", "args": {"port": 3000, "error": "refused"}}))]
    #[case::contract(json!({"command": "contract", "args": {}}))]
    #[case::watch(json!({"command": "watch", "args": {}}))]
    fn an_extra_argument_is_named(#[case] raw: Value) {
        assert_eq!(stray_in(&with(raw, &["args"], "bogus")).as_deref(), Some("args.bogus"));
    }

    #[test]
    fn an_extra_key_inside_a_queue_entry_is_named_by_its_path() {
        let raw = json!({"command": "queue", "args": {"action": "add", "entry": {"at": "14:30", "zone": "UTC"}}});

        assert_eq!(stray_in(&with(raw, &["args", "entry"], "colour")).as_deref(), Some("args.entry.colour"));
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

mod an_explicit_empty_value {
    use super::*;

    const EMPTY: [Value; 4] = [Value::Bool(false), Value::Null, Value::String(String::new()), Value::Array(Vec::new())];

    fn args_of(name: &str) -> Value {
        match name {
            "close" | "focus" | "collapse" | "expand" => json!({"item": {"pane": 1}}),
            "move" => json!({"item": {"pane": 1}, "before": 2}),
            "todo" => json!("list"),
            "queue" | "machines" | "bot" => json!({"action": "list"}),
            "config" => json!({"action": "get"}),
            "held" => json!({"action": "send-now", "pane": 1}),
            "plugin-run" => json!({"plugin": "a", "action": "b", "revision": 1}),
            "close-agent" => json!({"pane": 1}),
            "focus-machine" => json!({"machine": "vps", "item": {"pane": 1}}),
            "forum-server" => json!({"id": "1"}),
            "agent-models" => json!({"agent": "claude"}),
            "flow-save" => json!({"flow": {"roles": [{"name": "dev"}]}}),
            "persona-import" => json!({"personas": [{"id": "a"}]}),
            _ => json!({}),
        }
    }

    fn request(name: &str) -> Request {
        let raw = json!({"contract": 1, "command": name, "args": args_of(name)});
        serde_json::from_value(raw).unwrap_or_else(|e| panic!("`{name}` reads its fixture: {e}"))
    }

    fn skipped(known: &Value, written: &Value, at: &str, found: &mut Vec<(String, String)>) {
        match (known, written) {
            (Value::Object(known), Value::Object(written)) => {
                known.iter().for_each(|(key, inner)| match written.get(key) {
                    None => found.push((at.to_string(), key.clone())),
                    Some(value) => skipped(inner, value, &format!("{at}/{key}"), found),
                });
            }
            (Value::Array(known), Value::Array(written)) => known
                .iter()
                .zip(written)
                .enumerate()
                .for_each(|(index, (inner, value))| skipped(inner, value, &format!("{at}/{index}"), found)),
            _ => {}
        }
    }

    fn skipped_fields(parsed: &Request) -> (Value, Vec<(String, String)>) {
        let written = serde_json::to_value(parsed).expect("a request serializes");
        let known = every_field::every_field(parsed).expect("a request serializes with every field");
        let mut found = Vec::new();
        skipped(&known, &written, "", &mut found);
        (written, found)
    }

    fn with(mut raw: Value, at: &str, key: &str, value: Value) -> Value {
        raw.pointer_mut(at).and_then(Value::as_object_mut).expect("an object").insert(key.into(), value);
        raw
    }

    fn written_empty(raw: &Value, at: &str, key: &str) -> Option<(Value, Request)> {
        EMPTY.iter().find_map(|empty| {
            let sent = with(raw.clone(), at, key, empty.clone());
            let parsed = serde_json::from_value(sent.clone()).ok()?;
            Some((sent, parsed))
        })
    }

    fn every_skipped_field() -> Vec<(&'static str, String, String)> {
        Command::NAMES
            .iter()
            .flat_map(|name| skipped_fields(&request(name)).1.into_iter().map(|(at, key)| (*name, at, key)))
            .collect()
    }

    #[test]
    fn of_every_skipped_field_of_every_request_is_not_unknown() {
        for (name, at, key) in every_skipped_field() {
            let (raw, _) = skipped_fields(&request(name));
            let (sent, parsed) = written_empty(&raw, &at, &key).unwrap_or_else(|| panic!("`{name}` {at}/{key}"));

            assert_eq!(unknown_fields(&sent, &parsed), None, "{name} {at}/{key}");
        }
    }

    #[test]
    fn of_every_skipped_field_at_once_is_not_unknown() {
        for name in Command::NAMES {
            let (raw, found) = skipped_fields(&request(name));
            let sent =
                found.iter().fold(raw, |raw, (at, key)| written_empty(&raw, at, key).map_or(raw, |(sent, _)| sent));
            let parsed: Request = serde_json::from_value(sent.clone()).expect("a request");

            assert_eq!(unknown_fields(&sent, &parsed), None, "{sent}");
        }
    }

    #[rstest::rstest]
    #[case::close_when_merged("flow-run", "/args", "close_when_merged")]
    #[case::free_builds("flow-close-when-merged", "/args", "free_builds")]
    #[case::catalogue("persona-import", "/args", "catalogue")]
    #[case::source("persona-import", "/args/personas/0", "source")]
    #[case::role_persona("flow-save", "/args/flow/roles/0", "persona")]
    #[case::orchestrator_persona("flow-save", "/args/flow/orchestrator", "persona")]
    #[case::machine("start", "/args", "machine")]
    #[case::cached("stats", "/args", "cached")]
    #[case::item("plugin-run", "/args", "item")]
    #[case::machine_boot("focus-machine", "/args", "machine_boot")]
    fn reaches_the_fields_strict_clients_write(#[case] name: &str, #[case] at: &str, #[case] key: &str) {
        let found = every_skipped_field();

        assert!(found.iter().any(|field| field == &(name, at.to_string(), key.to_string())), "{found:?}");
    }

    #[rstest::rstest]
    #[case::beside_the_command("", "extra")]
    #[case::in_the_arguments("/args", "args.bogus")]
    #[case::on_a_role("/args/flow/roles/0", "args.flow.roles.0.bogus")]
    fn on_a_key_the_request_does_not_know_is_still_named(#[case] at: &str, #[case] path: &str) {
        let key = path.rsplit('.').next().expect("a key");
        for empty in EMPTY {
            let (raw, _) = skipped_fields(&request("flow-save"));
            let sent = with(raw, at, key, empty);
            let parsed: Request = serde_json::from_value(sent.clone()).expect("a request");

            assert_eq!(unknown_fields(&sent, &parsed).as_deref(), Some(path));
        }
    }
}
