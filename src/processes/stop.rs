use rustix::process::{Pid, Signal, kill_process};

use crate::process;
use crate::process::escalation::{self, Outcome, Stoppable};
use crate::text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Member {
    pub(crate) pid: i32,
    pub(crate) started: Option<u64>,
    pub(crate) name: String,
}

impl Member {
    pub(crate) fn read(pid: i32) -> Self {
        Self { pid, started: process::start_stamp(pid), name: process::name(pid).unwrap_or_else(|| "?".into()) }
    }

    fn running(&self) -> bool {
        process::running(self.pid, self.started)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Group {
    pub(crate) root: Member,
    pub(crate) members: Vec<Member>,
}

pub(crate) type Stopped = escalation::Stopped<Group>;

impl Group {
    pub(crate) fn live(pid: i32) -> Self {
        Self { root: Member::read(pid), members: process::descendants(pid).into_iter().map(Member::read).collect() }
    }

    pub(crate) fn size(&self) -> usize {
        self.members.len() + 1
    }

    fn refreshed(mut self) -> Self {
        for pid in process::descendants(self.root.pid) {
            if !self.members.iter().any(|member| member.pid == pid) && pid != self.root.pid {
                self.members.push(Member::read(pid));
            }
        }
        self
    }

    pub(crate) fn label(&self) -> String {
        format!("{} (pid {})", self.root.name, self.root.pid)
    }
}

impl Stoppable for Group {
    fn check(&self) -> Result<(), String> {
        let root = &self.root;
        if !process::running(root.pid, None) {
            return Err(format!("pid {} is no longer running", root.pid));
        }
        if !root.running() {
            return Err(format!("pid {} is no longer the program that was listed", root.pid));
        }
        Ok(())
    }

    fn signal(&self, signal: Signal) -> Result<(), String> {
        let sent = |member: &Member| {
            let pid = Pid::from_raw(member.pid).ok_or_else(String::new)?;
            kill_process(pid, signal).map_err(|errno| format!("could not signal pid {}: {errno}", member.pid))
        };
        for member in self.members.iter().filter(|member| member.running()) {
            let _ = sent(member);
        }
        if self.root.running() { sent(&self.root) } else { Ok(()) }
    }

    fn holds(&self) -> bool {
        self.root.running() || self.members.iter().any(Member::running)
    }
}

pub(crate) fn inside(pid: i32, root: i32) -> bool {
    let mut at = pid;
    for _ in 0..64 {
        match process::parent(at) {
            Some(up) if up == root => return true,
            Some(up) if up > 1 && up != at => at = up,
            _ => return false,
        }
    }
    false
}

pub(crate) fn stop(group: Group) -> Vec<Stopped> {
    escalation::escalate(vec![group.refreshed()])
}

pub(crate) fn summary(stopped: &[Stopped]) -> String {
    let Some((group, outcome)) = stopped.first() else { return "nothing to stop".into() };
    let label = group.label();
    let also = match group.members.len() {
        0 => String::new(),
        n => format!(" and {} it started", text::plural(n, "program", "programs")),
    };
    match outcome {
        Ok(Outcome::Stopped) => format!("stopped {label}{also}"),
        Ok(Outcome::Killed) => format!("killed {label}{also}: it ignored SIGTERM"),
        Ok(Outcome::StillRunning) => format!("{label} is still running after SIGKILL"),
        Err(reason) => format!("did not stop {label}: {reason}"),
    }
}

#[cfg(test)]
mod tests;
