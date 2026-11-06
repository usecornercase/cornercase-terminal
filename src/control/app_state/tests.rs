use serde_json::Value;

use super::*;
use crate::control::{Snapshot, TodoItem, contract};
use crate::queue::{Entry, Listed, Place, State, Task};
use crate::test_util::fixture;

fn queued() -> Entry {
    Entry {
        id: 3,
        task: Task {
            at: 1_760_000_000,
            zone: "UTC".into(),
            label: "Fix the login".into(),
            agent: "claude".into(),
            command: "claude".into(),
            program: "claude".into(),
            mode: None,
            prompt: "Fix the login".into(),
            submit: true,
            place: Place { project: "/home/me/web-shop".into(), ..Place::default() },
            accounts: BTreeMap::new(),
            ..Task::default()
        },
        state: State::Pending,
    }
}

fn update_info() -> UpdateInfo {
    UpdateInfo {
        release: Some(ReleaseInfo {
            version: "9.0.0".into(),
            notes: vec![
                Notes { version: "9.0.0".into(), text: "- Faster.".into() },
                Notes { version: "8.0.0".into(), text: "- Skipped one.".into() },
            ],
        }),
        none: None,
        checking: false,
        checked: Some("2026-10-18T10:00:00.000Z".into()),
        error: None,
        install: InstallInfo::Replace { path: "/home/me/.local/bin/cornercase".into() },
        installing: false,
        installed: false,
        install_error: None,
    }
}

fn snapshot() -> Snapshot<AppState> {
    Snapshot {
        revision: 4,
        report: AppState {
            version: "0.12.141".into(),
            boot: "4242-1760600000".into(),
            todos: vec![
                TodoItem { id: 1, text: "write the docs".into(), done: false },
                TodoItem { id: 2, text: "review the login fix".into(), done: true },
            ],
            queue: vec![Listed::from(&queued())],
            away: vec![
                AwayInfo { platform: "telegram".into(), ready: true, away: true, forum: None },
                AwayInfo { platform: "discord".into(), ready: false, away: false, forum: None },
            ],
            update: update_info(),
            keep_awake: Some(KeepAwake { held: true, by: Some("caffeinate".into()), unavailable: None }),
            tour: crate::control::tour::TourState {
                active: Some(crate::control::tour::ActiveInfo {
                    tour: "core".into(),
                    step: "tabs".into(),
                    index: 2,
                    card: true,
                }),
                offered: true,
                progress: vec![crate::control::tour::ProgressInfo {
                    tour: "core".into(),
                    done: vec!["projects".into(), "workspaces".into()],
                    finished: false,
                }],
            },
            profile: None,
            accounts: BTreeMap::new(),
            popup: None,
            machines: Vec::new(),
        },
    }
}

#[test]
fn the_fixture_is_of_the_app_version_the_contract_names() {
    assert_eq!(contract::APP_VERSION, 1);
}

#[test]
fn a_snapshot_is_written_as_its_fixture() {
    let written = serde_json::to_string_pretty(&snapshot()).expect("json");

    assert_eq!(written, fixture!("control/app/v1/snapshot.json").trim_end());
}

#[test]
fn the_fixture_reads_back_as_its_snapshot() {
    let read: Snapshot<AppState> = serde_json::from_str(fixture!("control/app/v1/snapshot.json")).expect("a snapshot");

    assert_eq!(read, snapshot());
}

#[test]
fn every_key_is_there_when_there_is_nothing() {
    let value = serde_json::to_value(AppState::default()).expect("json");

    let keys: Vec<&str> = value.as_object().expect("an object").keys().map(String::as_str).collect();
    assert_eq!(keys, ["away", "boot", "keep_awake", "queue", "todos", "tour", "update", "version"]);
    assert_eq!(value["keep_awake"], Value::Null);
    let update = &value["update"];
    assert_eq!(
        (&update["release"], &update["none"], &update["checked"], &update["error"]),
        (&Value::Null, &Value::Null, &Value::Null, &Value::Null)
    );
}

#[test]
fn an_open_popup_names_its_plugin_action_and_size() {
    let popup = PopupInfo { plugin: "acme.notes".into(), action: "status".into(), cols: 100, rows: 30 };
    let value = serde_json::to_value(AppState { popup: Some(popup.clone()), ..AppState::default() }).expect("json");

    assert_eq!(
        value["popup"],
        serde_json::json!({"plugin": "acme.notes", "action": "status", "cols": 100, "rows": 30})
    );
    let read: AppState = serde_json::from_value(value).expect("an app state");
    assert_eq!(read.popup, Some(popup));
}

#[test]
fn the_machines_are_there_only_when_some_are_listed() {
    let vps = Info { name: "vps".into(), icon: "☁".into(), ..Info::default() };
    let value = serde_json::to_value(AppState { machines: vec![vps.clone()], ..AppState::default() }).expect("json");

    assert_eq!(
        (value["machines"][0]["name"].as_str(), value["machines"][0]["state"].as_str()),
        (Some("vps"), Some("connecting"))
    );
    let read: AppState = serde_json::from_value(value).expect("an app state");
    assert_eq!(read.machines, [vps]);
}

mod the_forum {
    use serde_json::json;

    use super::*;
    use crate::control::{Command, Request};

    fn asked_to_pick() -> AwayInfo {
        AwayInfo {
            platform: "discord".into(),
            ready: true,
            away: false,
            forum: Some(ForumInfo {
                on: true,
                pending: Some("pick the server for the forum".into()),
                servers: vec![
                    ForumServerInfo { id: "1041234567890123457".into(), name: "home".into() },
                    ForumServerInfo { id: "1041234567890123459".into(), name: "work".into() },
                ],
                ..ForumInfo::default()
            }),
        }
    }

    #[test]
    fn asked_to_pick_is_written_as_its_fixture() {
        let written = serde_json::to_string_pretty(&asked_to_pick()).expect("json");

        assert_eq!(written, fixture!("control/app/v1/forum-servers.json").trim_end());
    }

    #[test]
    fn asked_to_pick_reads_back_from_its_fixture() {
        let read: AwayInfo = serde_json::from_str(fixture!("control/app/v1/forum-servers.json")).expect("a row");

        assert_eq!(read, asked_to_pick());
    }

    #[test]
    fn leaves_the_servers_out_when_there_are_none() {
        let value = serde_json::to_value(ForumInfo::default()).expect("json");

        assert_eq!(value, json!({"on": false, "ready": false, "guild": null, "channel": null, "pending": null}));
    }

    #[test]
    fn of_an_older_server_reads_without_servers() {
        let text = r#"{"on":true,"ready":false,"guild":null,"channel":null,"pending":"pick the server for the forum"}"#;

        let read: ForumInfo = serde_json::from_str(text).expect("a forum");

        assert_eq!((read.pending.as_deref(), read.servers), (Some("pick the server for the forum"), Vec::new()));
    }

    #[test]
    fn is_picked_by_an_id_given_as_text() {
        let sent = json!({"command": "forum-server", "args": {"id": "1041234567890123459"}});

        let request: Request = serde_json::from_value(sent).expect("a request");

        let pick = ForumServer { id: "1041234567890123459".into() };
        assert_eq!(request.command, Command::ForumServer(pick));
        assert_eq!(request.command.name(), "forum-server");
        assert_eq!(request.command.fields(), [("id", "1041234567890123459".to_string())]);
    }

    #[rstest::rstest]
    #[case::no_id(json!({}))]
    #[case::an_id_as_a_number(json!({"id": 8}))]
    fn is_not_picked_by_anything_else(#[case] args: Value) {
        let sent = json!({"command": "forum-server", "args": args});

        assert!(serde_json::from_value::<Request>(sent).is_err());
    }
}
