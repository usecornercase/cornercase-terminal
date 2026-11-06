use std::time::Duration;

use super::*;
use crate::control::{AppState, Snapshot};
#[cfg(unix)]
use crate::ui::todo as panel;

type Fed = Snapshot<AppState>;

fn follow(app: &mut App) -> Fed {
    app.clients.events.app.set_every(Duration::ZERO);
    let answer = value(app, None, Command::App(wire::AppWatch::default()));
    serde_json::from_value(answer).expect("an app snapshot")
}

fn lines(app: &mut App) -> Vec<Fed> {
    app.trace();
    let streamed = app.take_events();
    assert!(streamed.iter().all(|(client, _)| *client == CLIENT), "{streamed:?}");
    streamed
        .into_iter()
        .filter_map(|(_, streamed)| match streamed {
            Streamed::Snapshot(text) => match serde_json::from_str(&text) {
                Ok(Response::Ok(value)) => Some(value),
                other => panic!("not a snapshot: {other:?}"),
            },
            Streamed::Event(_) | Streamed::End => None,
        })
        .filter(|value: &Value| value.get("todos").is_some())
        .map(|value| serde_json::from_value(value).expect("an app snapshot"))
        .collect()
}

fn next(app: &mut App, after: u64) -> Fed {
    let got = lines(app);
    assert_eq!(got.len(), 1, "one snapshot after revision {after}");
    assert_eq!(got[0].revision, after + 1);
    got.into_iter().next().expect("a snapshot")
}

fn add(app: &mut App, text: &str) -> u64 {
    ok(app, "todo", &json!({"add": text}))["todo"].as_u64().expect("an id")
}

#[test]
fn the_answer_holds_every_part_and_nothing_else_is_sent() {
    let (mut app, _rx) = empty_app();

    let snapshot = follow(&mut app);

    assert!(snapshot.revision >= 1);
    let state = &snapshot.report;
    assert_eq!(state.boot, wire::server_token());
    assert_eq!((state.todos.len(), state.queue.len(), state.keep_awake.clone()), (0, 0, None));
    let away: Vec<(&str, bool, bool)> = state.away.iter().map(|a| (a.platform.as_str(), a.ready, a.away)).collect();
    assert_eq!(away, [("telegram", false, false), ("discord", false, false)]);
    assert_eq!(serde_json::to_value(state.update.none).expect("none"), "disabled");
    assert_eq!(app.take_events(), []);
}

#[test]
fn a_todo_added_by_a_request_sends_the_next_snapshot() {
    let (mut app, _rx) = empty_app();
    let revision = follow(&mut app).revision;

    add(&mut app, "write the docs");

    let snapshot = next(&mut app, revision);
    assert_eq!(snapshot.report.todos.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["write the docs"]);
}

#[cfg(unix)]
#[test]
fn a_change_made_in_the_panel_shows_too() {
    let (mut app, _rx, _dirs) = app_with(1);
    let id = add(&mut app, "write the docs");
    let revision = follow(&mut app).revision;

    let button = app.layout(AREA).todo_button.as_position();
    click(&mut app, button);
    let area = app.layout(AREA).shown(app.screen.nav).changes;
    let row = panel::rows(area, &app.todo_view(area)).item(0);
    click(&mut app, panel::check(row).as_position());

    let snapshot = next(&mut app, revision);
    assert_eq!(snapshot.report.todos.iter().map(|t| (t.id, t.done)).collect::<Vec<_>>(), [(id, true)]);
}

#[cfg(unix)]
#[test]
fn a_queue_entry_shows_as_queue_list_has_it() {
    let (mut app, _rx, _dirs) = app_with(1);
    let revision = follow(&mut app).revision;
    let entry = json!({"at": "in 2h", "zone": "UTC", "agent": "claude", "prompt": "go", "submit": true});
    ok(&mut app, "queue", &json!({"action": "add", "entry": entry}));

    let snapshot = next(&mut app, revision);
    let listed = ok(&mut app, "queue", &json!({"action": "list"}));

    assert_eq!(serde_json::to_value(&snapshot.report.queue).expect("json"), listed["entries"]);
    assert_eq!(snapshot.report.queue.len(), 1);
}

#[test]
fn away_shows_for_each_platform() {
    let (mut app, _rx) = empty_app();
    let _phone = Phone::paired(&mut app);
    let revision = follow(&mut app).revision;

    ok(&mut app, "away", &json!({}));

    let away = next(&mut app, revision).report.away;
    let rows: Vec<(&str, bool, bool)> = away.iter().map(|a| (a.platform.as_str(), a.ready, a.away)).collect();
    assert_eq!(rows, [("telegram", true, true), ("discord", false, false)]);
}

#[test]
fn nothing_is_built_while_nobody_follows() {
    let (mut app, _rx) = empty_app();
    app.clients.events.app.set_every(Duration::ZERO);

    for text in ["one", "two", "three"] {
        add(&mut app, text);
        app.trace();
    }

    assert_eq!(app.take_events(), []);
    assert_eq!(follow(&mut app).revision, 1);
}

#[test]
fn nothing_changed_sends_nothing() {
    let (mut app, _rx) = empty_app();
    let revision = follow(&mut app).revision;

    app.trace();
    app.trace();

    assert_eq!(lines(&mut app), []);
    add(&mut app, "x");
    assert_eq!(next(&mut app, revision).revision, revision + 1);
}

#[test]
fn a_change_inside_the_pace_comes_when_it_ends() {
    let (mut app, _rx) = empty_app();
    follow(&mut app);
    app.clients.events.app.set_every(Duration::from_secs(60));
    let revision = follow_revision(&mut app);
    add(&mut app, "later");

    assert_eq!(lines(&mut app), []);
    let wait = app.tick_at(Instant::now(), 0);
    assert!(wait.is_some_and(|wait| wait <= Duration::from_secs(60)), "{wait:?}");

    app.clients.events.app.set_built(Instant::now().checked_sub(Duration::from_secs(61)).expect("a past instant"));
    assert_eq!(next(&mut app, revision).revision, revision + 1);
}

fn follow_revision(app: &mut App) -> u64 {
    app.clients.events.app.revision()
}

#[test]
fn a_client_that_leaves_is_forgotten() {
    let (mut app, _rx) = empty_app();
    follow(&mut app);
    app.forget(CLIENT);

    add(&mut app, "unseen");

    assert_eq!(lines(&mut app), []);
    let fresh = follow(&mut app);
    assert_eq!(fresh.report.todos.len(), 1);
}

#[test]
fn the_session_feed_keeps_its_shape() {
    let (mut app, _rx) = empty_app();
    let status = value(&mut app, None, Command::Status(wire::Status::default()));
    let watch = value(&mut app, None, Command::Watch(wire::Watch::default()));

    for answer in [status, watch] {
        for key in ["todos", "queue", "away", "update"] {
            assert!(answer.get(key).is_none(), "{key} in {answer}");
        }
    }
}

mod accounts_revision {
    use super::*;
    use crate::app::worktrees::SYNC_EVERY;

    fn server() -> (App, Receiver<AppEvent>, TempDir) {
        let config = TempDir::new();
        let (app, rx) = new_app(config.path().join("config.json"));
        (app, rx, config)
    }

    fn folder_of_an_account(config: &TempDir) -> PathBuf {
        config.path().join("accounts").join("gemini").join("probe")
    }

    fn add_probe(app: &mut App) {
        ok(app, "account-add", &json!({"provider": "gemini", "name": "probe"}));
    }

    #[test]
    fn a_server_without_accounts_leaves_it_out_after_its_first_sync_step() {
        let (mut app, _rx, _config) = server();

        app.refresh(Instant::now());

        let state = serde_json::to_value(app.app_state()).expect("json");
        assert!(state.get("accounts_revision").is_none(), "{state}");
    }

    #[test]
    fn a_server_that_starts_with_accounts_has_it_from_its_first_sync_step() {
        let (mut app, _rx, config) = server();
        std::fs::create_dir_all(folder_of_an_account(&config)).expect("the account's folder");

        app.refresh(Instant::now());

        assert_eq!(follow(&mut app).report.accounts_revision, 1);
    }

    #[test]
    fn the_first_account_of_a_command_sends_a_line_that_moves_it_from_nothing() {
        let (mut app, _rx, _config) = server();
        app.refresh(Instant::now());
        let revision = follow(&mut app).revision;

        add_probe(&mut app);

        assert_eq!(next(&mut app, revision).report.accounts_revision, 1);
    }

    #[test]
    fn removing_an_account_no_setting_names_sends_a_line_too() {
        let (mut app, _rx, _config) = server();
        add_probe(&mut app);
        let config_revision = app.config_file.revision();
        let followed = follow(&mut app);

        ok(&mut app, "account-remove", &json!({"provider": "gemini", "name": "probe"}));

        let line = next(&mut app, followed.revision);
        assert_eq!((followed.report.accounts_revision, line.report.accounts_revision), (1, 2));
        assert_eq!(app.config_file.revision(), config_revision);
    }

    #[test]
    fn a_folder_another_program_makes_moves_it_on_the_sync_step() {
        let (mut app, _rx, config) = server();
        let start = Instant::now();
        app.refresh(start);
        let revision = follow(&mut app).revision;

        std::fs::create_dir_all(folder_of_an_account(&config)).expect("the account's folder");
        app.refresh(start + SYNC_EVERY);

        assert_eq!(next(&mut app, revision).report.accounts_revision, 1);
    }

    #[test]
    fn a_name_or_a_default_leaves_it_to_the_config_revision() {
        let (mut app, _rx, _config) = server();
        add_probe(&mut app);
        let config_revision = app.config_file.revision();

        ok(&mut app, "account-use", &json!({"provider": "gemini", "account": "probe"}));
        ok(&mut app, "account-rename", &json!({"provider": "gemini", "account": "probe", "label": "Probe"}));

        assert_eq!(app.app_state().accounts_revision, 1);
        assert!(app.config_file.revision() > config_revision);
    }
}
