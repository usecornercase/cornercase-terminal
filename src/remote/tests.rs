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
        let args = devbox().ssh_args(&[], Run::Plain, &["proxy"]);

        assert_eq!(args[0], "-T");
        let at = args.iter().position(|a| a == "--").expect("options end before the destination");
        assert_eq!(&args[at + 1..at + 2], ["me@devbox"]);
        assert!(args[at + 2].ends_with("exec cornercase proxy'"), "{}", args[at + 2]);
        assert_eq!(args.len(), at + 3);
    }

    #[test]
    fn never_prompts_when_it_reconnects() {
        assert!(!devbox().ssh_args(&[], Run::Plain, &["proxy"]).contains(&"BatchMode=yes".to_string()));
        assert!(devbox().ssh_args(&BATCH, Run::Plain, &["proxy"]).contains(&"BatchMode=yes".to_string()));
    }

    #[test]
    fn notices_a_dead_connection() {
        assert!(devbox().ssh_args(&[], Run::Plain, &["proxy"]).contains(&"ServerAliveInterval=5".to_string()));
    }

    #[test]
    fn a_command_has_no_terminal_and_never_asks_for_a_password() {
        let args = devbox().ssh_args(&BATCH, Run::Plain, &["logs", "--path"]);

        assert_eq!(args[0], "-T");
        assert!(args.contains(&"BatchMode=yes".to_string()));
        let at = args.iter().position(|a| a == "--").expect("options end before the destination");
        assert!(args[at + 2].ends_with("exec cornercase logs --path'"), "{}", args[at + 2]);
    }

    #[test]
    fn a_terminal_is_asked_for() {
        assert_eq!(devbox().ssh_args(&[], Run::Terminal, &["update"])[0], "-t");
    }

    #[test]
    fn a_tied_command_tells_cornercase_there_that_its_input_is_the_link() {
        let remote = Remote { command: Some("env".into()), ..devbox() };
        let run = |tied| {
            let script = remote.remote_command(&[] as &[&str], tied);
            let out = Command::new("/bin/sh").args(["-c", &script]).env_remove("CORNERCASE_TIED").output().expect("sh");
            String::from_utf8_lossy(&out.stdout).into_owned()
        };

        assert!(run(true).contains("CORNERCASE_TIED=1"));
        assert!(!run(false).contains("CORNERCASE_TIED"));
    }

    #[test]
    fn the_proxy_is_never_tied() {
        let args = devbox().ssh_args(&[], Run::Plain, &["proxy"]);

        assert!(!args.iter().any(|arg| arg.contains("CORNERCASE_TIED")));
    }

    #[test]
    fn a_tied_command_has_no_terminal_and_never_asks_for_a_password() {
        let args = devbox().ssh_args(&BATCH, Run::Tied, &["restart"]);

        assert_eq!(args[0], "-T");
        assert!(args.contains(&"BatchMode=yes".to_string()));
        assert!(args.last().is_some_and(|script| script.contains("CORNERCASE_TIED=1")));
    }

    #[test]
    fn arguments_arrive_as_they_were_given() {
        let out = Command::new("/bin/sh")
            .args(["-c", &devbox().remote_command(&["logs", "it's here", "a b"], false).replace("exec ", "echo ")])
            .output()
            .expect("run sh");

        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "cornercase logs it's here a b");
    }

    #[test]
    fn looks_for_cornercase_where_installers_put_it() {
        let command = devbox().remote_command(&["proxy"], false);

        for folder in ["$HOME/.local/bin", "$HOME/.cargo/bin", "/opt/homebrew/bin"] {
            assert!(command.contains(folder), "{command}");
        }
    }

    #[test]
    fn runs_the_given_command_as_sh_reads_it() {
        let remote = Remote { command: Some("~/bin/cornercase".into()), ..devbox() };

        let out = Command::new("/bin/sh")
            .args(["-c", &remote.remote_command(&["proxy"], false).replace("exec ", "echo ")])
            .env("HOME", "/home/me")
            .output()
            .expect("run sh");

        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "/home/me/bin/cornercase proxy");
    }
}

mod one_way {
    use super::*;

    fn options(usage: Use) -> Vec<String> {
        let args = devbox().ssh_args(usage.options(), Run::Plain, &["proxy"]);
        let at = args.iter().position(|a| a == "--").expect("options end before the destination");
        assert_eq!(args[at + 1], "me@devbox");
        args[..at].to_vec()
    }

    fn with(extra: &[&str]) -> Vec<String> {
        ["-T"].iter().chain(&ALIVE).chain(extra).map(ToString::to_string).collect()
    }

    #[test]
    fn a_link_never_prompts_and_forwards_nothing() {
        let expected = with(&["-o", "BatchMode=yes", "-a", "-x", "-o", "ClearAllForwardings=yes"]);
        assert_eq!(options(Use::Link), expected);
    }

    #[rstest]
    #[case::a_window(Use::Window { again: false }, &[][..])]
    #[case::a_window_reconnecting(Use::Window { again: true }, &["-o", "BatchMode=yes"][..])]
    #[case::a_command(Use::Command, &["-o", "BatchMode=yes"][..])]
    fn a_window_and_a_command_keep_what_they_had(#[case] usage: Use, #[case] extra: &[&str]) {
        assert_eq!(options(usage), with(extra));
    }

    #[test]
    fn a_link_takes_a_newer_release_and_refuses_an_older_one() {
        let check = |release: &str| {
            devbox().check(&Banner { protocol: protocol::VERSION + 1, release: release.into() }, Use::Link)
        };
        assert!(check(CURRENT).is_ok() && check("999.0.0").is_ok());
        assert!(matches!(check("0.0.1"), Err(Error::RemoteBehind(_))));
    }
}

mod machine_update {
    use super::*;

    fn run(script: &str) -> Result<()> {
        let mut sh = Command::new("/bin/sh");
        sh.args(["-c", script]);
        devbox().installed(sh)
    }

    #[test]
    fn runs_update_like_a_link_without_a_terminal_a_prompt_or_yes() {
        let args = devbox().update_args();
        let at = args.iter().position(|a| a == "--").expect("options end before the destination");
        let options: Vec<String> = ["-T"].iter().chain(&ALIVE).chain(&LINK).map(ToString::to_string).collect();
        assert_eq!(args[..at], options);
        assert_eq!(args[at + 1], "me@devbox");
        assert!(args[at + 2].ends_with("exec cornercase update'"), "{}", args[at + 2]);
        assert_eq!(args.len(), at + 3);
    }

    #[test]
    fn that_ends_well_is_done() {
        assert!(run("echo updated cornercase; exit 0").is_ok());
    }

    #[test]
    fn that_cannot_install_says_the_last_line_it_printed() {
        let error = run("echo 'cornercase 9.9.9 is out. Update it with:'; echo 'brew upgrade cornercase'; exit 1")
            .expect_err("not installed");
        assert_eq!(reason(&error), "brew upgrade cornercase");
    }

    #[test]
    fn that_fails_says_what_cornercase_said_there() {
        let error = run("echo 'Error: no release for this machine' >&2; echo more >&2; exit 1").expect_err("failed");
        assert_eq!(reason(&error), "no release for this machine");
    }

    #[test]
    fn never_waits_for_an_answer() {
        let error = run("read answer; echo \"read $?\"; exit 1").expect_err("no answer");
        assert_eq!(reason(&error), "read 1");
    }

    #[test]
    fn without_cornercase_there_is_a_missing_cornercase() {
        assert!(matches!(run("exit 127"), Err(Error::RemoteMissing { .. })));
    }
}

mod not_found {
    use super::*;

    #[test]
    fn names_the_machine_and_no_option() {
        assert_eq!(not_found("vps"), "cornercase was not found on vps: install it there");
    }
}

mod retrying {
    use super::*;

    #[test]
    fn waits_longer_after_each_failure_up_to_a_limit() {
        let waits: Vec<u64> = (1..=7).map(|n| backoff(n).as_secs()).collect();

        assert_eq!(waits, [1, 2, 4, 8, 15, 15, 15]);
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
        devbox().check(&Banner { protocol: protocol::VERSION, release: release.into() }, Use::Window { again: false })
    }

    fn asked_by_a_command(protocol: u32, release: &str) -> Result<()> {
        devbox().check(&Banner { protocol, release: release.into() }, Use::Command)
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

        assert!(devbox().check(&banner, Use::Window { again: false }).is_err());
    }

    #[test]
    fn a_command_takes_the_same_release() {
        assert!(asked_by_a_command(protocol::VERSION, CURRENT).is_ok());
    }

    #[test]
    fn a_command_takes_a_newer_release_whatever_its_protocol() {
        assert!(asked_by_a_command(protocol::VERSION, "999.0.0").is_ok());
        assert!(asked_by_a_command(protocol::VERSION + 1, "999.0.0").is_ok());
    }

    #[test]
    fn a_command_refuses_an_older_release_and_says_how_to_update_it() {
        let message = asked_by_a_command(protocol::VERSION, "0.0.1").expect_err("too old").to_string();

        assert!(
            message.contains("runs cornercase 0.0.1") && message.contains("`cornercase --remote me@devbox update`"),
            "{message}"
        );
    }

    #[test]
    fn a_command_refuses_a_release_it_cannot_read() {
        assert!(asked_by_a_command(protocol::VERSION, "next").is_err());
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
        for n in 0..KEPT_HEAD + KEPT_LINES {
            stderr.keep(&n.to_string());
        }

        let kept = stderr.0.lock();
        assert_eq!(kept.head.len(), KEPT_HEAD);
        assert_eq!(kept.tail.len(), KEPT_LINES);
        assert_eq!(kept.tail.back().map(String::as_str), Some((KEPT_HEAD + KEPT_LINES - 1).to_string().as_str()));
    }

    #[test]
    fn a_proxy_error_followed_by_a_long_backtrace_still_says_what_failed() {
        let mut lines = vec!["Error: no cornercase server is running".to_string(), "Stack backtrace:".to_string()];
        lines.extend((0..60).map(|n| format!("  {n}: some::frame")));
        let stderr = Stderr::default();
        for line in &lines {
            stderr.keep(line);
        }

        let error = devbox().classify(Some(exited(1)), &stderr);

        assert!(matches!(error, Some(Error::RemoteNoServer(_))), "{error:?}");
        assert_eq!(stderr.last().as_deref(), Some("no cornercase server is running"));
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

mod a_killer {
    use std::os::unix::process::ExitStatusExt;

    use super::*;

    fn sleeping() -> Child {
        Command::new("/bin/sleep").arg("60").spawn().expect("run sleep")
    }

    fn ended_by(mut child: Child) -> Option<i32> {
        let _ = kill_process(Pid::from_child(&child), Signal::TERM);
        child.wait().expect("sleep ends").signal()
    }

    #[test]
    fn ends_the_ssh_it_holds() {
        let killer = Killer::default();
        let child = sleeping();

        killer.arm(&child);
        killer.kill();

        assert_eq!(ended_by(child), Some(Signal::KILL.as_raw()));
    }

    #[test]
    fn spares_the_ssh_it_let_go() {
        let killer = Killer::default();
        let child = sleeping();

        killer.arm(&child);
        killer.disarm();
        killer.kill();

        assert_eq!(ended_by(child), Some(Signal::TERM.as_raw()));
    }

    #[test]
    fn stopped_first_ends_the_ssh_as_soon_as_it_holds_it() {
        let killer = Killer::default();
        let child = sleeping();

        killer.kill();
        killer.arm(&child);

        assert_eq!(ended_by(child), Some(Signal::KILL.as_raw()));
    }
}
