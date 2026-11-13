use std::ffi::OsStr;
use std::io::{self, Read};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::process::{Child, ExitStatus, Stdio};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use super::{ALIVE, BATCH, ENDED_WAIT, SSH, Stderr, wait_for_collector};
use crate::error::{Error, Result};
use crate::ports::listen;
use crate::process;
use crate::text;

pub(crate) const UP_WAIT: Duration = Duration::from_secs(30);
const REASON_CHARS: usize = 300;
const POLL: Duration = Duration::from_millis(100);
const ALIVE_POLL: Duration = Duration::from_millis(500);
const PROBE: Duration = Duration::from_millis(200);
const FORWARD: [&str; 6] = ["-o", "ExitOnForwardFailure=yes", "-o", "ControlPath=none", "-N", "-T"];

pub(crate) fn reason(text: &str) -> String {
    text::one_line(text).chars().take(REASON_CHARS).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Forward {
    pub(crate) port: u16,
    pub(crate) local: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Reverse {
    pub(crate) local: u16,
    pub(crate) there: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Watched {
    Up,
    Ended(String),
}

#[derive(Clone)]
enum Up {
    Listens(u16),
    Reported(Reverse, Arc<Mutex<Option<u16>>>),
}

pub(crate) struct Running {
    child: Arc<Mutex<Option<Child>>>,
    pid: i32,
    up: Up,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Owner {
    Listening,
    Starting,
    Gone,
}

enum Step {
    Stopped,
    Waiting,
    Ended(Option<ExitStatus>),
}

struct Watch {
    child: Arc<Mutex<Option<Child>>>,
    pid: i32,
    up: Up,
    wait: Duration,
    stderr: Stderr,
    collector: JoinHandle<()>,
}

pub(crate) fn args(destination: &str, forward: Forward) -> Vec<String> {
    line(destination, &[], ("-L", spec(forward)))
}

pub(crate) fn reverse_args(destination: &str, reverse: Reverse) -> Vec<String> {
    let level = if reverse.there == 0 { "LogLevel=INFO" } else { "LogLevel=DEBUG1" };
    line(destination, &["-o", level], ("-R", reverse_spec(reverse)))
}

fn line(destination: &str, options: &[&str], (flag, spec): (&str, String)) -> Vec<String> {
    let mut args: Vec<String> = FORWARD[4..].iter().map(ToString::to_string).collect();
    args.extend(ALIVE.iter().chain(&BATCH).chain(&FORWARD[..4]).chain(options).map(ToString::to_string));
    args.extend([flag.to_string(), spec, "--".to_string(), destination.to_string()]);
    args
}

fn reverse_spec(reverse: Reverse) -> String {
    format!("127.0.0.1:{}:localhost:{}", reverse.there, reverse.local)
}

pub(crate) fn reported(line: &str, reverse: Reverse) -> Option<u16> {
    let line = line.trim();
    let target = format!("localhost:{}", reverse.local);
    if reverse.there == 0 {
        let (port, said) = line.strip_prefix("Allocated port ")?.split_once(" for remote forward to ")?;
        return (said == target).then(|| port.parse().ok().filter(|port| *port != 0)).flatten();
    }
    let said = line.split_once("remote forward success for: listen ")?.1;
    let (listen, connect) = said.split_once(", connect ")?;
    let port = listen.rsplit(':').next()?;
    (connect == target && port.parse() == Ok(reverse.there)).then_some(reverse.there)
}

fn spec(forward: Forward) -> String {
    format!("127.0.0.1:{}:localhost:{}", forward.local, forward.port)
}

fn same_forward(args: &[String], destination: &str, forward: Forward) -> bool {
    let spec = spec(forward);
    let forwards = args.windows(2).any(|pair| pair[0] == "-L" && pair[1] == spec);
    let ends = args.len() >= 2 && args[args.len() - 2] == "--" && args[args.len() - 1] == destination;
    forwards && ends
}

pub(crate) fn owner(pid: i32, destination: &str, forward: Forward) -> Owner {
    if pid <= 0 || !process::running(pid, None) || !same_forward(&process::args(pid), destination, forward) {
        return Owner::Gone;
    }
    if listens(pid, forward.local) { Owner::Listening } else { Owner::Starting }
}

fn listens(pid: i32, local: u16) -> bool {
    listen::listeners(&[pid]).iter().any(|l| l.port == local && l.address.is_loopback())
}

pub(crate) fn local_port(port: u16, before: Option<u16>) -> io::Result<u16> {
    let candidates = [Some(port), before.filter(|before| *before != port)];
    if let Some(free) = candidates.into_iter().flatten().find(|candidate| !taken(*candidate)) {
        return Ok(free);
    }
    let socket = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    Ok(socket.local_addr()?.port())
}

fn taken(port: u16) -> bool {
    let answers = |address: SocketAddr| TcpStream::connect_timeout(&address, PROBE).is_ok();
    let (v4, v6) = thread::scope(|scope| {
        let v6 = scope.spawn(|| answers((Ipv6Addr::LOCALHOST, port).into()));
        (answers((Ipv4Addr::LOCALHOST, port).into()), v6.join().unwrap_or(false))
    });
    v4 || v6 || TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_err()
}

pub(crate) fn spawn(destination: &str, forward: Forward, report: impl Fn(Watched) + Send + 'static) -> Result<Running> {
    spawn_within(OsStr::new(SSH), destination, forward, UP_WAIT, report)
}

pub(crate) fn spawn_within(
    ssh: &OsStr,
    destination: &str,
    forward: Forward,
    wait: Duration,
    report: impl Fn(Watched) + Send + 'static,
) -> Result<Running> {
    start(ssh, &args(destination, forward), Up::Listens(forward.local), wait, report)
}

pub(crate) fn spawn_reverse(
    destination: &str,
    reverse: Reverse,
    report: impl Fn(Watched) + Send + 'static,
) -> Result<Running> {
    spawn_reverse_within(OsStr::new(SSH), destination, reverse, UP_WAIT, report)
}

pub(crate) fn spawn_reverse_within(
    ssh: &OsStr,
    destination: &str,
    reverse: Reverse,
    wait: Duration,
    report: impl Fn(Watched) + Send + 'static,
) -> Result<Running> {
    let up = Up::Reported(reverse, Arc::new(Mutex::new(None)));
    start(ssh, &reverse_args(destination, reverse), up, wait, report)
}

fn start(
    ssh: &OsStr,
    args: &[String],
    up: Up,
    wait: Duration,
    report: impl Fn(Watched) + Send + 'static,
) -> Result<Running> {
    let mut child = std::process::Command::new(ssh)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(Error::RunSsh)?;
    let stderr = Stderr::default();
    let collector = child.stderr.take().map(|errors| up.collect(&stderr, errors));
    let pid = i32::try_from(child.id()).unwrap_or(0);
    let child = Arc::new(Mutex::new(Some(child)));
    let collector = collector.unwrap_or_else(|| thread::spawn(|| {}));
    let watch = Watch { child: child.clone(), pid, up: up.clone(), wait, stderr, collector };
    thread::spawn(move || watch.run(&report));
    Ok(Running { child, pid, up })
}

fn is_debug_noise(line: &str) -> bool {
    line.starts_with("debug") || line.starts_with("Transferred: ") || line.starts_with("Bytes per second: ")
}

fn keeps(line: &str, reverse: Reverse, port: &Mutex<Option<u16>>) -> bool {
    let Some(there) = reported(line, reverse) else { return !is_debug_noise(line.trim_start()) };
    *port.lock() = Some(there);
    false
}

impl Up {
    fn collect(&self, stderr: &Stderr, errors: impl Read + Send + 'static) -> JoinHandle<()> {
        match self {
            Self::Listens(_) => stderr.clone().collect(errors),
            Self::Reported(reverse, port) => {
                let (reverse, port) = (*reverse, port.clone());
                stderr.clone().collect_lines(errors, move |line| keeps(line, reverse, &port))
            }
        }
    }

    fn is_up(&self, pid: i32) -> bool {
        match self {
            Self::Listens(local) => listens(pid, *local),
            Self::Reported(_, port) => port.lock().is_some(),
        }
    }

    fn gave_up(&self, seconds: u64) -> String {
        match self {
            Self::Listens(local) => format!("ssh did not listen on 127.0.0.1:{local} within {seconds} s"),
            Self::Reported(reverse, _) => {
                format!("ssh did not report the forward to localhost:{} within {seconds} s", reverse.local)
            }
        }
    }
}

impl Watch {
    fn run(self, report: &impl Fn(Watched)) {
        let started = Instant::now();
        let mut up = false;
        loop {
            match self.poll() {
                Step::Stopped => return,
                Step::Ended(status) => return report(Watched::Ended(self.reason(status))),
                Step::Waiting => {}
            }
            if !up && self.up.is_up(self.pid) {
                up = true;
                report(Watched::Up);
            }
            if !up && started.elapsed() >= self.wait {
                return self.give_up(report);
            }
            thread::sleep(if up { ALIVE_POLL } else { POLL });
        }
    }

    fn poll(&self) -> Step {
        let mut held = self.child.lock();
        let Some(child) = held.as_mut() else { return Step::Stopped };
        match child.try_wait() {
            Ok(None) => Step::Waiting,
            Ok(Some(status)) => {
                held.take();
                Step::Ended(Some(status))
            }
            Err(_) => {
                if let Some(child) = held.take() {
                    process::stop(child);
                }
                Step::Ended(None)
            }
        }
    }

    fn give_up(&self, report: &impl Fn(Watched)) {
        if let Some(child) = self.child.lock().take() {
            process::stop(child);
        }
        report(Watched::Ended(self.up.gave_up(self.wait.as_secs())));
    }

    fn reason(&self, status: Option<ExitStatus>) -> String {
        wait_for_collector(&self.collector, Instant::now() + ENDED_WAIT);
        self.stderr.reason(status)
    }
}

impl Running {
    pub(crate) fn pid(&self) -> i32 {
        self.pid
    }

    pub(crate) fn there(&self) -> Option<u16> {
        match &self.up {
            Up::Listens(_) => None,
            Up::Reported(_, port) => *port.lock(),
        }
    }

    pub(crate) fn stop(&self) {
        if let Some(child) = self.child.lock().take() {
            process::stop(child);
        }
    }
}

impl Running {
    pub(crate) fn finish(self) {
        if let Some(mut child) = self.child.lock().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests;
