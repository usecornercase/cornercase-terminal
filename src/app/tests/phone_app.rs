use std::net::{IpAddr, Ipv4Addr, TcpListener, TcpStream};
use std::sync::Arc;

use parking_lot::Mutex;
use tungstenite::client::IntoClientRequest as _;

use super::*;
use crate::app::phone_app::Phase;
use crate::control::contract::Code;
use crate::settings::Row;
use crate::test_util::{free_port, phone_secret, wait_until};

const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

struct Rig {
    app: App,
    rx: Receiver<AppEvent>,
    addresses: Arc<Mutex<Vec<IpAddr>>>,
    port: u16,
    at: Instant,
    _dir: TempDir,
}

fn rig() -> Rig {
    let dir = TempDir::new();
    let (mut app, rx) = new_app(dir.path().join("config.json"));
    let addresses = Arc::new(Mutex::new(vec![LOCALHOST]));
    let seen = Arc::clone(&addresses);
    app.phone.set_probe(move || seen.lock().clone());
    let port = free_port();
    app.config.phone_app_port = port;
    Rig { app, rx, addresses, port, at: Instant::now(), _dir: dir }
}

fn on() -> Rig {
    let mut r = rig();
    r.app.config.phone_app = true;
    listening(&mut r, Instant::now());
    r
}

/// Refreshes at `at`, then a read later each time, until the listener has its port, as the app tries again
/// at its next read. A child that a test beside this one forks holds a copy of every socket until it runs
/// its program, so a port can be taken for a moment (`web::bind` waits for the same reason).
fn listening(r: &mut Rig, at: Instant) {
    r.at = at;
    r.app.refresh(r.at);
    wait_until("the phone app listens", || {
        if matches!(r.app.phone.phase(), Phase::Listening(_)) {
            return true;
        }
        r.at += Duration::from_secs(2);
        r.app.refresh(r.at);
        false
    });
}

fn dialog(app: &App) -> Option<ui::RemoteAccess> {
    match app.overlay_view(app.overlay.as_ref()?, AREA)? {
        ui::Overlay::Remote(remote) => Some(remote),
        _ => None,
    }
}

fn refused(port: u16, secret: &str) -> bool {
    let mut request = format!("ws://127.0.0.1:{port}/cli").into_client_request().expect("a request");
    request.headers_mut().insert("Authorization", format!("Bearer {secret}").parse().expect("a header"));
    let stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    matches!(
        tungstenite::client::client(request, stream),
        Err(tungstenite::HandshakeError::Failure(tungstenite::Error::Http(response))) if response.status() == 403
    )
}

fn open_dialog(r: &mut Rig) -> String {
    r.app.open_phone_app_code();
    dialog(&r.app).and_then(|view| view.url).expect("an address")
}

fn settings_rows(app: &App) -> Vec<Row> {
    let Some(Overlay::Settings(s)) = &app.overlay else { panic!("the settings are not open") };
    s.rows()
}

fn open_remote_settings(app: &mut App) {
    from_more(app, "settings");
    let names = Page::ALL.map(Page::name);
    let at = Page::ALL.iter().position(|page| *page == Page::Remote).expect("the remote page");
    click(app, ui::settings_tabs(ui::settings_area(AREA), &names)[at].as_position());
}

fn enter_on(app: &mut App, row: &Row) {
    let target = settings_rows(app).iter().position(|r| r == row).expect("the row is there");
    loop {
        let Some(Overlay::Settings(s)) = &app.overlay else { panic!("the settings are not open") };
        let key = match s.cursor.cmp(&target) {
            std::cmp::Ordering::Less => KeyCode::Down,
            std::cmp::Ordering::Greater => KeyCode::Up,
            std::cmp::Ordering::Equal => break,
        };
        send_key(app, key, KeyModifiers::NONE);
    }
    send_key(app, KeyCode::Enter, KeyModifiers::NONE);
}

#[test]
fn switching_it_on_listens_and_says_where_and_switching_it_off_stops() {
    let mut r = on();

    assert_eq!(r.app.phone.note(), format!("ready at 127.0.0.1 (port {})", r.port));
    assert!(TcpStream::connect(("127.0.0.1", r.port)).is_ok());
    r.app.config.phone_app = false;
    r.app.refresh(Instant::now());

    assert_eq!(*r.app.phone.phase(), Phase::Off);
    wait_until("the port stops accepting", || TcpStream::connect(("127.0.0.1", r.port)).is_err());
}

#[test]
fn it_does_not_listen_until_it_is_switched_on() {
    let mut r = rig();
    r.app.refresh(Instant::now());

    assert_eq!(*r.app.phone.phase(), Phase::Off);
    assert!(TcpStream::connect(("127.0.0.1", r.port)).is_err());
}

#[test]
fn a_port_somebody_holds_is_named_in_the_note_and_refuses_a_code() {
    let taken = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let mut r = rig();
    r.app.config.phone_app_port = taken.local_addr().expect("address").port();
    r.app.config.phone_app = true;
    r.app.refresh(Instant::now());

    let refusal = r.app.request_app_code(Instant::now()).expect_err("refused");

    assert!(r.app.phone.note().contains("phone_app_port"), "{}", r.app.phone.note());
    assert_eq!(refusal.code, Code::Unavailable);
}

#[test]
fn a_code_is_refused_while_it_is_off() {
    let mut r = rig();

    let refusal = r.app.request_app_code(Instant::now()).expect_err("refused");

    assert_eq!(refusal.code, Code::Conflict);
    assert!(refusal.message.contains("phone_app true"), "{}", refusal.message);
}

#[test]
fn the_routes_follow_the_addresses_every_two_seconds() {
    let mut r = on();
    let start = r.at;
    r.addresses.lock().clear();

    r.app.refresh(start + Duration::from_secs(1));
    let before = r.app.phone.phase().clone();
    r.app.refresh(start + Duration::from_secs(3));

    assert!(matches!(before, Phase::Listening(_)));
    assert_eq!(*r.app.phone.phase(), Phase::NoAddress);
    assert_eq!(r.app.phone.note(), "waiting for a Wi-Fi, LAN or Tailscale address");
    let shown = r.app.app_shown();
    assert_eq!(shown.url, None);
    assert!(shown.lines[0].contains("no LAN or Tailscale address"), "{:?}", shown.lines);
    r.addresses.lock().push(LOCALHOST);
    listening(&mut r, start + Duration::from_secs(6));
    let phase = r.app.phone.phase().clone();
    assert!(matches!(&phase, Phase::Listening(routes) if routes.len() == 1), "{phase:?}");
}

#[test]
fn the_dialog_shows_the_code_and_closes_with_a_toast_when_an_app_pairs() {
    let mut r = on();
    let url = open_dialog(&mut r);
    assert!(url.starts_with(&format!("ws://127.0.0.1:{}/?code=", r.port)), "{url}");

    let secret = phone_secret(&url);
    pump_until(&mut r.app, &r.rx, "the pairing is told", |app| app.overlay.is_none());

    assert_eq!(toast(&r.app), Some("a phone app connected"));
    assert!(secret.contains('.'));
    assert_eq!(r.app.phone.paired().len(), 1);
}

#[test]
fn changing_the_port_keeps_the_held_code_working() {
    let mut r = on();
    open_dialog(&mut r);
    let port = free_port();

    r.app.config.phone_app_port = port;
    let at = r.at;
    listening(&mut r, at);
    let url = dialog(&r.app).and_then(|view| view.url).expect("an address");

    assert!(url.starts_with(&format!("ws://127.0.0.1:{port}/?code=")), "{url}");
    assert!(phone_secret(&url).contains('.'));
}

#[test]
fn a_paired_app_has_a_row_and_enter_on_it_forgets_the_app() {
    let mut r = on();
    let url = open_dialog(&mut r);
    let secret = phone_secret(&url);
    pump_until(&mut r.app, &r.rx, "the pairing is told", |app| app.overlay.is_none());
    let (id, _) = secret.split_once('.').expect("an id");
    let row = Row::PairedApp(id.to_string());

    open_remote_settings(&mut r.app);
    enter_on(&mut r.app, &row);

    assert!(!settings_rows(&r.app).contains(&row));
    assert_eq!(r.app.phone.paired().len(), 0);
    assert!(refused(r.port, &secret));
}

#[test]
fn enter_on_the_switch_and_on_pair_a_phone_app_open_the_code() {
    let mut r = rig();
    open_remote_settings(&mut r.app);
    enter_on(&mut r.app, &Row::PhoneApp);
    assert!(r.app.config.phone_app);
    listening(&mut r, Instant::now());
    assert!(settings_rows(&r.app).contains(&Row::PhoneAppCode));

    enter_on(&mut r.app, &Row::PhoneAppCode);

    let url = dialog(&r.app).and_then(|view| view.url).expect("an address");
    assert!(url.contains("/?code="), "{url}");
}
