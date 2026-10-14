use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::host::sound::{Cue, Kind};

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
    }
}

mod compatibility {
    use super::*;

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
    #[expect(dead_code, reason = "decoded only, the way a client from before Response reads it")]
    enum BeforeResponse {
        Rejected(String),
        Frame(Vec<u8>),
        Detached,
        Shutdown,
        Restart(PathBuf),
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
    fn a_response_comes_after_every_older_server_variant() {
        assert_eq!(postcard::to_stdvec(&ServerMessage::Response(String::new())).expect("encode"), [5, 0]);
    }

    #[test]
    fn a_client_without_responses_reads_one_as_invalid_data() {
        assert_eq!(read_as::<BeforeResponse>(&ServerMessage::Response("{}".into())), io::ErrorKind::InvalidData);
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

    #[test]
    fn a_sound_comes_after_every_older_server_variant() {
        let cue = Cue { kind: Kind::Waiting, path: String::new() };

        assert_eq!(postcard::to_stdvec(&ServerMessage::Sound(cue)).expect("encode")[0], 6);
    }

    #[test]
    fn a_client_without_sounds_reads_one_as_invalid_data() {
        let cue = Cue { kind: Kind::Done, path: "~/done.wav".into() };

        assert_eq!(read_as::<BeforeSound>(&ServerMessage::Sound(cue)), io::ErrorKind::InvalidData);
    }

    #[test]
    fn a_sound_round_trips() {
        let cue = Cue { kind: Kind::Done, path: "~/done.wav".into() };

        let msg = round_trip(&ServerMessage::Sound(cue.clone()));

        assert!(matches!(msg, ServerMessage::Sound(heard) if heard == cue));
    }
}

mod socket_dir {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixStream;

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

    fn uid() -> u32 {
        rustix::process::getuid().as_raw()
    }

    #[test]
    fn a_missing_folder_is_created_private_to_the_user() {
        let tmp = TempDir::new();
        let socket = tmp.path().join("nested").join("server.sock");

        check_socket_dir(&socket).expect("a new folder is fine");

        let mode = std::fs::metadata(tmp.path().join("nested")).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[rstest]
    #[case::private(0o700)]
    #[case::readable_by_others(0o755)]
    fn a_folder_only_the_user_can_write_is_accepted(#[case] permissions: u32) {
        let tmp = TempDir::new();
        let dir = folder(&tmp, permissions);

        assert!(check_socket_dir(&dir.join("server.sock")).is_ok());
    }

    #[rstest]
    #[case::writable_by_everyone(0o777)]
    #[case::writable_by_the_group(0o770)]
    fn a_folder_others_can_write_is_refused(#[case] permissions: u32) {
        let tmp = TempDir::new();
        let dir = folder(&tmp, permissions);

        let result = check_socket_dir(&dir.join("server.sock"));

        assert!(matches!(result, Err(Error::UnsafeSocketDir { reason: "can be written by other users", .. })));
    }

    #[test]
    fn a_symlink_is_refused_even_to_a_private_folder() {
        let tmp = TempDir::new();
        let dir = folder(&tmp, 0o700);
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink(&dir, &link).expect("symlink");

        let result = check_socket_dir(&link.join("server.sock"));

        assert!(matches!(result, Err(Error::UnsafeSocketDir { reason: "is a symbolic link", .. })));
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

    #[test]
    fn a_peer_of_the_same_user_is_accepted() {
        let (ours, _theirs) = UnixStream::pair().expect("a socket pair");

        assert!(check_peer(&ours, uid()).is_ok());
    }

    #[test]
    fn a_peer_of_another_user_is_refused() {
        let (ours, _theirs) = UnixStream::pair().expect("a socket pair");

        assert!(matches!(check_peer(&ours, uid().wrapping_add(1)), Err(Error::OtherUser(peer)) if peer == uid()));
    }
}
