use std::ffi::OsString;
use std::io::{IsTerminal, stdin, stdout};
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use super::args::{Cli, Command, KillServerArgs, LogsArgs, RestartArgs, UpdateArgs, WhenArgs};
use crate::client;
use crate::error::{Error, Result};
use crate::protocol::REMOTE_ENV;
use crate::remote::Remote;

const OPTION: &str = "--remote-command";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    There,
    Run,
    Here(&'static str),
    Apart(&'static str),
}

fn kind(command: Option<&Command>) -> Kind {
    match command {
        None | Some(Command::Control(_) | Command::StartServer(_) | Command::Stats(_)) => Kind::There,
        Some(Command::Plugin(args)) if args.action.asks_the_server() => Kind::There,
        Some(Command::Plugin(_)) => Kind::Here("plugin"),
        Some(Command::Update(_) | Command::Restart(_) | Command::KillServer(_) | Command::Logs(_)) => Kind::Run,
        Some(Command::Remote(_)) => Kind::Apart("remote"),
        Some(Command::Skill) => Kind::Apart("skill"),
        Some(Command::Server) => Kind::Apart("server"),
        Some(Command::Proxy) => Kind::Apart("proxy"),
    }
}

fn refused(kind: Kind, destination: &str, variable: bool) -> Error {
    let cause = if variable {
        format!(", which {REMOTE_ENV}={destination} sets: unset it, or run it")
    } else {
        ": run it".into()
    };
    Error::WrongUsage(match kind {
        Kind::Here(name) => format!(
            "`{name}` acts on this machine, so it takes no --remote{cause} on {destination} through ssh as `cornercase {name}`"
        ),
        Kind::Apart(name) => format!("`{name}` takes no --remote"),
        Kind::There | Kind::Run => format!("--remote {destination} was refused"),
    })
}

pub(super) fn chosen(cli: &Cli, var: &dyn Fn(&str) -> Option<OsString>) -> Result<Option<Remote>> {
    let kind = kind(cli.command.as_ref());
    let variable = var(REMOTE_ENV).and_then(|value| value.into_string().ok()).filter(|value| !value.is_empty());
    let wanted = match (&cli.remote, variable) {
        (Some(flag), _) => Some((flag.clone(), false)),
        (None, Some(value)) if !matches!(kind, Kind::Apart(_)) => Some((value, true)),
        (None, _) => None,
    };
    match (kind, wanted) {
        (Kind::There | Kind::Run, Some((destination, _))) => {
            Ok(Some(Remote { destination, command: cli.remote_command.clone(), option: OPTION }))
        }
        (_, Some((destination, variable))) => Err(refused(kind, &destination, variable)),
        (_, None) if cli.remote_command.is_some() => {
            Err(Error::WrongUsage(format!("{OPTION} needs --remote DESTINATION or {REMOTE_ENV}")))
        }
        (_, None) => Ok(None),
    }
}

fn words(name: &str, flags: &[(&str, bool)]) -> Vec<String> {
    let mut args = vec![name.to_string()];
    args.extend(flags.iter().filter(|(_, on)| *on).map(|(flag, _)| (*flag).to_string()));
    args
}

fn when_words(when: &WhenArgs) -> Vec<String> {
    let WhenArgs { when_idle, timeout } = when;
    let mut args: Vec<String> = when_idle.then(|| "--when-idle".to_string()).into_iter().collect();
    if let Some(seconds) = timeout {
        args.extend(["--timeout".to_string(), seconds.to_string()]);
    }
    args
}

fn logs_words(logs: &LogsArgs) -> Vec<String> {
    let LogsArgs { lines, follow, path } = logs;
    if *path {
        return words("logs", &[("--path", true)]);
    }
    let mut args = words("logs", &[("--follow", *follow)]);
    args.extend(["--lines".to_string(), lines.to_string()]);
    args
}

pub(super) fn there(command: &Command) -> Option<Vec<String>> {
    Some(match command {
        Command::KillServer(KillServerArgs { yes }) => words("kill-server", &[("--yes", *yes)]),
        Command::Restart(RestartArgs { yes, when }) => {
            [words("restart", &[("--yes", *yes)]), when_words(when)].concat()
        }
        Command::Update(UpdateArgs { check, yes, when }) => {
            [words("update", &[("--check", *check), ("--yes", *yes)]), when_words(when)].concat()
        }
        Command::Logs(logs) => logs_words(logs),
        _ => return None,
    })
}

pub(super) fn run_there(remote: &Remote, args: &[String]) -> Result<bool> {
    let tty = stdin().is_terminal() && stdout().is_terminal();
    outcome(remote, remote.run(true, tty, args)?)
}

fn outcome(remote: &Remote, status: ExitStatus) -> Result<bool> {
    match status.code() {
        Some(0) => Ok(true),
        Some(1) | None => Ok(false),
        Some(3) => Err(Error::Partial),
        Some(126 | 127) => Err(Error::RemoteMissing { host: remote.destination.clone(), option: remote.option }),
        Some(code) => Err(Error::Exited(u8::try_from(code).unwrap_or(1))),
    }
}

pub(super) fn server_path(path: &Path) -> Result<PathBuf> {
    path_on(path, client::used_remote())
}

pub(super) fn open_path(path: &Path) -> Result<PathBuf> {
    if path.starts_with("~") { Ok(path.to_path_buf()) } else { server_path(path) }
}

fn path_on(path: &Path, remote: Option<&Remote>) -> Result<PathBuf> {
    match remote {
        None => Ok(std::path::absolute(path)?),
        Some(_) if path.is_absolute() => Ok(path.to_path_buf()),
        Some(remote) => Err(Error::WrongUsage(format!(
            "`{}` is relative: with --remote, give its absolute path on {}",
            path.display(),
            remote.host()
        ))),
    }
}

#[cfg(test)]
mod tests;
