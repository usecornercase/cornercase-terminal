use rstest::rstest;

use super::*;
use crate::control;
use crate::graphics::Support;
use crate::host::notify::Channel;
use crate::host::theme::HostTheme;
use crate::protocol::Hello;

fn devbox() -> Remote {
    Remote { destination: "me@devbox".into(), command: None, option: "--command" }
}

fn exited(code: i32) -> ExitStatus {
    Command::new("/bin/sh").args(["-c", &format!("exit {code}")]).status().expect("run sh")
}

mod ssh {
    use super::*;

    #[test]
    fn runs_the_proxy_after_the_destination_without_a_terminal() {
        let args = devbox().ssh_args(false, false, "proxy");

        assert_eq!(args[0], "-T");
        let at = args.iter().position(|a| a == "--").expect("options end before the destination");
        assert_eq!(&args[at + 1..at + 2], ["me@devbox"]);
        assert!(args[at + 2].ends_with("exec cornercase proxy'"), "{}", args[at + 2]);
        assert_eq!(args.len(), at + 3);
    }

    #[test]
    fn never_prompts_when_it_reconnects() {
        assert!(!devbox().ssh_args(false, false, "proxy").contains(&"BatchMode=yes".to_string()));
        assert!(devbox().ssh_args(true, false, "proxy").contains(&"BatchMode=yes".to_string()));
    }

    #[test]
    fn notices_a_dead_connection() {
        assert!(devbox().ssh_args(false, false, "proxy").contains(&"ServerAliveInterval=5".to_string()));
    }

    #[test]
    fn looks_for_cornercase_where_installers_put_it() {
        let command = devbox().remote_command("proxy");

        for folder in ["$HOME/.local/bin", "$HOME/.cargo/bin", "/opt/homebrew/bin"] {
            assert!(command.contains(folder), "{command}");
        }
    }

    #[test]
    fn runs_the_given_command_as_sh_reads_it() {
        let remote = Remote { command: Some("~/bin/cornercase".into()), ..devbox() };

        let out = Command::new("/bin/sh")
            .args(["-c", &remote.remote_command("proxy").replace("exec ", "echo ")])
            .env("HOME", "/home/me")
            .output()
            .expect("run sh");

        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "/home/me/bin/cornercase proxy");
    }
}

mod banner {
    use super::*;

    #[test]
    fn round_trips() {
        assert_eq!(parse_banner(&banner()), Some(Banner { protocol: protocol::VERSION, release: CURRENT.to_string() }));
    }

    #[rstest]
    #[case::other_output("Welcome to devbox")]
    #[case::no_version("cornercase-proxy")]
    #[case::glued("cornercase-proxy1 0.1.0")]
    #[case::not_a_number("cornercase-proxy x 0.1.0")]
    fn ignores_lines_that_are_not_it(#[case] line: &str) {
        assert_eq!(parse_banner(line), None);
    }

    #[test]
    fn skips_what_the_remote_shell_prints_first() {
        let input = format!("motd\n\n{}\n", banner());

        assert!(matches!(read_banner(&mut input.as_bytes()), Found::Banner(_)));
    }

    #[test]
    fn leaves_what_follows_it_to_the_protocol() {
        let mut input = format!("{}\n", banner()).into_bytes();
        protocol::send(&mut input, &protocol::ServerMessage::Detached).expect("encode");
        let mut reader = BufReader::new(input.as_slice());

        assert!(matches!(read_banner(&mut reader), Found::Banner(_)));
        assert!(matches!(protocol::recv(&mut reader), Ok(Some(protocol::ServerMessage::Detached))));
    }

    #[test]
    fn gives_up_on_endless_output() {
        let input = "noise\n".repeat(BANNER_LINES + 1);

        assert!(matches!(read_banner(&mut input.as_bytes()), Found::Garbage));
    }
}

mod versions {
    use super::*;

    fn there(release: &str) -> Result<()> {
        devbox().check(&Banner { protocol: protocol::VERSION, release: release.into() })
    }

    #[test]
    fn the_same_release_attaches() {
        assert!(there(CURRENT).is_ok());
    }

    #[test]
    fn an_older_remote_is_updated_there() {
        let message = there("0.0.1").expect_err("too old").to_string();

        assert!(message.contains("runs cornercase 0.0.1") && message.ends_with("there"), "{message}");
    }

    #[test]
    fn a_newer_remote_asks_to_update_here() {
        let message = there("999.0.0").expect_err("too new").to_string();

        assert!(message.ends_with("here"), "{message}");
    }

    #[test]
    fn another_protocol_never_attaches() {
        let banner = Banner { protocol: protocol::VERSION + 1, release: CURRENT.into() };

        assert!(devbox().check(&banner).is_err());
    }
}

mod failures {
    use super::*;

    fn with(lines: &[&str]) -> Stderr {
        let stderr = Stderr::default();
        for line in lines {
            stderr.keep(line);
        }
        stderr
    }

    #[test]
    fn a_missing_command_names_the_option_given() {
        let remote = Remote { option: "--remote-command", ..devbox() };

        let error = remote.failure(Some(exited(127)), &with(&["sh: 1: exec: cornercase: not found"]));

        assert!(error.to_string().ends_with("with --remote-command"), "{error}");
    }

    #[test]
    fn a_missing_command_says_how_to_point_at_it() {
        let error = devbox().failure(Some(exited(127)), &with(&["sh: 1: exec: cornercase: not found"]));

        assert!(matches!(error, Error::RemoteMissing { .. }));
    }

    #[test]
    fn a_missing_path_in_bash_says_so_too() {
        let stderr = with(&["sh: line 0: exec: /opt/cornercase: cannot execute: No such file or directory"]);

        assert!(matches!(devbox().failure(Some(exited(126)), &stderr), Error::RemoteMissing { .. }));
    }

    #[test]
    fn a_command_that_cannot_run_shows_why() {
        let stderr = with(&["sh: line 0: exec: /opt/cornercase: cannot execute: Permission denied"]);

        assert!(matches!(devbox().failure(Some(exited(126)), &stderr), Error::Ssh { .. }));
    }

    #[test]
    fn a_cornercase_without_the_proxy_is_too_old() {
        let stderr = with(&["error: unrecognized subcommand 'proxy'", "", "For more information, try '--help'."]);

        assert!(matches!(devbox().failure(Some(exited(2)), &stderr), Error::RemoteTooOld(_)));
    }

    #[test]
    fn ssh_errors_show_its_last_line_and_are_retried() {
        let error = devbox().failure(Some(exited(255)), &with(&["ssh: connect to host devbox: Connection refused"]));

        assert_eq!(reason(&error), "ssh: connect to host devbox: Connection refused");
        assert!(!fatal(&error));
    }

    #[test]
    fn a_version_mismatch_stops_reconnecting() {
        let error = Error::RemoteOlder { host: "devbox".into(), there: "0.1.0".into() };

        assert!(fatal(&error));
    }

    #[test]
    fn keeps_only_the_last_lines() {
        let stderr = Stderr::default();
        for n in 0..=KEPT_LINES {
            stderr.keep(&n.to_string());
        }

        assert_eq!(stderr.0.lock().front().map(String::as_str), Some("1"));
        assert_eq!(stderr.0.lock().len(), KEPT_LINES);
        assert_eq!(stderr.last(), Some(KEPT_LINES.to_string()));
    }
}

mod ended {
    use super::*;

    fn with(lines: &[&str]) -> Stderr {
        let stderr = Stderr::default();
        for line in lines {
            stderr.keep(line);
        }
        stderr
    }

    fn spawned(script: &str) -> (Child, Stderr, JoinHandle<()>) {
        let mut child = Command::new("/bin/sh").args(["-c", script]).stderr(Stdio::piped()).spawn().expect("run sh");
        let stderr = Stderr::default();
        let collector = stderr.clone().collect(child.stderr.take().expect("stderr"));
        (child, stderr, collector)
    }

    #[test]
    fn a_clean_exit_keeps_the_local_meaning() {
        assert!(devbox().classify(Some(exited(0)), &Stderr::default()).is_none());
    }

    #[test]
    fn no_server_there_says_so() {
        let stderr = with(&["Error: no cornercase server is running"]);

        let error = devbox().classify(Some(exited(1)), &stderr);

        assert!(matches!(error, Some(Error::RemoteNoServer(host)) if host == "me@devbox"));
    }

    #[test]
    fn what_ssh_said_is_kept() {
        let stderr = with(&["Connection to devbox closed by remote host."]);

        let error = devbox().classify(Some(exited(255)), &stderr);

        assert!(matches!(error, Some(Error::Ssh { reason, .. }) if reason.contains("closed by remote host")));
    }

    #[test]
    fn waits_for_ssh_and_its_last_line() {
        let (mut child, stderr, collector) = spawned("echo 'Error: no cornercase server is running' >&2; exit 1");

        let error = devbox().ended(&mut child, &stderr, Some(collector));

        assert!(matches!(error, Some(Error::RemoteNoServer(_))), "{error:?}");
    }

    #[test]
    fn a_clean_end_keeps_the_local_meaning_after_waiting() {
        let (mut child, stderr, collector) = spawned("exit 0");

        assert!(devbox().ended(&mut child, &stderr, Some(collector)).is_none());
    }
}

mod proxy {
    use super::*;

    fn hello() -> Hello {
        Hello {
            zone: "UTC".into(),
            openers: vec![crate::host::open::Opener::Files],
            version: protocol::VERSION,
            build: "the laptop's".into(),
            cols: 80,
            rows: 24,
            theme: HostTheme::default(),
            notify: Channel::Bell,
            terminal: None,
            pointer: None,
            graphics: Support::default(),
            probe: String::new(),
        }
    }

    fn request(command: control::Command) -> ClientMessage {
        let request = control::Request { caller: None, server: None, contract: None, boot: None, view: None, command };
        ClientMessage::Request(serde_json::to_string(&request).expect("json"))
    }

    #[rstest]
    #[case::a_window(ClientMessage::Hello(Box::new(hello())), true)]
    #[case::start_server(request(control::Command::StartServer(control::StartServer::default())), true)]
    #[case::another_request(request(control::Command::Status(control::Status {})), false)]
    #[case::not_json(ClientMessage::Request("{".into()), false)]
    #[case::kill_server(ClientMessage::KillServer, false)]
    fn starts_a_server_only_for_a_window_or_start_server(#[case] first: ClientMessage, #[case] starts_one: bool) {
        assert_eq!(starts(&first), starts_one);
    }

    #[test]
    fn hands_the_server_its_own_build() {
        let message = rebuilt(ClientMessage::Hello(Box::new(hello())), "the server's".into());

        assert!(
            matches!(message, ClientMessage::Hello(hello) if hello.build == "the server's" && hello.openers.is_empty())
        );
    }

    #[test]
    fn passes_other_messages_as_they_are() {
        assert!(matches!(rebuilt(ClientMessage::KillServer, "b".into()), ClientMessage::KillServer));
    }

    struct Stalled {
        gate: mpsc::Receiver<()>,
        got: Arc<Mutex<Vec<u8>>>,
    }

    impl Write for Stalled {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.got.lock().is_empty() {
                let _ = self.gate.recv();
            }
            self.got.lock().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_slow_ssh_never_holds_the_server_back() {
        let (mut server, proxy) = UnixStream::pair().expect("a socket pair");
        server.set_write_timeout(Some(Duration::from_secs(1))).expect("a write timeout");
        let (open, gate) = mpsc::channel();
        let got = Arc::new(Mutex::new(Vec::new()));
        let mut ssh = Stalled { gate, got: Arc::clone(&got) };
        let relayed = thread::spawn(move || relay(proxy, &mut ssh).is_ok());
        let frame = vec![7; 4 << 20];

        server.write_all(&frame).expect("the server writes a big frame while ssh is stuck");
        drop(server);
        open.send(()).expect("ssh moves again");

        assert!(relayed.join().expect("the relay ends"));
        assert_eq!(got.lock().len(), frame.len());
    }
}
