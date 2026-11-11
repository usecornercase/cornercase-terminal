use std::process::ExitStatus;

use super::shell::RemoteShell;
use super::{CANNOT_EXECUTE, NO_SH, NO_SUCH_FILE, NOT_FOUND, OLD_CLI, SSH_FAILED, Stderr};
use crate::error::Result;

const READ_ONLY: [&[&str]; 3] = [&["logs"], &["plugin", "list"], &["plugin", "logs"]];

pub(super) enum Attempt<L> {
    Linked(L),
    Failed(Failed),
}

pub(super) struct Failed {
    pub(super) status: Option<ExitStatus>,
    pub(super) stderr: Stderr,
}

impl Failed {
    fn maybe_another_shell(&self) -> bool {
        match self.status.and_then(|status| status.code()) {
            None | Some(0 | SSH_FAILED | NOT_FOUND) => false,
            Some(CANNOT_EXECUTE) if self.stderr.any(NO_SUCH_FILE) => false,
            Some(_) => !self.stderr.any(OLD_CLI),
        }
    }
}

pub(super) trait Ask {
    fn remembered(&mut self) -> Option<RemoteShell>;
    fn probe(&mut self) -> Option<RemoteShell>;
    fn learn(&mut self, shell: RemoteShell);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Chosen {
    Remembered,
    Probed,
    Assumed,
}

pub(super) struct Ran {
    pub(super) status: ExitStatus,
    pub(super) learned: Option<RemoteShell>,
    pub(super) again: bool,
}

fn heard(ask: &mut impl Ask) -> Option<RemoteShell> {
    let shell = ask.probe()?;
    ask.learn(shell);
    Some(shell)
}

fn first(ask: &mut impl Ask, probe_first: bool) -> (RemoteShell, Chosen) {
    if let Some(shell) = ask.remembered() {
        return (shell, Chosen::Remembered);
    }
    match probe_first.then(|| heard(ask)).flatten() {
        Some(shell) => (shell, Chosen::Probed),
        None => (RemoteShell::Posix, Chosen::Assumed),
    }
}

fn other(ask: &mut impl Ask, tried: RemoteShell) -> Option<RemoteShell> {
    heard(ask).filter(|shell| *shell != tried)
}

fn stale(code: Option<i32>, chosen: Chosen, another: impl FnOnce(i32) -> bool) -> bool {
    match (code, chosen) {
        (None | Some(SSH_FAILED), _) | (_, Chosen::Probed) => false,
        (Some(_), Chosen::Remembered) => true,
        (Some(code), Chosen::Assumed) => another(code),
    }
}

pub(super) fn linked<L>(
    ask: &mut impl Ask,
    probe_first: bool,
    mut attempt: impl FnMut(RemoteShell) -> Result<Attempt<L>>,
) -> Result<std::result::Result<L, Failed>> {
    let (tried, chosen) = first(ask, probe_first);
    let failed = match attempt(tried)? {
        Attempt::Linked(link) => return Ok(Ok(link)),
        Attempt::Failed(failed) => failed,
    };
    let code = failed.status.and_then(|status| status.code());
    if !stale(code, chosen, |_| failed.maybe_another_shell()) {
        return Ok(Err(failed));
    }
    let Some(shell) = other(ask, tried) else { return Ok(Err(failed)) };
    Ok(match attempt(shell)? {
        Attempt::Linked(link) => Ok(link),
        Attempt::Failed(again) => Err(again),
    })
}

pub(super) fn ran(
    ask: &mut impl Ask,
    probe_first: bool,
    args: &[&str],
    mut run: impl FnMut(RemoteShell) -> Result<ExitStatus>,
) -> Result<Ran> {
    let (tried, chosen) = first(ask, probe_first);
    let status = run(tried)?;
    let done = Ran { status, learned: None, again: false };
    if status.success() || !stale(status.code(), chosen, |code| code == NO_SH) {
        return Ok(done);
    }
    let Some(shell) = other(ask, tried) else { return Ok(done) };
    if !READ_ONLY.iter().any(|words| args.starts_with(words)) {
        return Ok(Ran { learned: Some(shell), ..done });
    }
    Ok(Ran { status: run(shell)?, learned: Some(shell), again: true })
}

pub(super) fn windows_there(ask: &mut impl Ask) -> bool {
    ask.remembered().or_else(|| heard(ask)).is_some_and(RemoteShell::windows)
}

#[cfg(test)]
mod tests;
