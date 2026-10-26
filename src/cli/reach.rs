use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::args::{Cli, Command};
use crate::client;
use crate::error::{Error, Result};
use crate::protocol::REMOTE_ENV;
use crate::remote::Remote;

const OPTION: &str = "--remote-command";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    There,
    Here,
    Apart,
}

fn kind(command: Option<&Command>) -> (Kind, &'static str) {
    match command {
        None | Some(Command::Control(_) | Command::StartServer(_) | Command::Stats(_)) => (Kind::There, ""),
        Some(Command::Plugin(args)) if args.action.asks_the_server() => (Kind::There, ""),
        Some(Command::Plugin(_)) => (Kind::Here, "plugin"),
        Some(Command::Update(_)) => (Kind::Here, "update"),
        Some(Command::Restart(_)) => (Kind::Here, "restart"),
        Some(Command::KillServer(_)) => (Kind::Here, "kill-server"),
        Some(Command::Logs(_)) => (Kind::Here, "logs"),
        Some(Command::Remote(_)) => (Kind::Apart, "remote"),
        Some(Command::Skill) => (Kind::Apart, "skill"),
        Some(Command::Server) => (Kind::Apart, "server"),
        Some(Command::Proxy) => (Kind::Apart, "proxy"),
    }
}

fn refused(kind: Kind, name: &str, destination: &str, variable: bool) -> Error {
    let cause = if variable {
        format!(", which {REMOTE_ENV}={destination} sets: unset it, or run it")
    } else {
        ": run it".into()
    };
    Error::WrongUsage(match kind {
        Kind::Here => format!(
            "`{name}` acts on this machine, so it takes no --remote{cause} on {destination} through ssh as `cornercase {name}`"
        ),
        _ => format!("`{name}` takes no --remote"),
    })
}

pub(super) fn chosen(cli: &Cli, var: &dyn Fn(&str) -> Option<OsString>) -> Result<Option<Remote>> {
    let (kind, name) = kind(cli.command.as_ref());
    let variable = var(REMOTE_ENV).and_then(|value| value.into_string().ok()).filter(|value| !value.is_empty());
    let wanted = match (&cli.remote, variable) {
        (Some(flag), _) => Some((flag.clone(), false)),
        (None, Some(value)) if kind != Kind::Apart => Some((value, true)),
        (None, _) => None,
    };
    match (kind, wanted) {
        (Kind::There, Some((destination, _))) => {
            Ok(Some(Remote { destination, command: cli.remote_command.clone(), option: OPTION }))
        }
        (_, Some((destination, variable))) => Err(refused(kind, name, &destination, variable)),
        (_, None) if cli.remote_command.is_some() => {
            Err(Error::WrongUsage(format!("{OPTION} needs --remote DESTINATION or {REMOTE_ENV}")))
        }
        (_, None) => Ok(None),
    }
}

pub(super) fn server_path(path: &Path) -> Result<PathBuf> {
    path_on(path, client::used_remote())
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
