use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::Shutdown;
use std::process::{ChildStderr, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use parking_lot::Mutex;

use crate::client::{self, Link};
use crate::error::{Error, Result};
use crate::protocol::{self, ClientMessage};
use crate::update::{self, CURRENT};

const SSH: &str = "ssh";
const BANNER: &str = "cornercase-proxy";
const BANNER_LINES: usize = 64;
const BANNER_BYTES: u64 = 4096;
const KEPT_LINES: usize = 20;
const KEPT_CHARS: usize = 300;
const NOT_FOUND: i32 = 127;
const CANNOT_EXECUTE: i32 = 126;
const NO_SUCH_FILE: &str = "No such file or directory";
const OLD_CLI: &str = "unrecognized subcommand";
const GARBAGE: &str = "the remote shell printed too much before cornercase started";
const SEARCHED: &str = "$PATH:$HOME/.local/bin:$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin";
const ALIVE: [&str; 6] = ["-o", "ServerAliveInterval=5", "-o", "ServerAliveCountMax=3", "-o", "ConnectTimeout=10"];
const BATCH: [&str; 2] = ["-o", "BatchMode=yes"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    pub destination: String,
    pub command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Banner {
    protocol: u32,
    release: String,
}

#[derive(Clone, Default)]
pub struct Stderr(Arc<Mutex<VecDeque<String>>>);

impl Stderr {
    pub fn last(&self) -> Option<String> {
        self.0.lock().back().cloned()
    }

    fn any(&self, text: &str) -> bool {
        self.0.lock().iter().any(|line| line.contains(text))
    }

    fn keep(&self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        let mut lines = self.0.lock();
        if lines.len() == KEPT_LINES {
            lines.pop_front();
        }
        lines.push_back(line.chars().take(KEPT_CHARS).collect());
    }

    fn collect(self, from: ChildStderr) -> JoinHandle<()> {
        thread::spawn(move || {
            for line in BufReader::new(from).lines() {
                let Ok(line) = line else { return };
                self.keep(&line);
            }
        })
    }
}

enum Found {
    Banner(Banner),
    Ended,
    Garbage,
}

impl Remote {
    pub fn host(&self) -> &str {
        &self.destination
    }

    pub fn connect(&self, again: bool) -> Result<Link> {
        let mut child = Command::new(SSH)
            .args(self.ssh_args(again, false, "proxy"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(Error::RunSsh)?;
        let (Some(input), Some(output), Some(errors)) = (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            let _ = child.kill();
            return Err(Error::RunSsh(io::Error::other("ssh started without its pipes")));
        };
        let stderr = Stderr::default();
        let collector = stderr.clone().collect(errors);
        let mut reader = BufReader::new(output);
        match read_banner(&mut reader) {
            Found::Banner(banner) => match self.check(&banner) {
                Ok(()) => Ok(Link::piped(reader, input, child, stderr)),
                Err(e) => {
                    client::stop(child);
                    Err(e)
                }
            },
            Found::Garbage => {
                client::stop(child);
                Err(Error::Ssh { host: self.destination.clone(), reason: GARBAGE.into() })
            }
            Found::Ended => {
                drop(input);
                let status = child.wait().ok();
                let _ = collector.join();
                Err(self.failure(status, &stderr))
            }
        }
    }

    pub fn kill_server(&self) -> Result<bool> {
        self.run(false, "kill-server")
    }

    pub fn update(&self) -> Result<bool> {
        self.run(true, "update")
    }

    fn run(&self, tty: bool, action: &str) -> Result<bool> {
        let status = Command::new(SSH).args(self.ssh_args(false, tty, action)).status().map_err(Error::RunSsh)?;
        Ok(status.success())
    }

    fn ssh_args(&self, batch: bool, tty: bool, action: &str) -> Vec<String> {
        let mut args = vec![if tty { "-t" } else { "-T" }.to_string()];
        args.extend(ALIVE.iter().map(ToString::to_string));
        if batch {
            args.extend(BATCH.iter().map(ToString::to_string));
        }
        args.extend(["--".to_string(), self.destination.clone(), self.remote_command(action)]);
        args
    }

    fn remote_command(&self, action: &str) -> String {
        let command = self.command.as_deref().unwrap_or("cornercase");
        let script = format!("PATH=\"{SEARCHED}\"; export PATH; exec {command} {action}");
        format!("sh -c {}", quote(&script))
    }

    fn check(&self, banner: &Banner) -> Result<()> {
        if banner.protocol == protocol::VERSION && banner.release == CURRENT {
            return Ok(());
        }
        let (host, there) = (self.destination.clone(), banner.release.clone());
        if update::newer(&there, CURRENT) {
            Err(Error::RemoteNewer { host, there })
        } else {
            Err(Error::RemoteOlder { host, there })
        }
    }

    fn failure(&self, status: Option<ExitStatus>, stderr: &Stderr) -> Error {
        let host = self.destination.clone();
        let code = status.and_then(|s| s.code());
        if code == Some(NOT_FOUND) || (code == Some(CANNOT_EXECUTE) && stderr.any(NO_SUCH_FILE)) {
            return Error::RemoteMissing(host);
        }
        if stderr.any(OLD_CLI) {
            return Error::RemoteTooOld(host);
        }
        let reason = stderr.last().unwrap_or_else(|| match status.and_then(|s| s.code()) {
            Some(code) => format!("ssh exited with {code}"),
            None => "ssh stopped".into(),
        });
        Error::Ssh { host, reason }
    }
}

pub fn fatal(error: &Error) -> bool {
    !matches!(error, Error::Ssh { .. })
}

pub fn reason(error: &Error) -> String {
    match error {
        Error::Ssh { reason, .. } => reason.clone(),
        error => error.to_string(),
    }
}

fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

fn banner() -> String {
    format!("{BANNER} {} {CURRENT}", protocol::VERSION)
}

fn parse_banner(line: &str) -> Option<Banner> {
    let mut words = line.trim().strip_prefix(BANNER)?.strip_prefix(' ')?.split_whitespace();
    let protocol = words.next()?.parse().ok()?;
    let release = words.next()?.to_string();
    Some(Banner { protocol, release })
}

fn read_banner(reader: &mut impl BufRead) -> Found {
    for _ in 0..BANNER_LINES {
        let mut line = Vec::new();
        match reader.take(BANNER_BYTES).read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return Found::Ended,
            Ok(_) => {}
        }
        if let Some(banner) = parse_banner(&String::from_utf8_lossy(&line)) {
            return Found::Banner(banner);
        }
    }
    Found::Garbage
}

fn rebuilt(message: ClientMessage, build: String) -> ClientMessage {
    match message {
        ClientMessage::Hello(mut hello) => {
            hello.build = build;
            ClientMessage::Hello(hello)
        }
        message => message,
    }
}

pub fn proxy() -> Result<()> {
    let mut out = io::stdout().lock();
    writeln!(out, "{}", banner())?;
    out.flush()?;
    let Some(first) = protocol::recv::<ClientMessage>(&mut io::stdin().lock())? else { return Ok(()) };
    let path = protocol::socket_path();
    protocol::check_socket_dir(&path)?;
    let stream = client::connect_or_start(&path)?;
    protocol::check_peer(&stream, protocol::own_uid())?;
    let mut to_server = stream.try_clone()?;
    protocol::send(&mut to_server, &rebuilt(first, protocol::build_id()))?;
    thread::spawn(move || {
        let _ = io::copy(&mut io::stdin().lock(), &mut to_server);
        let _ = to_server.shutdown(Shutdown::Write);
    });
    let mut from_server = stream;
    let mut buf = vec![0; 64 * 1024];
    loop {
        match from_server.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(n) => {
                out.write_all(&buf[..n])?;
                out.flush()?;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::host_theme::HostTheme;
    use crate::notify::Channel;
    use crate::protocol::Hello;

    fn devbox() -> Remote {
        Remote { destination: "me@devbox".into(), command: None }
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
            assert_eq!(
                parse_banner(&banner()),
                Some(Banner { protocol: protocol::VERSION, release: CURRENT.to_string() })
            );
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
        fn a_missing_command_says_how_to_point_at_it() {
            let error = devbox().failure(Some(exited(127)), &with(&["sh: 1: exec: cornercase: not found"]));

            assert!(matches!(error, Error::RemoteMissing(_)));
        }

        #[test]
        fn a_missing_path_in_bash_says_so_too() {
            let stderr = with(&["sh: line 0: exec: /opt/cornercase: cannot execute: No such file or directory"]);

            assert!(matches!(devbox().failure(Some(exited(126)), &stderr), Error::RemoteMissing(_)));
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
            let error =
                devbox().failure(Some(exited(255)), &with(&["ssh: connect to host devbox: Connection refused"]));

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

    mod proxy {
        use super::*;

        #[test]
        fn hands_the_server_its_own_build() {
            let hello = Hello {
                version: protocol::VERSION,
                build: "the laptop's".into(),
                cols: 80,
                rows: 24,
                theme: HostTheme::default(),
                notify: Channel::Bell,
                terminal: None,
            };

            let message = rebuilt(ClientMessage::Hello(Box::new(hello)), "the server's".into());

            assert!(matches!(message, ClientMessage::Hello(hello) if hello.build == "the server's"));
        }

        #[test]
        fn passes_other_messages_as_they_are() {
            assert!(matches!(rebuilt(ClientMessage::KillServer, "b".into()), ClientMessage::KillServer));
        }
    }
}
