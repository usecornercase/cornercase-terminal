use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde_json::Value;

use crate::control;
use crate::error::{Behind, Error, Result};
use crate::process;
use crate::protocol::{self, ClientMessage};
use crate::version::{self, CURRENT};

const SSH: &str = "ssh";
const BANNER: &str = "cornercase-proxy";
const BANNER_LINES: usize = 64;
const BANNER_BYTES: u64 = 4096;
const KEPT_HEAD: usize = 8;
const KEPT_LINES: usize = 20;
const ERROR_PREFIX: &str = "Error: ";
const KEPT_CHARS: usize = 300;
const NOT_FOUND: i32 = 127;
const CANNOT_EXECUTE: i32 = 126;
const NO_SUCH_FILE: &str = "No such file or directory";
const OLD_CLI: &str = "unrecognized subcommand";
const GARBAGE: &str = "the remote shell printed too much before cornercase started";
const SEARCHED: &str = "$PATH:$HOME/.local/bin:$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin";
const ALIVE: [&str; 6] = ["-o", "ServerAliveInterval=5", "-o", "ServerAliveCountMax=3", "-o", "ConnectTimeout=10"];
const ENDED_WAIT: Duration = Duration::from_secs(2);
const ENDED_POLL: Duration = Duration::from_millis(10);
const TIED: &str = protocol::TIED_ENV;
const BATCH: [&str; 2] = ["-o", "BatchMode=yes"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Remote {
    pub(crate) destination: String,
    pub(crate) command: Option<String>,
    pub(crate) option: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Run {
    Plain,
    Terminal,
    Tied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Use {
    Window { again: bool },
    Command,
}

impl Use {
    fn batch(self) -> bool {
        match self {
            Self::Window { again } => again,
            Self::Command => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Banner {
    protocol: u32,
    release: String,
}

#[derive(Default)]
struct Kept {
    head: Vec<String>,
    tail: VecDeque<String>,
}

#[derive(Clone, Default)]
pub(crate) struct Stderr(Arc<Mutex<Kept>>);

impl Stderr {
    pub(crate) fn last(&self) -> Option<String> {
        let kept = self.0.lock();
        let said = kept.head.iter().find_map(|line| line.strip_prefix(ERROR_PREFIX));
        said.map(str::to_string).or_else(|| kept.tail.back().or(kept.head.last()).cloned())
    }

    fn any(&self, text: &str) -> bool {
        let kept = self.0.lock();
        kept.head.iter().chain(&kept.tail).any(|line| line.contains(text))
    }

    fn keep(&self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        let line: String = line.chars().take(KEPT_CHARS).collect();
        let mut kept = self.0.lock();
        if kept.head.len() < KEPT_HEAD {
            kept.head.push(line);
            return;
        }
        if kept.tail.len() == KEPT_LINES {
            kept.tail.pop_front();
        }
        kept.tail.push_back(line);
    }

    fn collect(self, from: ChildStderr) -> JoinHandle<()> {
        thread::spawn(move || {
            BufReader::new(from).lines().map_while(io::Result::ok).for_each(|line| self.keep(&line));
        })
    }
}

pub(crate) struct Piped {
    pub(crate) reader: BufReader<ChildStdout>,
    pub(crate) writer: ChildStdin,
    pub(crate) child: Child,
    pub(crate) stderr: Stderr,
    pub(crate) collector: Option<JoinHandle<()>>,
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

    pub(crate) fn connect(&self, usage: Use) -> Result<Piped> {
        let mut child = Command::new(SSH)
            .args(self.ssh_args(usage.batch(), Run::Plain, &["proxy"]))
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
            Found::Banner(banner) => match self.check(&banner, usage) {
                Ok(()) => Ok(Piped { reader, writer: input, child, stderr, collector: Some(collector) }),
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
        Ok(self.run(false, Run::Plain, &["kill-server"])?.success())
    }

    pub(crate) fn update(&self) -> Result<bool> {
        Ok(self.run(false, Run::Terminal, &["update"])?.success())
    }

    pub(crate) fn run(&self, batch: bool, run: Run, args: &[impl AsRef<str>]) -> Result<ExitStatus> {
        let mut command = Command::new(SSH);
        command.args(self.ssh_args(batch, run, args));
        if run != Run::Tied {
            return command.status().map_err(Error::RunSsh);
        }
        let mut child = command.stdin(Stdio::piped()).spawn().map_err(Error::RunSsh)?;
        let held = child.stdin.take();
        let status = child.wait().map_err(Error::RunSsh);
        drop(held);
        status
    }

    fn ssh_args(&self, batch: bool, run: Run, args: &[impl AsRef<str>]) -> Vec<String> {
        let mut ssh = vec![if run == Run::Terminal { "-t" } else { "-T" }.to_string()];
        ssh.extend(ALIVE.iter().map(ToString::to_string));
        if batch {
            ssh.extend(BATCH.iter().map(ToString::to_string));
        }
        ssh.extend(["--".to_string(), self.destination.clone(), self.remote_command(args, run == Run::Tied)]);
        ssh
    }

    fn remote_command(&self, args: &[impl AsRef<str>], tied: bool) -> String {
        let command = self.command.as_deref().unwrap_or("cornercase");
        let words: Vec<String> = args.iter().map(|arg| quote_word(arg.as_ref())).collect();
        let tie = if tied { format!(" {TIED}=1; export {TIED};") } else { String::new() };
        let script = format!("PATH=\"{SEARCHED}\"; export PATH;{tie} exec {command} {}", words.join(" "));
        format!("sh -c {}", quote(&script))
    }

    fn check(&self, banner: &Banner, usage: Use) -> Result<()> {
        match usage {
            Use::Window { .. } => self.check_window(banner),
            Use::Command => self.check_command(banner),
        }
    }

    fn check_command(&self, banner: &Banner) -> Result<()> {
        if banner.release == CURRENT || version::newer(&banner.release, CURRENT) {
            return Ok(());
        }
        Err(Error::RemoteBehind(Box::new(Behind {
            host: self.destination.clone(),
            there: banner.release.clone(),
            update: self.update_line(),
        })))
    }

    pub(crate) fn line(&self, action: &str) -> String {
        let command = self.command.as_ref().map(|command| format!(" --remote-command {}", quote_word(command)));
        format!("cornercase --remote {}{} {action}", quote_word(&self.destination), command.unwrap_or_default())
    }

    pub(crate) fn update_line(&self) -> String {
        self.line("update")
    }

    fn check_window(&self, banner: &Banner) -> Result<()> {
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

    pub(crate) fn ended(&self, child: &mut Child, stderr: &Stderr, collector: Option<JoinHandle<()>>) -> Option<Error> {
        let deadline = Instant::now() + ENDED_WAIT;
        let status = if let Ok(Some(status)) = process::wait_until(child, deadline) {
            Some(status)
        } else {
            let _ = child.kill();
            child.wait().ok()
        };
        if let Some(collector) = collector {
            while !collector.is_finished() && Instant::now() < deadline {
                thread::sleep(ENDED_POLL);
            }
        }
        self.classify(status, stderr)
    }

    fn classify(&self, status: Option<ExitStatus>, stderr: &Stderr) -> Option<Error> {
        if status.is_some_and(|status| status.success()) {
            return None;
        }
        if stderr.any(&Error::NoServer.to_string()) {
            return Some(Error::RemoteNoServer(self.destination.clone()));
        }
        Some(self.failure(status, stderr))
    }

    fn failure(&self, status: Option<ExitStatus>, stderr: &Stderr) -> Error {
        let host = self.destination.clone();
        let code = status.and_then(|s| s.code());
        if code == Some(NOT_FOUND) || (code == Some(CANNOT_EXECUTE) && stderr.any(NO_SUCH_FILE)) {
            return Error::RemoteMissing { host, option: self.option };
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

fn quote_word(text: &str) -> String {
    if !text.is_empty() && text.chars().all(|c| c.is_ascii_alphanumeric() || "-_./@:,+=%".contains(c)) {
        text.to_string()
    } else {
        quote(text)
    }
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
            hello.openers.clear();
            ClientMessage::Hello(hello)
        }
        message => message,
    }
}

fn starts(first: &ClientMessage) -> bool {
    match first {
        ClientMessage::Hello(_) => true,
        ClientMessage::Request(text) => {
            let name = control::Command::StartServer(control::StartServer::default()).name();
            serde_json::from_str::<Value>(text).ok().as_ref().and_then(control::command_name) == Some(name)
        }
        _ => false,
    }
}

pub(crate) fn proxy(connect: impl FnOnce(bool) -> Result<UnixStream>) -> Result<()> {
    let mut out = io::stdout().lock();
    writeln!(out, "{}", banner())?;
    out.flush()?;
    let Some(first) = protocol::recv::<ClientMessage>(&mut io::stdin().lock())? else { return Ok(()) };
    let stream = connect(starts(&first))?;
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
