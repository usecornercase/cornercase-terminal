use std::net::SocketAddr;
use std::thread;

use rustls::ClientConnection;
use rustls::pki_types::ServerName;
use tungstenite::stream::MaybeTlsStream;

use super::*;
use crate::test_util::tls::{Encrypted, connect_pinned, pinned};

const OTHER_PIN: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

impl Rig {
    fn pin(&self) -> String {
        self.phone.fingerprint().expect("a fingerprint")
    }

    fn tls(&self, path: &str, headers: &[(&'static str, String)], pin: &str) -> Result<Encrypted, Option<u16>> {
        tls_at(SocketAddr::new(LOCALHOST, self.port), path, headers, pin)
    }

    fn paired_over_tls(&self) -> String {
        let (code, _) = self.phone.pair(Instant::now()).expect("a code");
        let mut ws = self.tls(&format!("/?code={code}"), &[], &self.pin()).expect("paired");
        let seen = read_all(&mut ws);
        let reply: serde_json::Value = serde_json::from_str(&seen.texts[0]).expect("json");
        reply["secret"].as_str().expect("a secret").to_string()
    }

    fn cli_over_tls(&self, secret: &str) -> Encrypted {
        self.tls("/cli", &[("Authorization", format!("Bearer {secret}"))], &self.pin()).expect("an upgrade")
    }
}

fn tls_at(at: SocketAddr, path: &str, headers: &[(&'static str, String)], pin: &str) -> Result<Encrypted, Option<u16>> {
    let mut request = format!("wss://localhost:{}{path}", at.port()).into_client_request().expect("a request");
    for (name, value) in headers {
        request.headers_mut().insert(*name, value.parse().expect("a header value"));
    }
    let stream = TcpStream::connect(at).expect("connect");
    stream.set_read_timeout(Some(READ_FOR)).expect("read timeout");
    connect_pinned(request, stream, pin)
}

fn ended_cleanly(ws: &mut Encrypted) -> bool {
    let mut byte = [0_u8; 1];
    match ws.get_mut() {
        MaybeTlsStream::Rustls(stream) => matches!(stream.read(&mut byte), Ok(0)),
        _ => false,
    }
}

fn handshaken(rig: &Rig) -> (ClientConnection, TcpStream) {
    let name = ServerName::try_from("localhost").expect("a server name");
    let mut client = ClientConnection::new(pinned(&rig.pin()), name).expect("a client");
    let mut stream = TcpStream::connect(("127.0.0.1", rig.port)).expect("connect");
    while client.is_handshaking() {
        client.complete_io(&mut stream).expect("the handshake");
    }
    (client, stream)
}

fn raw(rig: &Rig, bytes: &[u8]) -> TcpStream {
    let mut stream = TcpStream::connect(("127.0.0.1", rig.port)).expect("connect");
    stream.write_all(bytes).expect("the bytes");
    stream
}

mod pinned {
    use super::*;

    #[test]
    fn a_client_that_pins_the_fingerprint_pairs_and_runs_a_command() {
        let rig = Rig::new();
        rig.say("out", "a");
        let secret = rig.paired_over_tls();

        let mut ws = rig.cli_over_tls(&secret);
        argv(&mut ws, &["status"]);
        let seen = read_all(&mut ws);

        assert_eq!(seen.texts, [r#"{"stdout":"a"}"#, r#"{"exit":0}"#]);
        assert_eq!(seen.close, Some(1000));
        assert!(ended_cleanly(&mut ws), "no close_notify after the close");
        assert_eq!(rig.phone.apps().len(), 1);
    }

    #[test]
    fn a_client_that_pins_another_fingerprint_fails_and_pairs_nothing() {
        let rig = Rig::new();
        let (code, _) = rig.phone.pair(Instant::now()).expect("a code");

        let refused = rig.tls(&format!("/?code={code}"), &[], OTHER_PIN).err();
        let right = rig.tls(&format!("/?code={code}"), &[], &rig.pin());

        assert_eq!(refused, Some(None));
        assert!(right.is_ok());
    }

    #[test]
    fn every_route_shows_the_same_certificate() {
        let mut rig = Rig::new();
        let v6 = IpAddr::V6(Ipv6Addr::LOCALHOST);
        let routes = if TcpListener::bind((v6, 0)).is_ok() { vec![LOCALHOST, v6] } else { vec![LOCALHOST] };
        rig.phone.listen(&routes, rig.port, &no_name).expect("listening");

        for ip in routes {
            let refused = tls_at(SocketAddr::new(ip, rig.port), "/cli", &[], &rig.pin()).err();
            assert_eq!(refused, Some(Some(403)), "{ip}");
        }
    }

    #[test]
    fn the_plain_and_the_encrypted_client_share_one_port() {
        let rig = Rig::new();
        let secret = rig.paired();

        let mut ws = rig.cli_over_tls(&secret);
        argv(&mut ws, &["status"]);
        let seen = read_all(&mut ws);

        assert_eq!(seen.texts.last().map(String::as_str), Some(r#"{"exit":0}"#));
    }
}

mod the_command_line {
    use super::*;

    #[test]
    fn a_client_that_stops_reading_blocks_the_command() {
        let rig = Rig::with(|limits| limits.queued_bytes = 64 * 1024);
        let line = "0123456789012345678901\n";
        rig.say("out", &line.repeat(12 * 1024 * 1024 / line.len()));
        let secret = rig.paired_over_tls();

        let mut ws = rig.cli_over_tls(&secret);
        argv(&mut ws, &["events"]);
        let pid = rig.pid();
        wait_until("the output waits for the client", || rig.phone.waits() > 0);
        let blocked = alive(pid);
        let seen = read_all(&mut ws);

        assert!(blocked, "the command ran to its end while nobody read");
        assert_eq!(seen.texts.last().map(String::as_str), Some(r#"{"exit":0}"#));
        assert_eq!(seen.texts.len(), 12 * 1024 * 1024 / line.len() + 1);
    }

    #[test]
    fn output_reaches_the_client_while_the_command_still_runs() {
        let rig = Rig::with(|limits| limits.ping_every = Duration::from_secs(60));
        let secret = rig.paired_over_tls();
        rig.say("out", "a\n");
        rig.say("hold", "");

        let mut ws = rig.cli_over_tls(&secret);
        argv(&mut ws, &["events"]);
        let line = ws.read().expect("a line");
        let pid = rig.pid();

        assert_eq!(line.into_text().expect("text").as_str(), r#"{"stdout":"a"}"#);
        assert!(alive(pid), "the command ended before its line was read");
    }

    #[test]
    fn closing_the_listener_says_going_away_then_ends_cleanly() {
        let mut rig = Rig::new();
        let secret = rig.paired_over_tls();
        rig.say("hold", "");
        let mut ws = rig.cli_over_tls(&secret);
        argv(&mut ws, &["events"]);
        let pid = rig.pid();

        rig.phone.close();
        let seen = read_all(&mut ws);

        assert_eq!((seen.texts.is_empty(), seen.close), (true, Some(1001)), "{seen:?}");
        assert!(ended_cleanly(&mut ws), "no close_notify after going away");
        wait_until("the command to be killed", || !alive(pid));
    }
}

mod the_handshake {
    use super::*;

    #[test]
    fn a_handshake_that_stalls_loses_its_place_at_the_head_s_deadline() {
        let rig = Rig::with(|limits| limits.head_within = Duration::from_millis(300));
        wait_until("nothing is open", || rig.phone.open() == 0);

        let _stalled = raw(&rig, &[0x16]);
        wait_until("the connection is accepted", || rig.phone.open() > 0);

        wait_until("the stalled handshake lets go", || rig.phone.open() == 0);
    }

    #[test]
    fn a_request_sent_a_byte_at_a_time_loses_its_place_at_the_head_s_deadline() {
        let rig = Rig::with(|limits| limits.head_within = Duration::from_millis(300));
        let (mut client, mut stream) = handshaken(&rig);
        client.writer().write_all(b"GET /cli HTTP/1.1\r\nHost: localhost\r\n\r\n").expect("the request");
        let mut record = Vec::new();
        client.write_tls(&mut record).expect("the record");

        let mut sent = 0;
        while sent < record.len() && rig.phone.open() > 0 {
            let _ = stream.write_all(&record[sent..=sent]);
            sent += 1;
            thread::sleep(Duration::from_millis(100));
        }

        assert!(sent < record.len() / 2, "held its place for {sent} of {} bytes", record.len());
    }

    #[test]
    fn garbage_after_the_first_byte_is_dropped_and_the_listener_still_serves() {
        let rig = Rig::new();
        let mut garbage = raw(&rig, &[0x16, 0x03, 0x01, 0x00, 0x05, 1, 2, 3, 4, 5]);

        let mut answer = Vec::new();
        let _ = garbage.read_to_end(&mut answer);

        assert!(!answer.starts_with(b"HTTP/"), "{answer:?}");
        assert!(http_get(rig.port, "/").starts_with("HTTP/1.1 426"));
        assert_eq!(rig.tls("/cli", &[], &rig.pin()).err(), Some(Some(403)));
    }
}

mod a_new_certificate {
    use super::*;

    #[test]
    fn replaces_the_pin_forgets_the_apps_and_cuts_their_connections() {
        let rig = Rig::new();
        let old = rig.pin();
        let secret = rig.paired_over_tls();
        rig.say("hold", "");
        let mut held = rig.cli_over_tls(&secret);
        argv(&mut held, &["events"]);
        let pid = rig.pid();

        let (new, forgotten) = rig.phone.renew(Some("Mac")).expect("renewed");
        let cut = read_all(&mut held);

        assert_ne!(new, old);
        assert_eq!((rig.pin(), forgotten), (new.clone(), 1));
        assert!(cut.close.is_none() && !cut.timed_out, "{cut:?}");
        wait_until("the command to be killed", || !alive(pid));
        assert_eq!(rig.tls("/cli", &[], &old).err(), Some(None));
        let refused = rig.tls("/cli", &[("Authorization", format!("Bearer {secret}"))], &new).err();
        assert_eq!(refused, Some(Some(403)));
        assert_eq!(rig.phone.apps().len(), 0);
        assert!(rig.paired_over_tls().contains('.'));
    }

    #[test]
    fn that_cannot_forget_the_apps_changes_nothing() {
        let rig = Rig::new();
        let old = rig.pin();
        rig.paired_over_tls();
        folder_in_place_of(&rig, "apps.json");

        let renewed = rig.phone.renew(None);

        assert!(renewed.is_err());
        assert_eq!((rig.pin(), rig.phone.apps().len()), (old.clone(), 1));
        assert_eq!(loaded_next_time(&rig), Some(old));
    }

    #[test]
    fn that_cannot_be_written_still_forgets_the_apps_and_keeps_the_old_one() {
        let rig = Rig::new();
        let old = rig.pin();
        rig.paired_over_tls();
        folder_in_place_of(&rig, "app-certificate.json");

        let renewed = rig.phone.renew(None).expect_err("not written");

        assert!(renewed.to_string().contains("forgotten"), "{renewed}");
        assert_eq!((rig.pin(), rig.phone.apps().len()), (old, 0));
    }

    #[test]
    fn is_the_one_loaded_next_time() {
        let rig = Rig::new();
        let (new, _) = rig.phone.renew(None).expect("renewed");

        assert_eq!(loaded_next_time(&rig), Some(new));
    }

    fn folder_in_place_of(rig: &Rig, file: &str) {
        let path = rig.dir.path().join(file);
        std::fs::remove_file(&path).expect("the file");
        std::fs::create_dir(&path).expect("a folder in its place");
    }

    fn loaded_next_time(rig: &Rig) -> Option<String> {
        let (apps, certificate) = (rig.dir.path().join("next.json"), rig.dir.path().join("app-certificate.json"));
        let mut next = Phone::new(apps, certificate, PathBuf::new(), mpsc::channel::<Event>().0);
        next.listen(&[LOCALHOST], free_port(), &no_name).expect("listening");
        next.fingerprint()
    }
}
