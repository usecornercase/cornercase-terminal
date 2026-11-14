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

#[test]
fn it_answers_with_the_address_and_the_lines_and_ends_with_paired() {
    let mut r = on();

    let line = shown(&mut r.app, FIRST, "app");
    let secret = phone_secret(&url(&line));
    pump_until(&mut r.app, &r.rx, "the pairing is told", |app| toast(app).is_some());

    assert!(url(&line).starts_with(&format!("ws://127.0.0.1:{}/?code=", r.port)), "{line}");
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
    r.app.config.phone_app = false;
    r.app.refresh(Instant::now());
    let off = refused(&mut r.app, "app-routes", &json!({}));

    assert_eq!(routes, json!({"routes": [{"host": "127.0.0.1", "port": r.port}]}));
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
