use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use parking_lot::Mutex;

use crate::error::{Error, Result};
use crate::process;
use crate::protocol::{self, ClientMessage};
use crate::version::{self, CURRENT};

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
pub(crate) struct Remote {
    pub(crate) destination: String,
    pub(crate) command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Banner {
    protocol: u32,
    release: String,
}

#[derive(Clone, Default)]
pub(crate) struct Stderr(Arc<Mutex<VecDeque<String>>>);

impl Stderr {
    pub(crate) fn last(&self) -> Option<String> {
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

pub(crate) struct Piped {
    pub(crate) reader: BufReader<ChildStdout>,
    pub(crate) writer: ChildStdin,
    pub(crate) child: Child,
    pub(crate) stderr: Stderr,
}

enum Found {
    Banner(Banner),
    Ended,
    Garbage,
}

impl Remote {
    pub(crate) fn host(&self) -> &str {
        &self.destination
    }

    pub(crate) fn connect(&self, again: bool) -> Result<Piped> {
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
                Ok(()) => Ok(Piped { reader, writer: input, child, stderr }),
                Err(e) => {
                    process::stop(child);
                    Err(e)
                }
            },
            Found::Garbage => {
                process::stop(child);
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

    pub(crate) fn kill_server(&self) -> Result<bool> {
        self.run(false, "kill-server")
    }

    pub(crate) fn update(&self) -> Result<bool> {
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
        if version::newer(&there, CURRENT) {
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

pub(crate) fn fatal(error: &Error) -> bool {
    !matches!(error, Error::Ssh { .. })
}

pub(crate) fn reason(error: &Error) -> String {
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

pub(crate) fn proxy(connect: impl FnOnce() -> Result<UnixStream>) -> Result<()> {
    let mut out = io::stdout().lock();
    writeln!(out, "{}", banner())?;
    out.flush()?;
    let Some(first) = protocol::recv::<ClientMessage>(&mut io::stdin().lock())? else { return Ok(()) };
    let stream = connect()?;
    let mut to_server = stream.try_clone()?;
    protocol::send(&mut to_server, &rebuilt(first, version::build_id()))?;
    thread::spawn(move || {
        let _ = io::copy(&mut io::stdin().lock(), &mut to_server);
        let _ = to_server.shutdown(Shutdown::Write);
    });
    relay(stream, &mut out)
}

fn relay(mut from: impl Read + Send + 'static, to: &mut impl Write) -> Result<()> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = vec![0; 64 * 1024];
        loop {
            let read = match from.read(&mut buf) {
                Ok(0) => return,
                Ok(n) => Ok(buf[..n].to_vec()),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => Err(e),
            };
            let failed = read.is_err();
            if tx.send(read).is_err() || failed {
                return;
            }
        }
    });
    while let Ok(chunk) = rx.recv() {
        to.write_all(&chunk?)?;
        for chunk in rx.try_iter() {
            to.write_all(&chunk?)?;
        }
        to.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
