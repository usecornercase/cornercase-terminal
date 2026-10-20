use serde_json::Value;

use super::*;
use crate::control::{Snapshot, TodoItem, contract};
use crate::queue::{Entry, Place, State, Task};
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
            queue: vec![queued()],
            away: vec![
                AwayInfo { platform: "telegram".into(), ready: true, away: true },
                AwayInfo { platform: "discord".into(), ready: false, away: false },
            ],
            update: update_info(),
            keep_awake: Some(KeepAwake { held: true, by: Some("caffeinate".into()), unavailable: None }),
            profile: None,
            accounts: BTreeMap::new(),
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
    assert_eq!(keys, ["away", "boot", "keep_awake", "queue", "todos", "update", "version"]);
    assert_eq!(value["keep_awake"], Value::Null);
    let update = &value["update"];
    assert_eq!(
        (&update["release"], &update["none"], &update["checked"], &update["error"]),
        (&Value::Null, &Value::Null, &Value::Null, &Value::Null)
    );
}
