use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::Shutdown;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde_json::Value;

use self::attempts::{Ask, Attempt, Failed};
use self::shell::RemoteShell;
use crate::control;
use crate::error::{Behind, Error, Result};
use crate::os::process::{Pid, Signal, kill_process};
use crate::os::socket::UnixStream;
use crate::process;
use crate::protocol::{self, ClientMessage};
use crate::version::{self, CURRENT};

mod attempts;
pub(crate) mod hints;
mod known;
pub(crate) mod machines;
mod shell;
pub(crate) mod tunnel;
pub(crate) mod tunnels;

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
const SSH_FAILED: i32 = 255;
const NO_SH: i32 = 1;
const NO_SUCH_FILE: &str = "No such file or directory";
const OLD_CLI: &str = "unrecognized subcommand";
const GARBAGE: &str = "the remote shell printed too much before cornercase started";
const SEARCHED: &str = "$PATH:$HOME/.local/bin:$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin";
const ALIVE: [&str; 6] = ["-o", "ServerAliveInterval=5", "-o", "ServerAliveCountMax=3", "-o", "ConnectTimeout=10"];
const ENDED_WAIT: Duration = Duration::from_secs(2);
const ENDED_POLL: Duration = Duration::from_millis(10);
const TIED: &str = protocol::TIED_ENV;
const BATCH: [&str; 2] = ["-o", "BatchMode=yes"];
const LINK: [&str; 6] = ["-o", "BatchMode=yes", "-a", "-x", "-o", "ClearAllForwardings=yes"];
const SECOND: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(15);
const BANNER_WAIT: Duration = Duration::from_secs(30);
const SILENT: &str = "cornercase did not answer within 30 s";

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
    Link,
}

impl Use {
    fn options(self) -> &'static [&'static str] {
        match self {
            Self::Window { again: false } => &[],
            Self::Window { again: true } | Self::Command => &BATCH,
            Self::Link => &LINK,
        }
    }

    fn batch(self) -> bool {
        !matches!(self, Self::Window { again: false })
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

    pub(crate) fn reason(&self, status: Option<ExitStatus>) -> String {
        self.last().unwrap_or_else(|| match status.and_then(|status| status.code()) {
            Some(code) => format!("ssh exited with {code}"),
            None => "ssh stopped".into(),
        })
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

    fn collect(self, from: impl Read + Send + 'static) -> JoinHandle<()> {
        self.collect_lines(from, |_| true)
    }

    fn collect_lines(
        self,
        from: impl Read + Send + 'static,
        mut keep: impl FnMut(&str) -> bool + Send + 'static,
    ) -> JoinHandle<()> {
        thread::spawn(move || {
            let lines = BufReader::new(from).lines().map_while(io::Result::ok);
            for line in lines.filter(|line| keep(line)) {
                self.keep(&line);
            }
        })
    }
}

pub(crate) struct Piped {
    pub(crate) release: String,
    pub(crate) reader: BufReader<ChildStdout>,
    pub(crate) writer: ChildStdin,
    pub(crate) child: Child,
    pub(crate) stderr: Stderr,
    pub(crate) collector: Option<JoinHandle<()>>,
}

#[derive(Default)]
struct Target {
    pid: Option<Pid>,
    killed: bool,
}

#[derive(Clone, Default)]
pub(crate) struct Killer(Arc<Mutex<Target>>);

impl Killer {
    pub(crate) fn kill(&self) {
        let mut target = self.0.lock();
        target.killed = true;
        if let Some(pid) = target.pid {
            let _ = kill_process(pid, Signal::KILL);
        }
    }

    pub(crate) fn arm(&self, child: &Child) {
        let mut target = self.0.lock();
        target.pid = i32::try_from(child.id()).ok().and_then(Pid::from_raw);
        if let Some(pid) = target.pid.filter(|_| target.killed) {
            let _ = kill_process(pid, Signal::KILL);
        }
    }

    pub(crate) fn disarm(&self) {
        self.0.lock().pid = None;
    }
}

enum Found<T> {
    Line(T),
    Ended,
    Garbage,
}

struct Asked<'a> {
    remote: &'a Remote,
    options: &'a [&'a str],
}

impl Ask for Asked<'_> {
    fn remembered(&mut self) -> Option<RemoteShell> {
        known::shell(&self.remote.destination)
    }

    fn probe(&mut self) -> Option<RemoteShell> {
        self.remote.probe(self.options)
    }

    fn learn(&mut self, shell: RemoteShell) {
        known::learn(&self.remote.destination, shell);
    }
}

impl Remote {
    pub(crate) fn host(&self) -> &str {
        &self.destination
    }

    fn asked<'a>(&'a self, options: &'a [&'a str]) -> Asked<'a> {
        Asked { remote: self, options }
    }

    fn probe_first(&self) -> bool {
        self.command.as_deref().is_some_and(shell::windows_program)
    }

    pub(crate) fn connect(&self, usage: Use) -> Result<Piped> {
        self.connect_killable(usage, &Killer::default())
    }

    pub(crate) fn connect_killable(&self, usage: Use, killer: &Killer) -> Result<Piped> {
        if usage == Use::Link {
            return match self.attempt(usage, RemoteShell::Posix, killer)? {
                Attempt::Linked(piped) => Ok(piped),
                Attempt::Failed(failed) => Err(self.failure(failed.status, &failed.stderr)),
            };
        }
        let mut asked = self.asked(usage.options());
        match attempts::linked(&mut asked, self.probe_first(), |shell| self.attempt(usage, shell, killer))? {
            Ok(piped) => Ok(piped),
            Err(failed) => Err(self.failure(failed.status, &failed.stderr)),
        }
    }

    fn attempt(&self, usage: Use, shell: RemoteShell, killer: &Killer) -> Result<Attempt<Piped>> {
        let line = self.command_for(shell, &["proxy"], false)?;
        let (child, input, output, errors) = spawn_piped(self.ssh_line(usage.options(), Run::Plain, line))?;
        let stderr = Stderr::default();
        let collector = stderr.clone().collect(errors);
        let mut reader = BufReader::new(output);
        let limit = usage.batch().then_some(BANNER_WAIT);
        let (child, found) = banner_of(child, &mut reader, limit, killer);
        let Some(mut child) = child else {
            return Err(Error::Ssh { host: self.destination.clone(), reason: SILENT.into() });
        };
        match found {
            Found::Line(banner) => match self.check(&banner, usage) {
                Ok(()) => Ok(Attempt::Linked(Piped {
                    release: banner.release,
                    reader,
                    writer: input,
                    child,
                    stderr,
                    collector: Some(collector),
                })),
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
                Ok(Attempt::Failed(Failed { status, stderr }))
            }
        }
    }

    fn probe(&self, options: &[&str]) -> Option<RemoteShell> {
        let mut child = Command::new(SSH)
            .args(self.ssh_line(options, Run::Plain, shell::PROBE.to_string()))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let found = child.stdout.take().map_or(Found::Garbage, |out| read_line(&mut BufReader::new(out), shell::heard));
        if !matches!(found, Found::Ended) {
            process::stop(child);
            return answered(found, None);
        }
        let code = process::wait_until(&mut child, Instant::now() + ENDED_WAIT).ok().flatten().and_then(|s| s.code());
        process::stop(child);
        answered(found, code)
    }

    pub(crate) fn on_windows(&self) -> bool {
        attempts::windows_there(&mut self.asked(&BATCH))
    }

    pub(crate) fn kill_server(&self) -> Result<bool> {
        Ok(self.run(false, Run::Plain, &["kill-server"])?.success())
    }

    pub(crate) fn update(&self) -> Result<bool> {
        Ok(self.run(false, Run::Terminal, &["update"])?.success())
    }

    pub(crate) fn install_update(&self) -> Result<()> {
        let mut ssh = Command::new(SSH);
        ssh.args(self.update_args());
        self.installed(ssh)
    }

    fn installed(&self, mut update: Command) -> Result<()> {
        let mut child =
            update.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(Error::RunSsh)?;
        let said = Stderr::default();
        let out = child.stdout.take().map(|out| said.clone().collect(out));
        let errors = child.stderr.take().map(|errors| said.clone().collect(errors));
        let status = child.wait().map_err(Error::RunSsh)?;
        for collector in [out, errors].into_iter().flatten() {
            let _ = collector.join();
        }
        if status.success() { Ok(()) } else { Err(self.failure(Some(status), &said)) }
    }

    fn update_args(&self) -> Vec<String> {
        self.ssh_line(Use::Link.options(), Run::Plain, self.remote_command(&["update"], false))
    }

    pub(crate) fn run(&self, batch: bool, run: Run, args: &[impl AsRef<str>]) -> Result<ExitStatus> {
        let words: Vec<&str> = args.iter().map(AsRef::as_ref).collect();
        let mut asked = self.asked(batched(batch));
        let ran =
            attempts::ran(&mut asked, self.probe_first(), &words, |shell| self.run_with(shell, batch, run, &words))?;
        if let Some(shell) = ran.learned {
            let then = if ran.again {
                "it ran the command again through it"
            } else {
                "run the command again, since cornercase does not repeat one that may have changed something there"
            };
            eprintln!(
                "`{}` runs commands with {}, which cornercase uses from now on: {then}",
                self.destination,
                shell.name()
            );
        }
        Ok(ran.status)
    }

    fn run_with(&self, shell: RemoteShell, batch: bool, run: Run, args: &[impl AsRef<str>]) -> Result<ExitStatus> {
        let line = self.command_for(shell, args, run == Run::Tied)?;
        let mut command = Command::new(SSH);
        command.args(self.ssh_line(batched(batch), run, line));
        let words: Vec<&str> = args.iter().map(AsRef::as_ref).collect();
        hints::status(self, command, hints::printed(&words), run == Run::Tied).map_err(Error::RunSsh)
    }

    fn ssh_line(&self, options: &[&str], run: Run, line: String) -> Vec<String> {
        let mut ssh = vec![if run == Run::Terminal { "-t" } else { "-T" }.to_string()];
        ssh.extend(ALIVE.iter().chain(options).map(ToString::to_string));
        ssh.extend(["--".to_string(), self.destination.clone(), line]);
        ssh
    }

    fn command_for(&self, shell: RemoteShell, args: &[impl AsRef<str>], tied: bool) -> Result<String> {
        let words: Vec<&str> = args.iter().map(AsRef::as_ref).collect();
        let line = match shell {
            RemoteShell::Posix => return Ok(self.remote_command(&words, tied)),
            RemoteShell::Cmd => shell::cmd_line(self.command.as_deref(), &words, tied),
            RemoteShell::PowerShell => shell::powershell_line(self.command.as_deref(), &words, tied),
        };
        line.map_err(|why| {
            Error::WrongUsage(format!(
                "cannot run cornercase on `{}` through {}: {why}",
                self.destination,
                shell.name()
            ))
        })
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
            Use::Command | Use::Link => self.check_command(banner),
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

    fn reaching(&self) -> Vec<String> {
        let mut words = vec!["cornercase".to_string(), "--remote".to_string(), self.destination.clone()];
        if let Some(command) = &self.command {
            words.push("--remote-command".to_string());
            words.push(command.clone());
        }
        words
    }

    pub(crate) fn update_words(&self) -> Vec<String> {
        let mut words = self.reaching();
        words.push("update".to_string());
        words
    }

    pub(crate) fn line(&self, action: &str) -> String {
        let words: Vec<String> = self.reaching().iter().map(|word| quote_word(word)).collect();
        format!("{} {action}", words.join(" "))
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
            wait_for_collector(&collector, deadline);
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
        Error::Ssh { host, reason: stderr.reason(status) }
    }
}

fn wait_for_collector(collector: &JoinHandle<()>, deadline: Instant) {
    while !collector.is_finished() && Instant::now() < deadline {
        thread::sleep(ENDED_POLL);
    }
}

pub(crate) fn backoff(failures: u32) -> Duration {
    SECOND.saturating_mul(1 << failures.saturating_sub(1).min(4)).min(MAX_BACKOFF)
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

pub(crate) fn not_found(name: &str) -> String {
    format!("cornercase was not found on {name}: install it there")
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

fn read_line<T>(reader: &mut impl BufRead, parse: impl Fn(&str) -> Option<T>) -> Found<T> {
    for _ in 0..BANNER_LINES {
        let mut line = Vec::new();
        match reader.take(BANNER_BYTES).read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return Found::Ended,
            Ok(_) => {}
        }
        if let Some(found) = parse(&String::from_utf8_lossy(&line)) {
            return Found::Line(found);
        }
    }
    Found::Garbage
}

fn answered(found: Found<RemoteShell>, code: Option<i32>) -> Option<RemoteShell> {
    match (found, code) {
        (Found::Line(shell), _) => Some(shell),
        (Found::Ended, Some(code)) if code != SSH_FAILED => Some(RemoteShell::Posix),
        _ => None,
    }
}

fn batched(batch: bool) -> &'static [&'static str] {
    if batch { &BATCH } else { &[] }
}

fn banner_of(
    child: Child,
    reader: &mut impl BufRead,
    limit: Option<Duration>,
    killer: &Killer,
) -> (Option<Child>, Found<Banner>) {
    killer.arm(&child);
    watched(child, limit, || {
        let found = read_line(reader, parse_banner);
        killer.disarm();
        found
    })
}

fn watched<T>(child: Child, limit: Option<Duration>, work: impl FnOnce() -> T) -> (Option<Child>, T) {
    let Some(limit) = limit else { return (Some(child), work()) };
    let held = Arc::new(Mutex::new(Some(child)));
    let (done, wait) = mpsc::channel::<()>();
    let watch = Arc::clone(&held);
    let watchdog = thread::spawn(move || {
        let late = matches!(wait.recv_timeout(limit), Err(RecvTimeoutError::Timeout));
        if let Some(child) = watch.lock().as_mut().filter(|_| late) {
            let _ = child.kill();
        }
        late
    });
    let found = work();
    drop(done);
    let late = watchdog.join().unwrap_or(false);
    let child = held.lock().take();
    match child {
        Some(child) if late => {
            process::stop(child);
            (None, found)
        }
        child => (child, found),
    }
}

fn spawn_piped(args: Vec<String>) -> Result<(Child, ChildStdin, ChildStdout, ChildStderr)> {
    let mut child = Command::new(SSH)
        .args(args)
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
    Ok((child, input, output, errors))
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
    #[cfg(windows)]
    process::handles::hand_down_nothing();
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
