use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};

use super::*;
use crate::test_util::write_executable;
use crate::web;

const STATUS: &str = r#"{"BackendState":"Running","CertDomains":["mac.tail1234.ts.net"],"Self":{"DNSName":"mac.tail1234.ts.net.","UserID":7},"User":{"7":{"LoginName":"ada@example.com"}}}"#;

struct Remote {
    app: App,
    rx: Receiver<AppEvent>,
    dir: TempDir,
    port: u16,
}

fn free_port() -> u16 {
    static NEXT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
    let base = 30_000 + u16::try_from(std::process::id() % 80).expect("fits") * 32;
    loop {
        let port = base + NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
}

fn with_tailscale(script: Option<&str>) -> Remote {
    let dir = TempDir::new();
    let (mut app, rx) = new_app(dir.path().join("config.json"));
    app.open_here(AREA).expect("open the first project");
    let program = match script {
        Some(script) => {
            let path = dir.path().join("tailscale");
            let log = dir.path().join("calls");
            write_executable(&path, &format!("#!/bin/sh\necho \"$@\" >> '{}'\n{script}\n", log.display()));
            path
        }
        None => dir.path().join("missing").join("tailscale"),
    };
    app.remote.set_tailscale(program);
    let port = free_port();
    app.config.remote_port = port;
    Remote { app, rx, dir, port }
}

fn ready() -> Remote {
    let mut r = with_tailscale(Some(&format!("[ \"$1\" = status ] && echo '{STATUS}'\nexit 0")));
    r.app.config.remote = true;
    r.app.refresh(Instant::now());
    pump_until(&mut r.app, &r.rx, "tailscale answers", |a| {
        matches!(a.remote.phase(), crate::app::remote_access::Phase::Ready(_))
    });
    r
}

fn calls(r: &Remote) -> String {
    std::fs::read_to_string(r.dir.path().join("calls")).unwrap_or_default()
}

fn get(port: u16, path: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    write!(stream, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n").expect("request");
    let mut answer = String::new();
    stream.read_to_string(&mut answer).expect("answer");
    answer
}

fn shown(app: &App) -> Option<ui::RemoteAccess> {
    match app.overlay_view(app.overlay.as_ref()?, AREA)? {
        ui::Overlay::Remote(remote) => Some(remote),
        _ => None,
    }
}

fn open_dialog(r: &mut Remote) {
    from_more(&mut r.app, "remote…");
    pick(&mut r.app, "open in your phone's browser");
}

fn open(r: &mut Remote) -> String {
    open_dialog(r);
    let url = shown(&r.app).and_then(|v| v.url).expect("a pairing address");
    url.split_once("?t=").map(|(_, token)| token.to_string()).expect("a token")
}

#[test]
fn is_off_until_switched_on() {
    let mut r = with_tailscale(Some("exit 0"));
    r.app.refresh(Instant::now());

    assert_eq!((r.app.remote_button(), r.app.remote.listening(), calls(&r)), (None, None, String::new()));
}

#[test]
fn switching_it_on_serves_the_port_over_tailscale_and_shows_the_button() {
    let r = ready();

    assert_eq!(calls(&r), format!("status --json\nserve --bg --yes --https=8443 http://127.0.0.1:{}\n", r.port));
    assert_eq!((r.app.remote_button(), r.app.remote.listening()), (Some(false), Some(r.port)));
    assert!(get(r.port, "/").starts_with("HTTP/1.1 401"));
}

#[test]
fn the_code_pairs_one_browser_once_then_the_dialog_closes() {
    let mut r = ready();
    let token = open(&mut r);

    let first = get(r.port, &format!("{}?t={token}", web::PAIR_PATH));
    pump_until(&mut r.app, &r.rx, "the pairing is told", |a| a.overlay.is_none());
    let again = get(r.port, &format!("{}?t={token}", web::PAIR_PATH));

    assert!(first.starts_with("HTTP/1.1 303") && first.contains("Set-Cookie: cornercase="), "{first}");
    assert!(again.starts_with("HTTP/1.1 403"), "{again}");
    assert_eq!((toast(&r.app), r.app.remote.paired()), (Some("a browser connected"), 1));
}

#[test]
fn a_code_stops_working_once_its_dialog_closes() {
    let mut r = ready();
    let token = open(&mut r);
    let done = ui::remote_done(AREA, &shown(&r.app).expect("the dialog"));
    click(&mut r.app, done.as_position());
    r.app.refresh(Instant::now());

    assert!(get(r.port, &format!("{}?t={token}", web::PAIR_PATH)).starts_with("HTTP/1.1 403"));
}

#[test]
fn switching_it_off_stops_serving() {
    let mut r = ready();
    r.app.config.remote = false;
    r.app.refresh(Instant::now());

    wait_until("tailscale stops serving", || calls(&r).ends_with("serve --https=8443 off\n"));
    assert_eq!((r.app.remote_button(), r.app.remote.listening()), (None, None));
    wait_until("the port stops accepting", || TcpStream::connect(("127.0.0.1", r.port)).is_err());
}

#[test]
fn without_tailscale_the_dialog_says_so() {
    let mut r = with_tailscale(None);
    r.app.config.remote = true;
    r.app.refresh(Instant::now());
    open_dialog(&mut r);

    let first = |a: &App| shown(a).and_then(|v| v.lines.first().cloned());
    pump_until(&mut r.app, &r.rx, "tailscale is found missing", |a| first(a).as_deref() != Some("starting Tailscale…"));
    assert_eq!(first(&r.app).as_deref(), Some("tailscale is not installed on this computer"));
}

#[test]
fn a_port_in_use_says_which() {
    let taken = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let mut r = with_tailscale(Some("exit 0"));
    r.app.config.remote_port = taken.local_addr().expect("address").port();
    r.app.config.remote = true;
    r.app.refresh(Instant::now());

    assert!(r.app.remote.note().contains("is taken"), "{}", r.app.remote.note());
}

fn get_with(port: u16, headers: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    write!(stream, "GET / HTTP/1.1\r\nHost: mac.tail1234.ts.net:8443\r\n{headers}\r\n").expect("request");
    let mut answer = String::new();
    stream.read_to_string(&mut answer).expect("answer");
    answer
}

#[test]
fn through_tailscale_only_the_owner_gets_past_the_door() {
    let r = ready();
    let forwarded = "X-Forwarded-For: 100.64.0.2\r\n";

    let owner = get_with(r.port, &format!("{forwarded}Tailscale-User-Login: ada@example.com\r\n"));
    let other = get_with(r.port, &format!("{forwarded}Tailscale-User-Login: bob@example.com\r\n"));
    let tagged = get_with(r.port, forwarded);

    assert!(owner.starts_with("HTTP/1.1 401"), "{owner}");
    assert!(other.starts_with("HTTP/1.1 403") && tagged.starts_with("HTTP/1.1 403"), "{other}\n{tagged}");
}

#[test]
fn opening_the_dialog_after_a_failure_tries_again_on_the_same_port() {
    let mut r = with_tailscale(None);
    let ok = r.dir.path().join("ok");
    let stopped = STATUS.replace("Running", "Stopped");
    let script = format!(
        "[ \"$1\" = status ] || exit 0\nif [ -e '{}' ]; then echo '{STATUS}'; else echo '{stopped}'; fi",
        ok.display()
    );
    let program = r.dir.path().join("tailscale");
    write_executable(&program, &format!("#!/bin/sh\n{script}\n"));
    r.app.remote.set_tailscale(program);
    r.app.config.remote = true;
    r.app.refresh(Instant::now());
    pump_until(&mut r.app, &r.rx, "tailscale is not connected", |a| {
        matches!(a.remote.phase(), crate::app::remote_access::Phase::Failed(_))
    });
    std::fs::write(&ok, "").expect("connect tailscale");

    open_dialog(&mut r);
    pump_until(&mut r.app, &r.rx, "tailscale answers again", |a| {
        matches!(a.remote.phase(), crate::app::remote_access::Phase::Ready(_))
    });

    assert_eq!(r.app.remote.listening(), Some(r.port));
    assert!(shown(&r.app).and_then(|v| v.url).is_some());
}

#[test]
fn switching_quickly_runs_the_tailscale_commands_in_order() {
    let mut r = with_tailscale(Some(&format!("[ \"$1\" = status ] && echo '{STATUS}'\nexit 0")));
    for on in [true, false, true] {
        r.app.config.remote = on;
        r.app.refresh(Instant::now());
    }

    let serve = format!("serve --bg --yes --https=8443 http://127.0.0.1:{}", r.port);
    let expected = format!("status --json\n{serve}\nserve --https=8443 off\nstatus --json\n{serve}\n");
    let ran = || calls(&r).lines().count() >= 5;
    wait_until_within("every command ran", Duration::from_secs(20), ran);
    assert_eq!(calls(&r), expected, "phase {:?}", r.app.remote.phase());
}

#[test]
fn forgetting_the_browsers_refuses_their_cookie() {
    let mut r = ready();
    let token = open(&mut r);
    let paired = get(r.port, &format!("{}?t={token}", web::PAIR_PATH));
    let cookie = paired.lines().find_map(|l| l.strip_prefix("Set-Cookie: ")).and_then(|c| c.split(';').next());
    let cookie = cookie.expect("a cookie").to_string();
    let page = |port: u16| {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        write!(stream, "GET / HTTP/1.1\r\nHost: x\r\nCookie: {cookie}\r\n\r\n").expect("request");
        let mut answer = String::new();
        stream.read_to_string(&mut answer).expect("answer");
        answer
    };
    let before = page(r.port);

    r.app.forget_browsers().expect("forgotten");

    assert!(before.starts_with("HTTP/1.1 200"), "{before}");
    assert!(page(r.port).starts_with("HTTP/1.1 401"));
}
