use rustix::process::{Pid, Signal, kill_process};

use super::Port;
use super::listen::listeners;
use crate::process::{self, escalation};

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
}

impl escalation::Stoppable for Target {
    fn check(&self) -> Result<(), String> {
        if Pid::from_raw(self.pid).is_none() {
            return Err(format!("{} is not a process", self.pid));
        }
        if self.started.is_some() && process::start_stamp(self.pid) != self.started {
            return Err(format!("pid {} is no longer the program that listened on port {}", self.pid, self.port));
        }
        if !listens(self) {
            return Err(format!("pid {} no longer listens on port {}", self.pid, self.port));
        }
        Ok(())
    }

    fn signal(&self, signal: Signal) -> Result<(), String> {
        let pid = Pid::from_raw(self.pid).ok_or_else(String::new)?;
        kill_process(pid, signal).map_err(|errno| format!("could not signal pid {}: {errno}", self.pid))
    }

    fn holds(&self) -> bool {
        let restarted = self.started.is_some() && process::start_stamp(self.pid) != self.started;
        !restarted && listens(self)
    }
}

fn listens(target: &Target) -> bool {
    listeners(&[target.pid]).iter().any(|listener| listener.port == target.port)
}

pub(crate) use crate::process::escalation::Outcome;

pub(crate) type Stopped = escalation::Stopped<Target>;

pub(crate) fn stop(targets: Vec<Target>) -> Vec<Stopped> {
    escalation::escalate(targets)
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

#[cfg(test)]
mod tests;
