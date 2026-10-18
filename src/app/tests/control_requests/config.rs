use serde_json::{Value, json};

use super::*;
use crate::control::config::Described;
use crate::issues::secrets;
use crate::test_util::{SHORTCUT_MEMBER, git_repo};

const MARKER: &str = "t0k-marker";

struct Setup {
    app: App,
    rx: Receiver<AppEvent>,
    config: TempDir,
    folder: TempDir,
}

fn setup() -> Setup {
    let (config, folder) = (TempDir::new(), TempDir::new());
    let (mut app, rx) = app_in(folder.path(), config.path().join("config.json"));
    app.issues.env_tokens.clear();
    Setup { app, rx, config, folder }
}

fn call(s: &mut Setup, args: &Value) -> std::result::Result<Value, Refused> {
    asked(&mut s.app, &s.rx, "config", args)
}

fn ok(s: &mut Setup, args: &Value) -> Described {
    match call(s, args) {
        Ok(value) => serde_json::from_value(value).expect("a description"),
        Err(e) => panic!("refused: {} {}", e.code, e.message),
    }
}

fn refused(s: &mut Setup, args: &Value) -> Refused {
    match call(s, args) {
        Ok(value) => panic!("taken: {value}"),
        Err(refused) => refused,
    }
}

fn set(s: &mut Setup, entry: &Value) -> Described {
    ok(s, &json!({"action": "set", "entry": entry}))
}

fn file(s: &Setup) -> Value {
    let text = std::fs::read_to_string(s.config.path().join("config.json")).unwrap_or_else(|_| "{}".into());
    serde_json::from_str(&text).expect("json")
}

fn setting<'a>(described: &'a Described, key: &str) -> &'a crate::control::config::Setting {
    described.settings.iter().find(|s| s.key == key).expect("a setting")
}

fn shortcut(s: &mut Setup, status: u16) -> crate::test_util::FakeHttp {
    fake_shortcut(&mut s.app, vec![("GET /api/v3/member", status, SHORTCUT_MEMBER)])
}

#[test]
fn get_lists_the_settings_with_their_values_defaults_and_notes() {
    let mut s = setup();
    let got = ok(&mut s, &json!({"action": "get"}));
    let fetch = setting(&got, "fetch_minutes");
    assert_eq!((fetch.value.clone(), fetch.default.clone()), (json!(5), json!(5)));
    assert_eq!(setting(&got, "sidebar").choices.len(), 4);
    assert!(got.connections.iter().any(|c| c.key == "shortcut"));
    assert_eq!(got.revision, s.app.report(None).config.expect("a revision").revision);
}

#[test]
fn a_set_is_applied_live_and_saved() {
    let mut s = setup();
    let before = s.app.report(None).config.expect("a revision").revision;

    let got = set(&mut s, &json!({"keep_awake": true, "sidebar": "tree"}));

    assert_eq!(got.notices.len(), 2, "{:?}", got.notices);
    assert_eq!((file(&s)["keep_awake"].clone(), file(&s)["sidebar"].clone()), (json!(true), json!("tree")));
    s.app.refresh(Instant::now() + Duration::from_secs(2));
    assert!(s.app.report(None).keep_awake.is_some());
    assert!(s.app.layout(AREA).tree);
    assert!(s.app.report(None).config.expect("a revision").revision > before);
}

#[test]
fn a_refused_value_changes_nothing() {
    let mut s = setup();
    let refusal = refused(&mut s, &json!({"action": "set", "entry": {"counts": false, "worktrees_dir": "rel"}}));
    assert_eq!(
        (refusal.code.as_str(), refusal.message.as_str()),
        ("malformed", "use an absolute path or one starting with ~/")
    );
    assert!(s.app.config.counts && !s.config.path().join("config.json").exists());

    assert_eq!(refused(&mut s, &json!({"action": "set", "entry": {"nonsense": 1}})).code, "not-found");
    assert_eq!(refused(&mut s, &json!({"action": "set", "entry": {"linear": "x"}})).code, "malformed");
    assert_eq!(refused(&mut s, &json!({"action": "set", "entry": {"counts": "no"}})).code, "malformed");
}

#[test]
fn a_stray_key_is_an_unknown_field() {
    let mut s = setup();
    let refusal = refused(&mut s, &json!({"action": "get", "stray": 1}));
    assert_eq!(refusal.code, "unknown-field");
}

#[test]
fn a_token_is_checked_saved_and_never_echoed() {
    let mut s = setup();
    let server = shortcut(&mut s, 200);

    let answer = call(&mut s, &json!({"action": "secret", "entry": {"key": "shortcut", "value": MARKER}}));

    let value = answer.unwrap_or_else(|e| panic!("refused {} {}", e.code, e.message));
    assert!(!value.to_string().contains(MARKER));
    let got: Described = serde_json::from_value(value).expect("a description");
    let connection = got.connections.iter().find(|c| c.key == "shortcut").expect("shortcut");
    assert_eq!(connection.state, crate::control::config::State::Connected);
    assert!(connection.account.is_some());
    assert!(got.notices[0].starts_with("Shortcut connected as"), "{:?}", got.notices);
    let saved = paths::secrets(&s.config.path().join("config.json"));
    assert_eq!(secrets::read(&saved, "shortcut_token").as_deref(), Some(MARKER));
    assert!(server.request(0).to_lowercase().contains(MARKER));
}

#[test]
fn a_rejected_token_is_not_saved_and_not_echoed() {
    let mut s = setup();
    let _server = shortcut(&mut s, 401);

    let refusal = refused(&mut s, &json!({"action": "secret", "entry": {"key": "shortcut", "value": MARKER}}));

    assert_eq!(refusal.code, "failed");
    assert!(!refusal.message.contains(MARKER), "{}", refusal.message);
    let saved = paths::secrets(&s.config.path().join("config.json"));
    assert_eq!(secrets::read(&saved, "shortcut_token"), None);
}

#[test]
fn a_token_from_the_environment_cannot_be_replaced_or_removed() {
    let mut s = setup();
    s.app.issues.env_tokens.insert(Source::Linear, "from-env".into());

    let replaced = refused(&mut s, &json!({"action": "secret", "entry": {"key": "linear", "value": MARKER}}));
    let removed = refused(&mut s, &json!({"action": "remove", "entry": "linear"}));

    assert_eq!((replaced.code.as_str(), removed.code.as_str()), ("conflict", "conflict"));
}

#[test]
fn a_token_is_removed_and_removing_it_again_says_it_is_not_set() {
    let mut s = setup();
    let _server = shortcut(&mut s, 200);
    ok(&mut s, &json!({"action": "secret", "entry": {"key": "shortcut", "value": MARKER}}));

    let removed = ok(&mut s, &json!({"action": "remove", "entry": "shortcut"}));
    assert_eq!(removed.notices, ["the Shortcut API token was removed"]);
    let saved = paths::secrets(&s.config.path().join("config.json"));
    assert_eq!(secrets::read(&saved, "shortcut_token"), None);

    let again = ok(&mut s, &json!({"action": "remove", "entry": "shortcut"}));
    assert_eq!(again.notices, ["`shortcut` is not set"]);
}

#[test]
fn project_settings_are_set_listed_and_cleared() {
    let repo = git_repo(&[("a", "1")]);
    let mut s = setup();
    s.app.open_project(repo.path().to_path_buf(), AREA).expect("open the repository");
    let id = s.app.projects.last().expect("a project").id;

    let got = ok(&mut s, &json!({"action": "project", "entry": {"project": id, "setup": "npm ci"}}));
    let entry = got.projects.iter().find(|p| p.project == Some(id)).expect("the project");
    assert_eq!((entry.setup.as_str(), entry.repository), ("npm ci", true));
    assert_eq!(s.app.config.project(repo.path(), None).setup, "npm ci");

    let cleared = ok(&mut s, &json!({"action": "project", "entry": {"project": id, "setup": ""}}));
    assert!(cleared.projects.iter().find(|p| p.project == Some(id)).is_some_and(|p| p.setup.is_empty()));
    assert!(s.app.config.projects.is_empty());
}

#[test]
fn project_commands_need_a_repository() {
    let mut s = setup();
    let path = s.folder.path().display().to_string();
    let refusal = refused(&mut s, &json!({"action": "project", "entry": {"path": path, "setup": "make"}}));
    assert_eq!(refusal.code, "conflict");
    assert_eq!(refused(&mut s, &json!({"action": "project", "entry": {}})).code, "malformed");
    assert_eq!(
        refused(&mut s, &json!({"action": "project", "entry": {"project": 9999, "setup": ""}})).code,
        "not-found"
    );
}

#[test]
fn a_file_that_cannot_be_read_refuses_changes() {
    let mut s = setup();
    std::fs::write(s.config.path().join("config.json"), "{").expect("write");

    let refusal = refused(&mut s, &json!({"action": "set", "entry": {"counts": false}}));

    assert_eq!(refusal.code, "conflict");
    let got = ok(&mut s, &json!({"action": "get"}));
    assert!(got.problem.is_some());
}

#[test]
fn a_watcher_sees_the_revision_grow_when_a_setting_changes() {
    let mut s = setup();
    s.app.events.snapshots.every = Duration::ZERO;
    let first: crate::control::Snapshot =
        serde_json::from_value(value(&mut s.app, None, Command::Watch(wire::Watch::default()))).expect("a snapshot");
    let before = first.report.config.expect("a revision").revision;

    set(&mut s, &json!({"counts": false}));
    s.app.trace();
    s.app.refresh(Instant::now());

    let mut revisions = Vec::new();
    for (_, streamed) in s.app.take_events() {
        if let Streamed::Snapshot(text) = streamed {
            let Ok(Response::Ok(value)) = serde_json::from_str(&text) else { panic!("a snapshot") };
            let snapshot: crate::control::Snapshot = serde_json::from_value(value).expect("a snapshot");
            revisions.push(snapshot.report.config.expect("a revision").revision);
        }
    }
    assert!(revisions.iter().any(|r| *r > before), "{revisions:?} after {before}");
}
