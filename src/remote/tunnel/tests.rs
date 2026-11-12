use std::net::{Ipv4Addr, Ipv6Addr, TcpListener};
use std::sync::mpsc;

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
