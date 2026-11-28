use super::*;
use crate::test_util::phone_secret;

const LOCALHOST: std::net::IpAddr = std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);

struct Rig {
    app: App,
    rx: Receiver<AppEvent>,
    port: u16,
    _dir: TempDir,
}

fn on() -> Rig {
    let dir = TempDir::new();
    let (mut app, rx) = new_app(dir.path().join("config.json"));
    app.phone.set_probe(|| vec![LOCALHOST]);
    let port = free_port();
    app.config.phone_app_port = port;
    app.config.phone_app = true;
    app.refresh(Instant::now());
    Rig { app, rx, port, _dir: dir }
}

fn url(line: &Value) -> String {
    line["url"].as_str().expect("an address").to_string()
}

fn fingerprint(line: &Value) -> String {
    let url = url(line);
    crate::test_util::tls::fingerprint_in(&url).expect("a fingerprint").to_string()
}

fn apps_listed(app: &mut App) -> usize {
    let listed = ok(app, "config", &json!({"action": "get"}));
    let connections = listed["connections"].as_array().cloned().unwrap_or_default();
    connections.iter().filter(|c| c["kind"] == "app").count()
}

#[test]
fn it_answers_with_the_address_and_the_lines_and_ends_with_paired() {
    let mut r = on();

    let line = shown(&mut r.app, FIRST, "app");
    let secret = phone_secret(&url(&line));
    pump_until(&mut r.app, &r.rx, "the pairing is told", |app| toast(app).is_some());

    assert!(url(&line).starts_with(&format!("wss://127.0.0.1:{}/?code=", r.port)), "{line}");
    assert_eq!(fingerprint(&line).len(), 43, "{line}");
    assert_eq!(line["lines"][0], "scan the code with the cornercase app on your phone.");
    assert_eq!(lines(&mut r.app), ended(FIRST, &json!({"paired": true})));
    assert_eq!(toast(&r.app), Some("a phone app connected"));
    assert!(secret.contains('.'));
}

#[test]
fn switching_it_off_ends_it_with_the_reason() {
    let mut r = on();
    shown(&mut r.app, FIRST, "app");

    r.app.config.phone_app = false;
    r.app.refresh(Instant::now());

    let got = lines(&mut r.app);
    assert_eq!(got.len(), 2, "{got:?}");
    let ended = got[0].1.clone().expect("the last line");
    assert_eq!(ended["paired"], false);
    assert!(ended["error"].as_str().is_some_and(|error| error.contains("phone app's access is off")), "{ended}");
}

#[test]
fn a_second_follower_on_one_connection_is_a_conflict() {
    let mut r = on();
    shown(&mut r.app, FIRST, "app");

    let again = pair(&mut r.app, FIRST, "app").expect_err("refused");

    assert_eq!(again.code, "conflict");
}

#[test]
fn it_is_refused_while_it_is_off() {
    let dir = TempDir::new();
    let (mut app, _rx) = new_app(dir.path().join("config.json"));

    let (code, message) = refusal(&mut app, "app");

    assert_eq!(code, "conflict");
    assert!(
        message.contains("phone app's access is off") && message.contains("config set phone_app true"),
        "{message}"
    );
}

#[test]
fn the_routes_are_where_it_listens_and_a_conflict_while_it_is_off() {
    let mut r = on();

    let routes = ok(&mut r.app, "app-routes", &json!({}));
    let shown = fingerprint(&shown(&mut r.app, FIRST, "app"));
    r.app.config.phone_app = false;
    r.app.refresh(Instant::now());
    let off = refused(&mut r.app, "app-routes", &json!({}));

    assert_eq!(routes, json!({"routes": [{"host": "127.0.0.1", "port": r.port}], "fingerprint": shown}));
    assert_eq!(off.code, "conflict");
}

#[test]
fn config_lists_the_paired_app_and_remove_forgets_it() {
    let mut r = on();
    let secret = phone_secret(&url(&shown(&mut r.app, FIRST, "app")));
    pump_until(&mut r.app, &r.rx, "the pairing is told", |app| toast(app).is_some());
    let (id, _) = secret.split_once('.').expect("an id");
    let key = format!("app.{id}");

    let listed = ok(&mut r.app, "config", &json!({"action": "get"}));
    let connection = listed["connections"].as_array().and_then(|all| all.iter().find(|c| c["key"] == key.as_str()));
    let removed = ok(&mut r.app, "config", &json!({"action": "remove", "entry": key}));
    let after = ok(&mut r.app, "config", &json!({"action": "get"}));

    assert_eq!(connection.map(|c| c["kind"].clone()), Some(json!("app")));
    assert!(listed["settings"].as_array().is_some_and(|all| all.iter().any(|s| s["key"] == "phone_app")));
    assert!(removed["notices"].to_string().contains("phone app was forgotten"), "{removed}");
    assert!(after["connections"].as_array().is_some_and(|all| all.iter().all(|c| c["key"] != key.as_str())));
}

fn set_phone_app(app: &mut App) -> crate::control::config::Described {
    let got = ok(app, "config", &json!({"action": "set", "entry": {"phone_app": true}}));
    serde_json::from_value(got).expect("a description")
}

#[test]
fn a_set_of_phone_app_answers_the_note_the_listener_settled_on() {
    let dir = TempDir::new();
    let (mut app, _rx) = new_app(dir.path().join("config.json"));
    app.phone.set_probe(|| vec![LOCALHOST]);
    app.config.phone_app_port = free_port();

    let got = set_phone_app(&mut app);
    app.refresh(Instant::now());

    assert_ne!(*app.phone.phase(), crate::app::phone_app::Phase::Off);
    let note = got.settings.iter().find(|s| s.key == "phone_app").map(|s| s.note.clone());
    assert_eq!(note, Some(app.phone.note()));
    assert_eq!(got.revision, app.config_file.revision());
}

mod a_new_certificate {
    use super::*;

    fn renewed(app: &mut App) -> Value {
        ok(app, "config-pair", &json!({"key": "app", "new_certificate": true}))
    }

    #[test]
    fn changes_the_fingerprint_and_forgets_every_paired_app() {
        let mut r = on();
        let before = shown(&mut r.app, FIRST, "app");
        phone_secret(&url(&before));
        pump_until(&mut r.app, &r.rx, "the pairing is told", |app| toast(app).is_some());
        let (paired, revision) = (apps_listed(&mut r.app), r.app.config_file.revision());

        let after = renewed(&mut r.app);

        assert_eq!(paired, 1);
        assert_ne!(fingerprint(&after), fingerprint(&before));
        assert_eq!(apps_listed(&mut r.app), 0);
        assert!(r.app.config_file.revision() > revision);
        let routes = ok(&mut r.app, "app-routes", &json!({}));
        assert_eq!(routes["fingerprint"].as_str(), Some(fingerprint(&after).as_str()));
        assert!(phone_secret(&url(&after)).contains('.'));
    }

    #[test]
    fn goes_with_the_key_app_only() {
        let mut r = on();

        let refused = refused(&mut r.app, "config-pair", &json!({"key": "browsers", "new_certificate": true}));

        assert_eq!(refused.code, "malformed");
        assert!(refused.message.contains("`new_certificate`"), "{}", refused.message);
    }

    #[test]
    fn is_refused_while_it_is_off_and_replaces_nothing() {
        let dir = TempDir::new();
        let (mut app, _rx) = new_app(dir.path().join("config.json"));

        let refused = refused(&mut app, "config-pair", &json!({"key": "app", "new_certificate": true}));

        assert_eq!(refused.code, "conflict");
        assert!(!dir.path().join("app-certificate.json").exists());
    }
}
