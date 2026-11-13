use std::net::{Ipv4Addr, Ipv6Addr, TcpListener};
use std::sync::mpsc;

use rstest::rstest;

use super::*;
use crate::test_util::{TempDir, fake_program, wait_until};

fn free_port() -> u16 {
    let socket = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind a port");
    socket.local_addr().expect("the port").port()
}

fn pid_of(running: &Running) -> i32 {
    let child = running.child.lock();
    i32::try_from(child.as_ref().expect("ssh runs").id()).expect("a pid")
}

mod the_ssh_line {
    use super::*;

    #[test]
    fn holds_the_forward_with_a_double_dash_before_the_destination() {
        let line = args("vps", Forward { port: 5173, local: 5174 });

        let expected = [
            "-N",
            "-T",
            "-o",
            "ServerAliveInterval=5",
            "-o",
            "ServerAliveCountMax=3",
            "-o",
            "ConnectTimeout=10",
            "-o",
            "BatchMode=yes",
            "-o",
            "ExitOnForwardFailure=yes",
            "-o",
            "ControlPath=none",
            "-L",
            "127.0.0.1:5174:localhost:5173",
            "--",
            "vps",
        ];
        assert_eq!(line, expected);
        assert!(!line.iter().any(|word| word.contains("ClearAllForwardings")));
    }

    #[test]
    fn a_reverse_tunnel_asks_the_machine_for_a_port_at_info_level() {
        let line = reverse_args("vps", Reverse { local: 9222, there: 0 });

        let expected = [
            "-N",
            "-T",
            "-o",
            "ServerAliveInterval=5",
            "-o",
            "ServerAliveCountMax=3",
            "-o",
            "ConnectTimeout=10",
            "-o",
            "BatchMode=yes",
            "-o",
            "ExitOnForwardFailure=yes",
            "-o",
            "ControlPath=none",
            "-o",
            "LogLevel=INFO",
            "-R",
            "127.0.0.1:0:localhost:9222",
            "--",
            "vps",
        ];
        assert_eq!(line, expected);
        assert!(!line.iter().any(|word| word.contains("ClearAllForwardings") || word == "-L"));
    }

    #[test]
    fn a_reverse_tunnel_on_a_chosen_port_asks_for_debug_lines() {
        let line = reverse_args("vps", Reverse { local: 9222, there: 9223 });

        assert!(line.iter().any(|word| word == "LogLevel=DEBUG1"));
        assert!(line.iter().any(|word| word == "127.0.0.1:9223:localhost:9222"));
    }
}

mod the_report {
    use super::*;

    const ANY: Reverse = Reverse { local: 9222, there: 0 };
    const CHOSEN: Reverse = Reverse { local: 9222, there: 9223 };

    #[rstest]
    #[case::allocated(ANY, "Allocated port 41234 for remote forward to localhost:9222", Some(41234))]
    #[case::with_a_carriage_return(ANY, "Allocated port 41234 for remote forward to localhost:9222\r", Some(41234))]
    #[case::another_forward(ANY, "Allocated port 41234 for remote forward to localhost:9333", None)]
    #[case::port_zero(ANY, "Allocated port 0 for remote forward to localhost:9222", None)]
    #[case::success(
        CHOSEN,
        "debug1: remote forward success for: listen 127.0.0.1:9223, connect localhost:9222",
        Some(9223)
    )]
    #[case::old_wording(CHOSEN, "debug1: remote forward success for: listen 9223, connect localhost:9222", Some(9223))]
    #[case::another_port(
        CHOSEN,
        "debug1: remote forward success for: listen 127.0.0.1:9224, connect localhost:9222",
        None
    )]
    #[case::debug_line_while_asking_for_any(
        ANY,
        "debug1: remote forward success for: listen 127.0.0.1:0, connect localhost:9222",
        None
    )]
    #[case::failure(CHOSEN, "Error: remote port forwarding failed for listen port 9223", None)]
    #[case::empty(ANY, "", None)]
    #[case::junk(CHOSEN, "hello", None)]
    fn is_read_from_ssh_s_own_words(#[case] reverse: Reverse, #[case] line: &str, #[case] port: Option<u16>) {
        assert_eq!(reported(line, reverse), port);
    }
}

mod the_same_forward {
    use super::*;

    const FORWARD: Forward = Forward { port: 5173, local: 5174 };

    fn words(line: &[&str]) -> Vec<String> {
        line.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn is_the_line_the_window_builds() {
        assert!(same_forward(&args("vps", FORWARD), "vps", FORWARD));
    }

    #[test]
    fn is_also_the_line_after_other_words() {
        let spec = spec(FORWARD);
        let line = words(&["python3", "forward.py", &spec, "-N", "-L", &spec, "--", "vps"]);

        assert!(same_forward(&line, "vps", FORWARD));
    }

    #[rstest]
    #[case::another_local(Forward { port: 5173, local: 9999 }, "vps")]
    #[case::another_port(Forward { port: 3000, local: 5174 }, "vps")]
    #[case::another_destination(FORWARD, "other")]
    fn is_not_a_line_of_another_forward(#[case] forward: Forward, #[case] destination: &str) {
        assert!(!same_forward(&args("vps", FORWARD), destination, forward));
    }

    #[test]
    fn is_not_a_line_without_the_double_dash() {
        let mut line = args("vps", FORWARD);
        line.retain(|word| word != "--");

        assert!(!same_forward(&line, "vps", FORWARD));
    }
}

mod the_owner {
    use super::*;

    fn ssh_in(dir: &TempDir) -> std::path::PathBuf {
        let ssh = fake_program(dir.path(), "ssh");
        std::fs::write(ssh.with_extension("hold"), "").expect("hold it");
        ssh
    }

    fn start(ssh: &std::path::Path, forward: Forward) -> (Running, mpsc::Receiver<Watched>) {
        let (tx, rx) = mpsc::channel();
        let report = move |watched| {
            let _ = tx.send(watched);
        };
        (spawn_within(ssh.as_os_str(), "vps", forward, UP_WAIT, report).expect("start ssh"), rx)
    }

    #[test]
    fn is_listening_once_ssh_listens_and_gone_for_another_destination() {
        let dir = TempDir::new();
        let ssh = ssh_in(&dir);
        std::fs::write(ssh.with_extension("forward"), "").expect("make it listen");
        let forward = Forward { port: 5173, local: free_port() };
        let (running, reports) = start(&ssh, forward);
        assert_eq!(reports.recv_timeout(Duration::from_secs(20)), Ok(Watched::Up));

        assert_eq!(owner(running.pid(), "vps", forward), Owner::Listening);
        assert_eq!(owner(running.pid(), "other", forward), Owner::Gone);
    }

    #[test]
    fn is_starting_while_ssh_has_not_listened_yet() {
        let dir = TempDir::new();
        let ssh = ssh_in(&dir);
        let forward = Forward { port: 5173, local: free_port() };
        let (running, _reports) = start(&ssh, forward);

        assert_eq!(owner(running.pid(), "vps", forward), Owner::Starting);
    }

    #[test]
    fn is_gone_once_ssh_is_stopped_and_for_no_pid() {
        let dir = TempDir::new();
        let ssh = ssh_in(&dir);
        let forward = Forward { port: 5173, local: free_port() };
        let (running, _reports) = start(&ssh, forward);
        let pid = running.pid();
        assert_eq!(pid, pid_of(&running));

        running.stop();

        wait_until("the owner is gone", || owner(pid, "vps", forward) == Owner::Gone);
        assert_eq!(owner(0, "vps", forward), Owner::Gone);
    }
}

mod the_local_port {
    use super::*;

    #[test]
    fn is_the_remote_port_when_it_is_free() {
        let port = free_port();

        assert_eq!(local_port(port, None).expect("a port"), port);
    }

    #[test]
    fn is_another_one_when_a_program_listens_on_the_loopback() {
        let held = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("listen");
        let port = held.local_addr().expect("the port").port();

        let chosen = local_port(port, None).expect("a port");

        assert_ne!(chosen, port);
        TcpListener::bind((Ipv4Addr::LOCALHOST, chosen)).expect("the chosen port is free");
    }

    #[test]
    fn is_another_one_when_a_program_listens_on_the_wildcard_address() {
        let held = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).expect("listen");
        let port = held.local_addr().expect("the port").port();

        assert_ne!(local_port(port, None).expect("a port"), port);
    }

    #[test]
    fn is_another_one_when_a_program_listens_on_the_ipv6_loopback() {
        let Ok(held) = TcpListener::bind((Ipv6Addr::LOCALHOST, 0)) else { return };
        let port = held.local_addr().expect("the port").port();

        assert_ne!(local_port(port, None).expect("a port"), port);
    }

    #[test]
    fn is_the_remembered_one_when_the_port_is_taken_and_it_is_free() {
        let held = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("listen");
        let port = held.local_addr().expect("the port").port();
        let before = free_port();

        assert_eq!(local_port(port, Some(before)).expect("a port"), before);
    }
}

mod running {
    use super::*;

    fn ssh_in(dir: &TempDir) -> std::path::PathBuf {
        let ssh = fake_program(dir.path(), "ssh");
        std::fs::write(ssh.with_extension("forward"), "").expect("make it listen");
        ssh
    }

    fn start(ssh: &std::path::Path, forward: Forward, wait: Duration) -> (Running, mpsc::Receiver<Watched>) {
        let (tx, rx) = mpsc::channel();
        let report = move |watched| {
            let _ = tx.send(watched);
        };
        (spawn_within(ssh.as_os_str(), "vps", forward, wait, report).expect("start ssh"), rx)
    }

    #[test]
    fn is_up_once_ssh_listens_on_the_local_port() {
        let dir = TempDir::new();
        let ssh = ssh_in(&dir);
        std::fs::write(ssh.with_extension("hold"), "").expect("hold it");
        let forward = Forward { port: 5173, local: free_port() };

        let (running, reports) = start(&ssh, forward, UP_WAIT);

        assert_eq!(reports.recv_timeout(Duration::from_secs(20)), Ok(Watched::Up));
        let listening = listen::listeners(&[pid_of(&running)]);
        assert!(listening.iter().any(|listener| listener.port == forward.local));
        assert_eq!(running.there(), None);
    }

    #[test]
    fn ended_with_the_line_ssh_printed_when_it_exits_before_it_is_up() {
        let dir = TempDir::new();
        let ssh = fake_program(dir.path(), "ssh");
        std::fs::write(ssh.with_extension("err"), "bind [127.0.0.1]:5174: Address already in use\n").expect("err");
        std::fs::write(ssh.with_extension("code"), "255").expect("code");

        let (_running, reports) = start(&ssh, Forward { port: 5173, local: 5174 }, UP_WAIT);

        let ended = Watched::Ended("bind [127.0.0.1]:5174: Address already in use".into());
        assert_eq!(reports.recv_timeout(Duration::from_secs(20)), Ok(ended));
    }

    #[test]
    fn stopping_it_frees_the_port_and_reports_nothing_more() {
        let dir = TempDir::new();
        let ssh = ssh_in(&dir);
        std::fs::write(ssh.with_extension("hold"), "").expect("hold it");
        let forward = Forward { port: 5173, local: free_port() };
        let (running, reports) = start(&ssh, forward, UP_WAIT);
        assert_eq!(reports.recv_timeout(Duration::from_secs(20)), Ok(Watched::Up));

        running.stop();

        wait_until("the port is free again", || TcpListener::bind((Ipv4Addr::LOCALHOST, forward.local)).is_ok());
        assert!(reports.recv_timeout(Duration::from_secs(1)).is_err());
    }

    #[test]
    fn gives_up_when_ssh_never_listens() {
        let dir = TempDir::new();
        let ssh = fake_program(dir.path(), "ssh");
        std::fs::write(ssh.with_extension("hold"), "").expect("hold it");

        let (_running, reports) = start(&ssh, Forward { port: 5173, local: 5174 }, Duration::from_millis(300));

        let Ok(Watched::Ended(reason)) = reports.recv_timeout(Duration::from_secs(20)) else {
            panic!("the tunnel should end");
        };
        assert!(reason.starts_with("ssh did not listen on 127.0.0.1:5174 within"), "{reason}");
    }
}

mod reversed {
    use super::*;

    const ANY: Reverse = Reverse { local: 9222, there: 0 };

    fn ssh_saying(dir: &TempDir, said: &str) -> std::path::PathBuf {
        let ssh = fake_program(dir.path(), "ssh");
        std::fs::write(ssh.with_extension("err"), said).expect("err");
        ssh
    }

    fn hold(ssh: &std::path::Path) {
        std::fs::write(ssh.with_extension("hold"), "").expect("hold it");
    }

    fn start(ssh: &std::path::Path, reverse: Reverse, wait: Duration) -> (Running, mpsc::Receiver<Watched>) {
        let (tx, rx) = mpsc::channel();
        let report = move |watched| {
            let _ = tx.send(watched);
        };
        (spawn_reverse_within(ssh.as_os_str(), "vps", reverse, wait, report).expect("start ssh"), rx)
    }

    #[test]
    fn is_up_once_ssh_reports_the_port_the_machine_picked() {
        let dir = TempDir::new();
        let ssh = ssh_saying(&dir, "Allocated port 41234 for remote forward to localhost:9222\n");
        hold(&ssh);

        let (running, reports) = start(&ssh, ANY, UP_WAIT);

        assert_eq!(reports.recv_timeout(Duration::from_secs(20)), Ok(Watched::Up));
        assert_eq!(running.there(), Some(41234));
        let args = std::fs::read_to_string(ssh.with_extension("args")).expect("args");
        assert!(args.contains("-R 127.0.0.1:0:localhost:9222"), "{args}");
    }

    #[test]
    fn is_up_on_a_chosen_port_once_ssh_reports_the_forward() {
        let dir = TempDir::new();
        let said = "debug1: remote forward success for: listen 127.0.0.1:9223, connect localhost:9222\n";
        let ssh = ssh_saying(&dir, said);
        hold(&ssh);

        let (running, reports) = start(&ssh, Reverse { local: 9222, there: 9223 }, UP_WAIT);

        assert_eq!(reports.recv_timeout(Duration::from_secs(20)), Ok(Watched::Up));
        assert_eq!(running.there(), Some(9223));
    }

    #[test]
    fn ends_with_ssh_s_line_when_the_port_is_taken_there() {
        let dir = TempDir::new();
        let ssh = ssh_saying(&dir, "Error: remote port forwarding failed for listen port 9223\n");
        std::fs::write(ssh.with_extension("code"), "255").expect("code");

        let (_running, reports) = start(&ssh, Reverse { local: 9222, there: 9223 }, UP_WAIT);

        let ended = Watched::Ended("remote port forwarding failed for listen port 9223".into());
        assert_eq!(reports.recv_timeout(Duration::from_secs(20)), Ok(ended));
    }

    #[test]
    fn a_debug_line_or_the_report_is_never_the_reason() {
        let dir = TempDir::new();
        let ssh = ssh_saying(&dir, "debug1: something\nAllocated port 41234 for remote forward to localhost:9222\n");
        std::fs::write(ssh.with_extension("code"), "255").expect("code");

        let (_running, reports) = start(&ssh, ANY, UP_WAIT);

        let heard: Vec<Watched> = reports.iter().take(2).collect();
        let ended = heard.iter().find(|watched| matches!(watched, Watched::Ended(_)));
        assert_eq!(ended, Some(&Watched::Ended("ssh exited with 255".into())), "{heard:?}");
    }

    #[test]
    fn gives_up_when_ssh_never_reports() {
        let dir = TempDir::new();
        let ssh = ssh_saying(&dir, "");
        hold(&ssh);

        let (_running, reports) = start(&ssh, ANY, Duration::from_millis(300));

        let Ok(Watched::Ended(reason)) = reports.recv_timeout(Duration::from_secs(20)) else {
            panic!("the tunnel should end");
        };
        assert!(reason.starts_with("ssh did not report the forward to localhost:9222 within"), "{reason}");
    }
}
