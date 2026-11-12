use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::host::open::Opener;

fn round_trip<T: Serialize + DeserializeOwned>(msg: &T) -> T {
    let mut buf = Vec::new();
    send(&mut buf, msg).expect("send");
    recv(&mut buf.as_slice()).expect("recv").expect("a message")
}

mod framing {
    use super::*;

    #[test]
    fn round_trips_client_events() {
        let key = Event::Key(KeyEvent::new(KeyCode::Char('ñ'), KeyModifiers::ALT));

        let msg = round_trip(&ClientMessage::Event(key.clone()));

        assert!(matches!(msg, ClientMessage::Event(ev) if ev == key));
    }

    #[test]
    fn round_trips_frames() {
        let msg = round_trip(&ServerMessage::Frame(b"\x1b[2Jhello".to_vec()));

        assert!(matches!(msg, ServerMessage::Frame(bytes) if bytes == b"\x1b[2Jhello"));
    }

    #[test]
    fn reads_none_at_end_of_stream() {
        assert!(recv::<ServerMessage>(&mut [].as_slice()).expect("recv").is_none());
    }

    #[test]
    fn rejects_garbage_as_invalid_data() {
        let mut buf = Vec::new();
        send(&mut buf, &[0xffu8; 8]).expect("send");

        let err = recv::<ClientMessage>(&mut buf.as_slice()).expect_err("garbage must not decode");

        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn rejects_oversized_messages() {
        let len = u32::try_from(MAX_MESSAGE + 1).expect("fits in u32").to_le_bytes();

        let err = recv::<ClientMessage>(&mut len.as_slice()).expect_err("too big");

        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(!undecoded(&err));
    }

    #[test]
    fn a_whole_message_it_cannot_decode_leaves_the_next_one_readable() {
        let mut buf = Vec::new();
        send(&mut buf, &99u8).expect("send");
        send(&mut buf, &ServerMessage::Detached).expect("send");
        let mut reader = buf.as_slice();

        let err = recv::<ServerMessage>(&mut reader).expect_err("an unknown variant");

        assert!(undecoded(&err));
        assert!(matches!(recv(&mut reader), Ok(Some(ServerMessage::Detached))));
    }

    #[test]
    fn a_broken_stream_is_not_an_undecoded_message() {
        let err = recv::<ServerMessage>(&mut [2u8, 0, 0, 0, 1].as_slice()).expect_err("cut short");

        assert!(!undecoded(&err));
    }
}

mod compatibility {
    use super::*;
    use crate::test_util::fixture;

    #[test]
    fn kill_server_is_the_first_client_variant() {
        assert_eq!(postcard::to_stdvec(&ClientMessage::KillServer).expect("encode"), [0]);
    }

    #[test]
    fn rejected_is_the_first_server_variant() {
        assert_eq!(postcard::to_stdvec(&ServerMessage::Rejected(String::new())).expect("encode"), [0, 0]);
    }

    #[derive(Debug, Deserialize)]
    #[expect(dead_code, reason = "decoded only, the way a server from before Restart reads it")]
    enum BeforeRestart {
        KillServer,
        Hello(Box<Hello>),
        Event(Event),
    }

    #[derive(Debug, Deserialize)]
    #[expect(dead_code, reason = "decoded only, the way a server from before Request reads it")]
    enum BeforeRequest {
        KillServer,
        Hello(Box<Hello>),
        Event(Event),
        Restart,
    }

    #[derive(Debug, Deserialize)]
    #[expect(dead_code, reason = "decoded only, the way a server from before Graphics reads it")]
    enum BeforeGraphics {
        KillServer,
        Hello(Box<Hello>),
        Event(Event),
        Restart,
        Request(String),
    }

    #[derive(Debug, Deserialize)]
    #[expect(dead_code, reason = "decoded only, the way a server from before Activated reads it")]
    enum BeforeActivated {
        KillServer,
        Hello(Box<Hello>),
        Event(Event),
        Restart,
        Request(String),
        Graphics(Support),
    }

    #[derive(Debug, Deserialize)]
    #[expect(dead_code, reason = "decoded only, the way a client from before Response reads it")]
    enum BeforeResponse {
        Rejected(String),
        Frame(Vec<u8>),
        Detached,
        Shutdown,
        Restart(PathBuf),
    }

    #[derive(Debug, Deserialize)]
    #[expect(dead_code, reason = "decoded only, the way a client from before Sound reads it")]
    enum BeforeSound {
        Rejected(String),
        Frame(Vec<u8>),
        Detached,
        Shutdown,
        Restart(PathBuf),
        Response(String),
    }

    #[derive(Debug, Deserialize)]
    #[expect(dead_code, reason = "decoded only, the way a server from before OpenFailed reads it")]
    enum BeforeOpenFailed {
        KillServer,
        Hello(Box<Hello>),
        Event(Event),
        Restart,
        Request(String),
        Graphics(Support),
        Activated(String),
    }

    #[derive(Debug, Deserialize)]
    #[expect(dead_code, reason = "decoded only, the way a client from before Open reads it")]
    enum BeforeOpen {
        Rejected(String),
        Frame(Vec<u8>),
        Detached,
        Shutdown,
        Restart(PathBuf),
        Response(String),
        Sound(Cue),
    }

    #[derive(Debug, Deserialize)]
    #[expect(dead_code, reason = "decoded only, the way a client from before Tunnel reads it")]
    enum BeforeTunnel {
        Rejected(String),
        Frame(Vec<u8>),
        Detached,
        Shutdown,
        Restart(PathBuf),
        Response(String),
        Sound(Cue),
        Open(Open),
        Browse(Browse),
        Machine(MachineOrder),
    }

    #[derive(Debug, Deserialize)]
    #[expect(dead_code, reason = "decoded only, the way a server from before Tunnels reads it")]
    enum BeforeTunnels {
        KillServer,
        Hello(Box<Hello>),
        Event(Event),
        Restart,
        Request(String),
        Graphics(Support),
        Activated(String),
        OpenFailed(Failed),
        Machines(Machines),
    }

    #[derive(Debug, Deserialize)]
    #[expect(dead_code, reason = "decoded only, the way a window from before Forget reads an order")]
    enum BeforeForget {
        Open(u16),
        Close(u16),
    }

    #[derive(Debug, Deserialize)]
    #[expect(dead_code, reason = "decoded only, the way a server from before Closed reads a state")]
    enum BeforeClosed {
        Opening,
        Up,
        Down(String),
    }

    fn read_as<T: DeserializeOwned + std::fmt::Debug>(msg: &impl Serialize) -> io::ErrorKind {
        let mut frame = Vec::new();
        send(&mut frame, msg).expect("encode");
        recv::<T>(&mut frame.as_slice()).expect_err("an unknown variant").kind()
    }

    #[test]
    fn a_server_without_restart_reads_it_as_invalid_data() {
        assert_eq!(read_as::<BeforeRestart>(&ClientMessage::Restart), io::ErrorKind::InvalidData);
    }

    #[test]
    fn a_request_comes_after_every_older_client_variant() {
        assert_eq!(postcard::to_stdvec(&ClientMessage::Request(String::new())).expect("encode"), [4, 0]);
    }

    #[test]
    fn a_server_without_requests_reads_one_as_invalid_data() {
        let request = ClientMessage::Request(r#"{"command":"status","args":{}}"#.into());

        assert_eq!(read_as::<BeforeRequest>(&request), io::ErrorKind::InvalidData);
    }

    #[test]
    fn graphics_come_after_every_older_client_variant() {
        assert_eq!(postcard::to_stdvec(&ClientMessage::Graphics(Support::default())).expect("encode")[0], 5);
    }

    #[test]
    fn a_server_without_graphics_reads_them_as_invalid_data() {
        let graphics = ClientMessage::Graphics(Support { id_hi: 7, ..Support::default() });

        assert_eq!(read_as::<BeforeGraphics>(&graphics), io::ErrorKind::InvalidData);
    }

    #[test]
    fn activations_come_after_every_older_client_variant() {
        assert_eq!(postcard::to_stdvec(&ClientMessage::Activated(String::new())).expect("encode"), [6, 0]);
    }

    #[test]
    fn a_server_without_activations_reads_one_as_invalid_data() {
        assert_eq!(read_as::<BeforeActivated>(&ClientMessage::Activated("x".into())), io::ErrorKind::InvalidData);
    }

    #[test]
    fn a_response_comes_after_every_older_server_variant() {
        assert_eq!(postcard::to_stdvec(&ServerMessage::Response(String::new())).expect("encode"), [5, 0]);
    }

    #[test]
    fn a_client_without_responses_reads_one_as_invalid_data() {
        assert_eq!(read_as::<BeforeResponse>(&ServerMessage::Response("{}".into())), io::ErrorKind::InvalidData);
    }

    fn cue() -> Cue {
        Cue { kind: crate::host::sound::Kind::NeedsYou, path: "~/ding.wav".into() }
    }

    #[test]
    fn a_sound_comes_after_every_older_server_variant() {
        assert_eq!(postcard::to_stdvec(&ServerMessage::Sound(cue())).expect("encode")[0], 6);
    }

    #[test]
    fn a_client_without_sounds_reads_one_as_invalid_data() {
        assert_eq!(read_as::<BeforeSound>(&ServerMessage::Sound(cue())), io::ErrorKind::InvalidData);
    }

    #[test]
    fn a_sound_round_trips() {
        assert!(matches!(round_trip(&ServerMessage::Sound(cue())), ServerMessage::Sound(c) if c == cue()));
    }

    fn order() -> Open {
        Open { opener: Opener::Vscode, path: "/work/web".into(), line: Some(3) }
    }

    fn failure() -> Failed {
        Failed { opener: Opener::Files, reason: "exit status 1".into() }
    }

    #[test]
    fn an_open_comes_after_every_older_server_variant() {
        assert_eq!(postcard::to_stdvec(&ServerMessage::Open(order())).expect("encode")[0], 7);
    }

    #[test]
    fn a_client_without_opening_reads_an_open_as_invalid_data() {
        assert_eq!(read_as::<BeforeOpen>(&ServerMessage::Open(order())), io::ErrorKind::InvalidData);
    }

    #[test]
    fn an_open_failure_comes_after_every_older_client_variant() {
        assert_eq!(postcard::to_stdvec(&ClientMessage::OpenFailed(failure())).expect("encode")[0], 7);
    }

    #[test]
    fn a_server_without_opening_reads_a_failure_as_invalid_data() {
        assert_eq!(read_as::<BeforeOpenFailed>(&ClientMessage::OpenFailed(failure())), io::ErrorKind::InvalidData);
    }

    #[test]
    fn a_browse_comes_after_every_older_server_variant_and_round_trips() {
        let browse = Browse { ip: Some("::1".parse().expect("an address")), port: 5173 };
        assert_eq!(postcard::to_stdvec(&ServerMessage::Browse(browse)).expect("encode")[0], 8);
        assert!(matches!(round_trip(&ServerMessage::Browse(browse)), ServerMessage::Browse(b) if b == browse));
    }

    #[test]
    fn opens_and_failures_round_trip() {
        assert!(matches!(round_trip(&ServerMessage::Open(order())), ServerMessage::Open(o) if o == order()));
        assert!(
            matches!(round_trip(&ClientMessage::OpenFailed(failure())), ClientMessage::OpenFailed(f) if f == failure())
        );
    }

    #[test]
    fn a_hello_carries_its_openers() {
        let hello = Hello {
            version: VERSION,
            build: "b".into(),
            cols: 80,
            rows: 24,
            theme: HostTheme::default(),
            notify: Channel::Bell,
            terminal: None,
            pointer: None,
            graphics: Support::default(),
            probe: String::new(),
            zone: "UTC".into(),
            openers: vec![Opener::Files, Opener::Vscode],
            machines: Some(Machines { here: Some("devbox".into()), known: vec!["devbox".into()] }),
        };
        let back = round_trip(&ClientMessage::Hello(Box::new(hello)));
        assert!(
            matches!(back, ClientMessage::Hello(h) if h.openers.len() == 2 && h.machines.as_ref().is_some_and(|m| m.known == ["devbox"]))
        );
    }

    #[test]
    fn a_machine_order_comes_after_every_older_server_variant_and_round_trips() {
        let order = MachineOrder::Switch(Some("devbox".into()));
        assert_eq!(postcard::to_stdvec(&ServerMessage::Machine(order.clone())).expect("encode")[0], 9);
        assert!(matches!(round_trip(&ServerMessage::Machine(order.clone())), ServerMessage::Machine(o) if o == order));
    }

    #[test]
    fn the_machines_come_after_every_older_client_variant_and_round_trip() {
        let machines = Machines { here: None, known: vec!["devbox".into(), "gpu-box".into()] };
        assert_eq!(postcard::to_stdvec(&ClientMessage::Machines(machines.clone())).expect("encode")[0], 8);
        assert!(
            matches!(round_trip(&ClientMessage::Machines(machines.clone())), ClientMessage::Machines(m) if m == machines)
        );
    }

    fn holding() -> Holding {
        Holding {
            browse: true,
            orders: 2,
            tunnels: vec![Tunnel { port: 5173, local: Some(5174), state: TunnelState::Down("no".into()) }],
        }
    }

    #[test]
    fn a_tunnel_order_comes_after_every_older_server_variant_and_round_trips() {
        let order = TunnelOrder::Open(5173);
        assert_eq!(postcard::to_stdvec(&ServerMessage::Tunnel(order)).expect("encode")[0], 10);
        assert!(matches!(round_trip(&ServerMessage::Tunnel(order)), ServerMessage::Tunnel(o) if o == order));
    }

    #[test]
    fn the_tunnels_come_after_every_older_client_variant_and_round_trip() {
        assert_eq!(postcard::to_stdvec(&ClientMessage::Tunnels(holding())).expect("encode")[0], 9);
        assert!(matches!(round_trip(&ClientMessage::Tunnels(holding())), ClientMessage::Tunnels(h) if h == holding()));
    }

    #[test]
    fn a_client_without_tunnel_orders_reads_one_as_invalid_data() {
        assert_eq!(read_as::<BeforeTunnel>(&ServerMessage::Tunnel(TunnelOrder::Close(1))), io::ErrorKind::InvalidData);
    }

    #[test]
    fn a_server_without_tunnels_reads_them_as_invalid_data() {
        assert_eq!(read_as::<BeforeTunnels>(&ClientMessage::Tunnels(holding())), io::ErrorKind::InvalidData);
    }

    #[test]
    fn a_forget_order_and_the_closed_state_come_after_the_older_variants_and_round_trip() {
        assert_eq!(postcard::to_stdvec(&TunnelOrder::Forget(5173)).expect("encode")[0], 2);
        assert_eq!(postcard::to_stdvec(&TunnelState::Closed).expect("encode")[0], 3);
        let order = ServerMessage::Tunnel(TunnelOrder::Forget(5173));
        assert!(matches!(round_trip(&order), ServerMessage::Tunnel(TunnelOrder::Forget(5173))));
        let closed = ClientMessage::Tunnels(Holding {
            browse: false,
            orders: 0,
            tunnels: vec![Tunnel { port: 1, local: None, state: TunnelState::Closed }],
        });
        assert!(matches!(round_trip(&closed), ClientMessage::Tunnels(h) if h.tunnels[0].state == TunnelState::Closed));
    }

    #[test]
    fn a_window_without_forget_reads_one_as_an_error() {
        assert_eq!(read_as::<BeforeForget>(&TunnelOrder::Forget(1)), io::ErrorKind::InvalidData);
        assert_eq!(read_as::<BeforeClosed>(&TunnelState::Closed), io::ErrorKind::InvalidData);
    }

    fn framed(msg: &impl Serialize) -> String {
        let mut buf = Vec::new();
        send(&mut buf, msg).expect("send");
        crate::text::hex(&buf)
    }

    #[test]
    fn a_request_is_framed_as_its_fixture() {
        let request = ClientMessage::Request(r#"{"contract":1,"boot":"1-2","command":"status","args":{}}"#.into());

        assert_eq!(framed(&request), fixture!("control/postcard/request.hex").trim_end());
    }

    #[test]
    fn a_response_is_framed_as_its_fixture() {
        let response = ServerMessage::Response(r#"{"ok":{"pane":4,"tab":3}}"#.into());

        assert_eq!(framed(&response), fixture!("control/postcard/response.hex").trim_end());
    }
}

mod socket_path {
    use super::*;
    use crate::error::Error;
    use crate::os::socket::UnixListener;
    use crate::test_util::TempDir;

    #[test]
    fn a_socket_path_too_long_is_refused_before_anything_is_created() {
        let tmp = TempDir::new();
        let nested = tmp.path().join("missing");
        let name = "x".repeat(MAX_SOCKET_PATH);
        let socket = nested.join(name);

        let result = check_socket_dir(&socket, Folder::Own);

        assert!(matches!(result, Err(Error::SocketTooLong { .. })));
        assert!(!nested.exists());
    }

    #[test]
    fn the_longest_socket_path_allowed_can_listen() {
        let tmp = TempDir::new();
        let dir = tmp.path().join("s");
        let pad = MAX_SOCKET_PATH.checked_sub(dir.as_os_str().len() + 1).filter(|&pad| pad > 0).unwrap_or_else(|| {
            panic!("the temporary folder {} leaves no room for a socket name", tmp.path().display())
        });
        let socket = dir.join("x".repeat(pad));
        assert_eq!(socket.as_os_str().len(), MAX_SOCKET_PATH);
        std::fs::create_dir_all(tmp.path().join("s")).expect("folder");

        UnixListener::bind(&socket).expect("a path of the maximum length binds");
    }
}

mod peers {
    use super::*;
    use crate::error::Error;
    use crate::os::socket::UnixStream;

    #[test]
    fn a_peer_of_the_same_user_is_accepted() {
        let (ours, _theirs) = UnixStream::pair().expect("a socket pair");

        assert!(check_peer(&ours, own_uid()).is_ok());
    }

    #[test]
    fn a_peer_of_another_user_is_refused() {
        let (ours, _theirs) = UnixStream::pair().expect("a socket pair");

        let result = check_peer(&ours, own_uid().wrapping_add(1));

        assert!(matches!(result, Err(Error::OtherUser(peer)) if peer == own_uid()));
    }
}

#[cfg(windows)]
mod windows_socket_dir {
    use rstest::rstest;
    use windows_sys::Win32::Storage::FileSystem::{FILE_GENERIC_READ, FILE_GENERIC_WRITE};

    use super::*;
    use crate::error::Error;
    use crate::os::fs::{link_to, share_with_everyone};
    use crate::test_util::TempDir;

    const READ: u32 = FILE_GENERIC_READ;
    const WRITE: u32 = FILE_GENERIC_READ | FILE_GENERIC_WRITE;

    fn folder(tmp: &TempDir, shared: Option<u32>) -> PathBuf {
        let dir = tmp.path().join("sock");
        std::fs::create_dir(&dir).expect("create the folder");
        if let Some(access) = shared {
            share_with_everyone(&dir, access).expect("share the folder");
        }
        dir
    }

    fn access(dir: &Path) -> (u32, u32) {
        owner_and_mode(dir, &std::fs::symlink_metadata(dir).expect("metadata"))
    }

    #[test]
    fn a_missing_folder_is_created_private_to_the_user_even_in_a_shared_one() {
        let tmp = TempDir::new();
        let shared = folder(&tmp, Some(WRITE));
        let socket = shared.join("nested").join("server.sock");

        check_socket_dir(&socket, Folder::Given).expect("a new folder is fine");

        assert_eq!(access(&shared.join("nested")), (own_uid(), 0o700));
    }

    #[rstest]
    #[case::private(None)]
    #[case::readable_by_others(Some(READ))]
    fn a_folder_only_the_user_can_write_is_accepted_as_it_is(#[case] shared: Option<u32>) {
        let tmp = TempDir::new();
        let dir = folder(&tmp, shared);
        let before = access(&dir);

        assert!(check_socket_dir(&dir.join("server.sock"), Folder::Given).is_ok());
        assert_eq!(access(&dir), before);
    }

    #[test]
    fn a_folder_others_can_write_is_refused() {
        let tmp = TempDir::new();
        let dir = folder(&tmp, Some(WRITE));

        let result = check_socket_dir(&dir.join("server.sock"), Folder::Given);

        assert!(matches!(result, Err(Error::UnsafeSocketDir { reason: "can be written by other users", .. })));
    }

    #[rstest]
    #[case::writable_by_everyone(WRITE)]
    #[case::readable_by_everyone(READ)]
    fn cornercases_own_folder_is_made_private(#[case] shared: u32) {
        let tmp = TempDir::new();
        let dir = folder(&tmp, Some(shared));

        check_socket_dir(&dir.join("server.sock"), Folder::Own).expect("tightened");

        assert_eq!(access(&dir), (own_uid(), 0o700));
    }

    #[test]
    fn a_junction_is_refused_even_to_a_private_folder() {
        let tmp = TempDir::new();
        let dir = folder(&tmp, None);
        let link = tmp.path().join("link");
        link_to(&dir, &link).expect("a junction");

        let error = check_socket_dir(&link.join("server.sock"), Folder::Own).expect_err("refused");

        assert!(matches!(error, Error::UnsafeSocketDir { reason: "is a symbolic link", .. }));
        assert!(error.to_string().contains("CORNERCASE_SOCKET"), "{error}");
    }

    #[test]
    fn the_folder_probe_never_creates_or_changes_anything() {
        let tmp = TempDir::new();
        let missing = tmp.path().join("missing").join("server.sock");
        let open = folder(&tmp, Some(WRITE));

        assert!(!socket_dir_is_safe(&missing));
        assert!(!tmp.path().join("missing").exists());
        assert!(!socket_dir_is_safe(&open.join("server.sock")));
        assert_eq!(access(&open), (own_uid(), 0o777));
        make_private(&open).expect("made private");
        assert!(socket_dir_is_safe(&open.join("server.sock")));
    }
}

#[cfg(unix)]
mod socket_dir {
    use std::os::unix::fs::PermissionsExt;

    use rstest::rstest;

    use super::*;
    use crate::error::Error;
    use crate::test_util::TempDir;

    const ME: u32 = 501;

    fn folder(tmp: &TempDir, permissions: u32) -> PathBuf {
        let dir = tmp.path().join("sock");
        std::fs::create_dir(&dir).expect("create the folder");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(permissions)).expect("chmod");
        dir
    }

    #[test]
    fn a_missing_folder_is_created_private_to_the_user() {
        let tmp = TempDir::new();
        let socket = tmp.path().join("nested").join("server.sock");

        check_socket_dir(&socket, Folder::Given).expect("a new folder is fine");

        let mode = std::fs::metadata(tmp.path().join("nested")).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[rstest]
    #[case::private(0o700)]
    #[case::readable_by_others(0o755)]
    fn a_folder_only_the_user_can_write_is_accepted(#[case] permissions: u32) {
        let tmp = TempDir::new();
        let dir = folder(&tmp, permissions);

        assert!(check_socket_dir(&dir.join("server.sock"), Folder::Given).is_ok());
    }

    #[rstest]
    #[case::writable_by_everyone(0o777)]
    #[case::writable_by_the_group(0o770)]
    fn a_folder_others_can_write_is_refused(#[case] permissions: u32) {
        let tmp = TempDir::new();
        let dir = folder(&tmp, permissions);

        let result = check_socket_dir(&dir.join("server.sock"), Folder::Given);

        assert!(matches!(result, Err(Error::UnsafeSocketDir { reason: "can be written by other users", .. })));
    }

    #[test]
    fn a_symlink_is_refused_even_to_a_private_folder() {
        let tmp = TempDir::new();
        let dir = folder(&tmp, 0o700);
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink(&dir, &link).expect("symlink");

        let result = check_socket_dir(&link.join("server.sock"), Folder::Given);

        assert!(matches!(result, Err(Error::UnsafeSocketDir { reason: "is a symbolic link", .. })));
    }

    #[rstest]
    #[case::group_writable(0o775)]
    #[case::world_writable(0o777)]
    #[case::group_only(0o770)]
    #[case::readable_by_others(0o755)]
    fn cornercases_own_folder_is_made_private(#[case] permissions: u32) {
        let tmp = TempDir::new();
        let dir = folder(&tmp, permissions);

        check_socket_dir(&dir.join("server.sock"), Folder::Own).expect("tightened");

        let mode = std::fs::metadata(&dir).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[test]
    fn a_given_folder_keeps_its_permissions() {
        let tmp = TempDir::new();
        let dir = folder(&tmp, 0o755);

        check_socket_dir(&dir.join("server.sock"), Folder::Given).expect("accepted");

        assert_eq!(std::fs::metadata(&dir).expect("metadata").permissions().mode() & 0o777, 0o755);
    }

    #[test]
    fn own_folder_refusals_never_ask_to_remove_it() {
        let tmp = TempDir::new();
        let dir = folder(&tmp, 0o700);
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink(&dir, &link).expect("symlink");

        let error = check_socket_dir(&link.join("server.sock"), Folder::Own).expect_err("refused").to_string();

        assert!(error.contains("CORNERCASE_SOCKET"), "{error}");
        assert!(!error.contains("remove"), "{error}");
    }

    #[test]
    fn the_folder_probe_never_creates_or_changes_anything() {
        let tmp = TempDir::new();
        let missing = tmp.path().join("missing").join("server.sock");
        let open = folder(&tmp, 0o777);

        assert!(!socket_dir_is_safe(&missing));
        assert!(!tmp.path().join("missing").exists());
        assert!(!socket_dir_is_safe(&open.join("server.sock")));
        assert_eq!(std::fs::metadata(&open).expect("metadata").permissions().mode() & 0o777, 0o777);
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o700)).expect("chmod");
        assert!(socket_dir_is_safe(&open.join("server.sock")));
    }

    #[rstest]
    #[case::private(false, true, ME, 0o700, None)]
    #[case::readable_by_others(false, true, ME, 0o755, None)]
    #[case::symlink(true, false, ME, 0o755, Some("is a symbolic link"))]
    #[case::not_a_folder(false, false, ME, 0o700, Some("is not a folder"))]
    #[case::another_users(false, true, 0, 0o700, Some("belongs to another user"))]
    #[case::writable_by_the_group(false, true, ME, 0o720, Some("can be written by other users"))]
    #[case::writable_by_everyone(false, true, ME, 0o702, Some("can be written by other users"))]
    fn names_what_makes_a_folder_unsafe(
        #[case] is_symlink: bool,
        #[case] is_dir: bool,
        #[case] owner: u32,
        #[case] mode: u32,
        #[case] expected: Option<&str>,
    ) {
        assert_eq!(unsafe_dir_reason(is_symlink, is_dir, owner, mode, ME), expected);
    }
}
