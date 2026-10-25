use std::thread;
use std::time::{Duration, Instant};

use rustix::process::{Pid, Signal, kill_process};

use super::Port;
use super::listen::listeners;
use crate::process;

const GRACE: Duration = Duration::from_secs(3);

const KILL_WAIT: Duration = Duration::from_secs(1);

const POLL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) pid: i32,
    pub(crate) port: u16,
    pub(crate) started: Option<u64>,
    pub(crate) program: Option<String>,
}

impl Target {
    pub(crate) fn of(port: &Port) -> Self {
        Self { pid: port.pid, port: port.number, started: port.started, program: port.program.clone() }
    }

    pub(crate) fn label(&self) -> String {
        format!("{} (pid {}) on port {}", self.program.as_deref().unwrap_or("a program"), self.pid, self.port)
    }

    fn holds(&self) -> bool {
        listeners(&[self.pid]).iter().any(|listener| listener.port == self.port)
    }

    fn check(&self) -> Result<Pid, String> {
        let pid = Pid::from_raw(self.pid).ok_or_else(|| format!("{} is not a process", self.pid))?;
        if self.started.is_some() && process::start_stamp(self.pid) != self.started {
            return Err(format!("pid {} is no longer the program that listened on port {}", self.pid, self.port));
        }
        if !self.holds() {
            return Err(format!("pid {} no longer listens on port {}", self.pid, self.port));
        }
        Ok(pid)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Stopped,
    Killed,
    StillRunning,
}

impl Outcome {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Killed => "killed",
            Self::StillRunning => "still-running",
        }
    }
}

pub(crate) type Stopped = (Target, Result<Outcome, String>);

pub(crate) fn stop(targets: Vec<Target>) -> Vec<Stopped> {
    let mut done = Vec::new();
    let mut signalled = Vec::new();
    for target in targets {
        match target.check().and_then(|pid| signal(pid, Signal::TERM)) {
            Ok(()) => signalled.push(target),
            Err(reason) => done.push((target, Err(reason))),
        }
    }
    let left = wait(signalled, GRACE, Outcome::Stopped, &mut done);
    let killed = kill(left, &mut done);
    let left = wait(killed, KILL_WAIT, Outcome::Killed, &mut done);
    done.extend(left.into_iter().map(|target| (target, Ok(Outcome::StillRunning))));
    done
}

fn kill(targets: Vec<Target>, done: &mut Vec<Stopped>) -> Vec<Target> {
    let mut killed = Vec::new();
    for target in targets {
        if target.started.is_some() && process::start_stamp(target.pid) != target.started {
            done.push((target, Ok(Outcome::Stopped)));
            continue;
        }
        match Pid::from_raw(target.pid).ok_or_else(String::new).and_then(|pid| signal(pid, Signal::KILL)) {
            Ok(()) => killed.push(target),
            Err(reason) => done.push((target, Err(reason))),
        }
    }
    killed
}

pub(crate) fn summary(stopped: &[Stopped]) -> String {
    if let [(target, outcome)] = stopped {
        return match outcome {
            Ok(Outcome::Stopped) => format!("stopped {}", target.label()),
            Ok(Outcome::Killed) => format!("killed {}: it ignored SIGTERM", target.label()),
            Ok(Outcome::StillRunning) => format!("{} is still running after SIGKILL", target.label()),
            Err(reason) => format!("did not stop {}: {reason}", target.label()),
        };
    }
    let count = |wanted: fn(&Result<Outcome, String>) -> bool| stopped.iter().filter(|(_, o)| wanted(o)).count();
    let parts = [
        (count(|o| matches!(o, Ok(Outcome::Stopped | Outcome::Killed))), "stopped"),
        (count(|o| matches!(o, Ok(Outcome::StillRunning))), "still running"),
        (count(Result::is_err), "left alone: they no longer held their port"),
    ];
    let said: Vec<String> = parts.iter().filter(|(n, _)| *n > 0).map(|(n, what)| format!("{n} {what}")).collect();
    if said.is_empty() { "nothing to stop".into() } else { said.join(", ") }
}

fn signal(pid: Pid, signal: Signal) -> Result<(), String> {
    kill_process(pid, signal).map_err(|errno| format!("could not signal pid {}: {errno}", pid.as_raw_nonzero()))
}

fn wait(mut targets: Vec<Target>, within: Duration, outcome: Outcome, done: &mut Vec<Stopped>) -> Vec<Target> {
    let until = Instant::now() + within;
    loop {
        let (released, holding): (Vec<Target>, Vec<Target>) = targets.into_iter().partition(|t| !t.holds());
        done.extend(released.into_iter().map(|target| (target, Ok(outcome))));
        targets = holding;
        if targets.is_empty() || Instant::now() >= until {
            return targets;
        }
        thread::sleep(POLL);
    }
}

#[cfg(test)]
mod tests;
