use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use rstest::rstest;
use tungstenite::client::IntoClientRequest as _;
use tungstenite::handshake::HandshakeError;
use tungstenite::{Message, WebSocket};

use super::*;
use crate::os::process::alive;
use crate::test_util::{TempDir, fake_program, free_port, http_get, wait_until};

const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const READ_FOR: Duration = Duration::from_secs(20);

struct Rig {
    dir: TempDir,
    phone: Phone,
    port: u16,
    events: Receiver<Event>,
    program: PathBuf,
}

impl Rig {
    fn new() -> Self {
        Self::with(|_| {})
    }

    fn with(configure: impl FnOnce(&mut Limits)) -> Self {
        let dir = TempDir::new();
        let (tx, events) = mpsc::channel();
        let mut phone = Phone::new(dir.path().join("apps.json"), dir.path().join("cornercase.sock"), tx);
        let program = fake_program(dir.path(), "cornercase");
        phone.set_program(program.clone());
        let mut limits = Limits::default();
        configure(&mut limits);
        phone.set_limits(limits);
        let port = free_port();
        let listened = phone.listen(&[LOCALHOST], port);
        assert_eq!(listened.routes.len(), 1, "{:?}", listened.failed);
        Self { dir, phone, port, events, program }
    }

    fn beside(&self, extension: &str) -> PathBuf {
        self.program.with_extension(extension)
    }

    fn say(&self, extension: &str, text: &str) {
        std::fs::write(self.beside(extension), text).expect("write beside the fake");
    }

    fn paired(&self) -> String {
        let (code, _) = self.phone.pair(Instant::now()).expect("a code");
        let mut ws = self
            .connect(&format!("/?code={code}"), &[("User-Agent", "cornercase-mobile/0.1 (Phone)".into())])
            .expect("paired");
        let seen = read_all(&mut ws);
        let reply: serde_json::Value = serde_json::from_str(&seen.texts[0]).expect("json");
        reply["secret"].as_str().expect("a secret").to_string()
    }

    fn connect(&self, path: &str, headers: &[(&'static str, String)]) -> Result<WebSocket<TcpStream>, u16> {
        let mut request = format!("ws://127.0.0.1:{}{path}", self.port).into_client_request().expect("a request");
        for (name, value) in headers {
            request.headers_mut().insert(*name, value.parse().expect("a header value"));
        }
        let stream = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        stream.set_read_timeout(Some(READ_FOR)).expect("read timeout");
        match tungstenite::client::client(request, stream) {
            Ok((ws, _)) => Ok(ws),
            Err(HandshakeError::Failure(tungstenite::Error::Http(response))) => Err(response.status().as_u16()),
            Err(e) => panic!("unexpected handshake failure: {e}"),
        }
    }

    fn cli(&self, secret: &str) -> WebSocket<TcpStream> {
        self.connect("/cli", &[("Authorization", format!("Bearer {secret}"))]).expect("an upgrade")
    }

    fn holding(&self, secret: &str) -> (WebSocket<TcpStream>, i32) {
        self.say("hold", "");
        let mut ws = self.cli(secret);
        argv(&mut ws, &["events"]);
        let pid = self.pid();
        (ws, pid)
    }

    fn pid(&self) -> i32 {
        wait_until("the fake's pid", || std::fs::read_to_string(self.beside("pid")).is_ok_and(|text| !text.is_empty()));
        let text = std::fs::read_to_string(self.beside("pid")).expect("the pid file");
        text.lines().next().and_then(|line| line.parse().ok()).expect("a pid")
    }

    fn ran(&self) -> bool {
        self.beside("argv").exists()
    }
}

#[derive(Debug, Default)]
struct Seen {
    texts: Vec<String>,
    close: Option<u16>,
}

fn read_all(ws: &mut WebSocket<TcpStream>) -> Seen {
    let mut seen = Seen::default();
    while let Ok(message) = ws.read() {
        match message {
            Message::Text(text) => seen.texts.push(text.to_string()),
            Message::Close(frame) => seen.close = frame.map(|frame| u16::from(frame.code)),
            _ => {}
        }
    }
    seen
}

fn send(ws: &mut WebSocket<TcpStream>, text: &str) {
    ws.send(Message::text(text)).expect("send");
}

fn argv(ws: &mut WebSocket<TcpStream>, words: &[&str]) {
    send(ws, &serde_json::json!({ "argv": words }).to_string());
}

fn stderr_then_exit_2(seen: &Seen) -> bool {
    seen.texts.len() == 2 && seen.texts[0].starts_with(r#"{"stderr":"#) && seen.texts[1] == r#"{"exit":2}"#
}

mod pairing {
    use super::*;

    #[test]
    fn takes_the_code_once_and_gives_a_secret() {
        let rig = Rig::new();
        let (code, _) = rig.phone.pair(Instant::now()).expect("a code");
        let agent = ("User-Agent", "cornercase-mobile/0.1 (Phone)".to_string());

        let mut ws = rig.connect(&format!("/?code={code}"), std::slice::from_ref(&agent)).expect("paired");
        let seen = read_all(&mut ws);
        let again = rig.connect(&format!("/?code={code}"), &[agent]).err();

        let reply: serde_json::Value = serde_json::from_str(&seen.texts[0]).expect("json");
        let secret = reply["secret"].as_str().expect("a secret");
        let (id, hex) = secret.split_once('.').expect("id and secret");
        assert_eq!((seen.texts.len(), seen.close), (1, Some(1000)));
        assert_eq!((hex.len(), hex.chars().all(|c| c.is_ascii_hexdigit())), (64, true));
        assert_eq!(again, Some(403));
        let saved = std::fs::read_to_string(rig.dir.path().join("apps.json")).expect("apps.json");
        assert!(saved.contains(id) && saved.contains("cornercase-mobile/0.1 (Phone)") && !saved.contains(hex));
        assert_eq!(rig.events.recv_timeout(READ_FOR), Ok(Event::AppPaired(1)));
    }

    #[test]
    fn a_wrong_code_is_refused_and_the_right_one_still_works() {
        let rig = Rig::new();
        let (code, _) = rig.phone.pair(Instant::now()).expect("a code");

        let wrong = rig.connect(&format!("/?code={}", "0".repeat(code.len())), &[]).err();
        let none = rig.connect("/", &[]).err();
        let right = rig.connect(&format!("/?code={code}"), &[]);

        assert_eq!((wrong, none), (Some(403), Some(403)));
        assert!(right.is_ok());
    }

    #[test]
    fn a_plain_get_with_the_code_gets_426_and_keeps_the_code() {
        let rig = Rig::new();
        let (code, _) = rig.phone.pair(Instant::now()).expect("a code");

        let answer = http_get(rig.port, &format!("/?code={code}"));

        assert!(answer.starts_with("HTTP/1.1 426"), "{answer}");
        assert!(rig.connect(&format!("/?code={code}"), &[]).is_ok());
    }

    #[test]
    fn other_paths_and_methods_are_refused() {
        let rig = Rig::new();

        assert!(http_get(rig.port, "/ws").starts_with("HTTP/1.1 404"));
        assert!(http_get(rig.port, "/pair?t=x").starts_with("HTTP/1.1 404"));
    }
}

mod the_command_line {
    use super::*;

    #[test]
    fn runs_the_argv_and_ends_with_one_exit() {
        let rig = Rig::new();
        rig.say("out", "a\nb");
        rig.say("err", "warn\n");
        rig.say("code", "3");
        let secret = rig.paired();

        let mut ws = rig.cli(&secret);
        argv(&mut ws, &["status", "--json"]);
        let seen = read_all(&mut ws);

        let stdout: Vec<&String> = seen.texts.iter().filter(|text| text.starts_with(r#"{"stdout""#)).collect();
        assert_eq!(stdout, [r#"{"stdout":"a"}"#, r#"{"stdout":"b"}"#]);
        assert!(seen.texts.contains(&r#"{"stderr":"warn"}"#.to_string()));
        assert_eq!(seen.texts.last().map(String::as_str), Some(r#"{"exit":3}"#));
        assert_eq!(seen.texts.iter().filter(|text| text.starts_with(r#"{"exit""#)).count(), 1);
        assert_eq!(seen.close, Some(1000));
        let recorded = std::fs::read_to_string(rig.beside("argv")).expect("argv");
        assert_eq!(recorded, "[\"status\", \"--json\"]\n");
    }

    #[rstest]
    #[case::no_secret(None)]
    #[case::a_wrong_one(Some(format!("abcd.{}", "0".repeat(64))))]
    fn without_a_known_secret_is_403_and_runs_nothing(#[case] secret: Option<String>) {
        let rig = Rig::new();
        let headers: Vec<(&'static str, String)> =
            secret.into_iter().map(|secret| ("Authorization", format!("Bearer {secret}"))).collect();

        let refused = rig.connect("/cli", &headers).err();

        assert_eq!(refused, Some(403));
        assert!(!rig.ran());
    }

    #[test]
    fn stdin_reaches_the_command() {
        let rig = Rig::new();
        rig.say("echo", "");
        let secret = rig.paired();

        let mut ws = rig.cli(&secret);
        argv(&mut ws, &["status"]);
        send(&mut ws, r#"{"stdin":"hello"}"#);
        send(&mut ws, r#"{"stdin_base64":"aGk="}"#);
        send(&mut ws, r#"{"eof":true}"#);
        let seen = read_all(&mut ws);

        assert_eq!(seen.texts, [r#"{"stdout":"hello"}"#, r#"{"stdout":"hi"}"#, r#"{"exit":0}"#]);
    }

    #[rstest]
    #[case::kill_server(&["kill-server"])]
    #[case::start_server(&["start-server"])]
    #[case::no_command(&[])]
    #[case::a_remote_flag(&["status", "--remote", "vps"])]
    fn a_refused_argv_is_one_stderr_line_and_exit_2(#[case] words: &[&str]) {
        let rig = Rig::new();
        let secret = rig.paired();

        let mut ws = rig.cli(&secret);
        argv(&mut ws, words);
        let seen = read_all(&mut ws);

        assert!(stderr_then_exit_2(&seen), "{seen:?}");
        assert_eq!(seen.close, Some(1000));
        assert!(!rig.ran());
    }

    #[rstest]
    #[case::not_json("nope")]
    #[case::stdin_first(r#"{"stdin":"a"}"#)]
    #[case::two_keys(r#"{"argv":[],"eof":true}"#)]
    fn a_bad_first_message_is_refused_the_same_way(#[case] first: &str) {
        let rig = Rig::new();
        let secret = rig.paired();

        let mut ws = rig.cli(&secret);
        send(&mut ws, first);
        let seen = read_all(&mut ws);

        assert!(stderr_then_exit_2(&seen), "{seen:?}");
        assert!(!rig.ran());
    }

    #[test]
    fn closing_kills_the_command() {
        let rig = Rig::new();
        let secret = rig.paired();

        let (ws, pid) = rig.holding(&secret);
        drop(ws);

        wait_until("the command to be killed", || !alive(pid));
    }

    #[test]
    fn silence_kills_the_command() {
        let rig = Rig::with(|limits| {
            limits.ping_every = Duration::from_millis(50);
            limits.silent_for = Duration::from_millis(300);
        });
        let secret = rig.paired();

        let (ws, pid) = rig.holding(&secret);

        wait_until("the command to be killed", || !alive(pid));
        drop(ws);
    }

    #[test]
    fn a_client_that_stops_reading_blocks_the_command() {
        let rig = Rig::with(|limits| limits.queued_bytes = 64 * 1024);
        let line = "0123456789012345678901\n";
        rig.say("out", &line.repeat(12 * 1024 * 1024 / line.len()));
        let secret = rig.paired();

        let mut ws = rig.cli(&secret);
        argv(&mut ws, &["events"]);
        let pid = rig.pid();
        thread::sleep(Duration::from_millis(500));
        let blocked = alive(pid);
        let seen = read_all(&mut ws);

        assert!(blocked, "the command ran to its end while nobody read");
        assert_eq!(seen.texts.last().map(String::as_str), Some(r#"{"exit":0}"#));
        assert_eq!(seen.texts.len(), 12 * 1024 * 1024 / line.len() + 1);
    }

    #[test]
    fn a_command_that_never_reads_its_stdin_is_killed_once_nothing_moves() {
        let rig = Rig::with(|limits| {
            limits.ping_every = Duration::from_millis(50);
            limits.silent_for = Duration::from_millis(500);
        });
        let secret = rig.paired();

        let (mut ws, pid) = rig.holding(&secret);
        ws.get_ref().set_write_timeout(Some(Duration::from_secs(2))).expect("a write timeout");
        let chunk = format!(r#"{{"stdin":"{}"}}"#, "x".repeat(8 * 1024));
        for _ in 0..40 {
            let _ = ws.send(Message::text(chunk.clone()));
        }
        drop(ws);

        wait_until("the command to be killed", || !alive(pid));
    }

    #[test]
    fn a_line_over_the_limit_ends_with_exit_2() {
        let rig = Rig::with(|limits| limits.longest_line = 16);
        rig.say("out", "this line is longer than sixteen bytes\n");
        let secret = rig.paired();

        let mut ws = rig.cli(&secret);
        argv(&mut ws, &["events"]);
        let seen = read_all(&mut ws);

        assert!(stderr_then_exit_2(&seen), "{seen:?}");
    }

    #[test]
    fn a_bad_message_after_argv_kills_the_command() {
        let rig = Rig::new();
        let secret = rig.paired();

        let (mut ws, pid) = rig.holding(&secret);
        send(&mut ws, r#"{"stdin":1}"#);
        let seen = read_all(&mut ws);

        assert!(stderr_then_exit_2(&seen), "{seen:?}");
        wait_until("the command to be killed", || !alive(pid));
    }

    #[test]
    fn a_message_over_1_mib_is_refused() {
        let rig = Rig::new();
        let secret = rig.paired();

        let (mut ws, pid) = rig.holding(&secret);
        let big = format!(r#"{{"stdin":"{}"}}"#, "x".repeat(1024 * 1024));
        let _ = ws.send(Message::text(big));
        let seen = read_all(&mut ws);

        assert!(stderr_then_exit_2(&seen), "{seen:?}");
        assert!(seen.texts[0].contains("1 MiB"));
        wait_until("the command to be killed", || !alive(pid));
    }

    #[test]
    fn no_argv_in_time_is_refused() {
        let rig = Rig::with(|limits| limits.argv_within = Duration::from_millis(300));
        let secret = rig.paired();

        let mut ws = rig.cli(&secret);
        let seen = read_all(&mut ws);

        assert!(stderr_then_exit_2(&seen), "{seen:?}");
        assert!(seen.texts[0].contains("no `argv`"));
    }

    #[test]
    fn forgetting_an_app_closes_its_connections() {
        let rig = Rig::new();
        let secret = rig.paired();
        let (id, _) = secret.split_once('.').expect("an id");

        let (mut ws, pid) = rig.holding(&secret);
        assert_eq!(rig.phone.forget(id).ok(), Some(true));
        let seen = read_all(&mut ws);

        assert!(seen.texts.is_empty() && seen.close.is_none(), "{seen:?}");
        wait_until("the command to be killed", || !alive(pid));
        assert_eq!(rig.connect("/cli", &[("Authorization", format!("Bearer {secret}"))]).err(), Some(403));
        assert_eq!(rig.phone.forget(id).ok(), Some(false));
    }

    #[test]
    fn a_connection_accepted_before_a_close_runs_nothing_after_it() {
        let mut rig = Rig::new();
        let secret = rig.paired();
        let mut early = TcpStream::connect(("127.0.0.1", rig.port)).expect("connect");
        wait_until("the connection is accepted", || rig.phone.open() > 0);

        rig.phone.close();
        let head = format!(
            "GET /cli HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\
             Authorization: Bearer {secret}\r\n\r\n"
        );
        io::Write::write_all(&mut early, head.as_bytes()).expect("the head");
        let mut answer = String::new();
        io::Read::read_to_string(&mut early, &mut answer).expect("an answer");

        assert!(answer.starts_with("HTTP/1.1 503"), "{answer}");
        assert!(!rig.ran());
    }

    #[test]
    fn a_connection_still_waiting_for_its_argv_hears_going_away() {
        let mut rig = Rig::new();
        let secret = rig.paired();

        let mut ws = rig.cli(&secret);
        rig.phone.close();
        let seen = read_all(&mut ws);

        assert_eq!((seen.texts.len(), seen.close), (0, Some(1001)));
    }

    #[test]
    fn closing_the_listener_says_going_away_and_sends_no_exit() {
        let mut rig = Rig::new();
        let secret = rig.paired();

        let (mut ws, pid) = rig.holding(&secret);
        rig.phone.close();
        let seen = read_all(&mut ws);

        assert_eq!((seen.texts.is_empty(), seen.close), (true, Some(1001)));
        wait_until("the command to be killed", || !alive(pid));
    }
}

mod the_listener {
    use super::*;

    fn can_bind_ipv6() -> bool {
        TcpListener::bind((Ipv6Addr::LOCALHOST, 0)).is_ok()
    }

    #[test]
    fn follows_the_addresses_and_keeps_the_connections_made() {
        let mut rig = Rig::new();
        let secret = rig.paired();
        let (ws, pid) = rig.holding(&secret);
        let (port, v6) = (rig.port, IpAddr::V6(Ipv6Addr::LOCALHOST));

        if can_bind_ipv6() {
            let both = rig.phone.listen(&[LOCALHOST, v6], port);
            assert_eq!(both.routes.len(), 2, "{:?}", both.failed);
        }
        let none = rig.phone.listen(&[], port);

        assert_eq!(none.routes.len(), 0);
        assert_eq!(rig.phone.listening().len(), 0);
        assert!(alive(pid));
        assert!(TcpStream::connect(("127.0.0.1", port)).is_err());
        let again = rig.phone.listen(&[LOCALHOST], port);
        assert_eq!(again.routes.len(), 1);
        assert!(rig.connect("/cli", &[]).is_err());
        drop(ws);
        wait_until("the command to be killed", || !alive(pid));
    }

    #[test]
    fn a_taken_port_is_reported_for_its_address() {
        let rig = Rig::new();
        let taken = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a port");
        let port = taken.local_addr().expect("address").port();
        let mut other = Phone::new(rig.dir.path().join("other.json"), PathBuf::new(), mpsc::channel::<Event>().0);

        let listened = other.listen(&[LOCALHOST], port);

        assert_eq!(listened.routes.len(), 0);
        assert_eq!(listened.failed.len(), 1);
        let _: &Path = rig.dir.path();
    }
}
