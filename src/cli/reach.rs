use std::ffi::OsString;
use std::io::{IsTerminal, stdin, stdout};
use std::iter::once;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use clap::Parser as _;

use super::args::plugin::{InstallArgs, UpdatePluginArgs};
use super::args::{
    Cli, Command, KillServerArgs, LogsArgs, PluginAction, PluginArgs, Print, RestartArgs, UpdateArgs, WhenArgs,
};
use super::usage;
use crate::client;
use crate::error::{Error, Result};
use crate::protocol::REMOTE_ENV;
use crate::remote::{Remote, Run};

const OPTION: &str = "--remote-command";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    There,
    Run,
    Apart(&'static str),
    BesideTheWindow,
}

pub(super) fn kind(command: Option<&Command>) -> Kind {
    match command {
        None | Some(Command::Control(_) | Command::StartServer(_) | Command::Stats(_)) => Kind::There,
        Some(Command::Plugin(args)) if args.action.asks_the_server() => Kind::There,
        Some(Command::Plugin(args)) if matches!(args.action, PluginAction::Windows(_)) => Kind::BesideTheWindow,
        Some(
            Command::Plugin(_) | Command::Update(_) | Command::Restart(_) | Command::KillServer(_) | Command::Logs(_),
        ) => Kind::Run,
        Some(Command::Remote(_)) => Kind::Apart("remote"),
        Some(Command::Skill) => Kind::Apart("skill"),
        Some(Command::Server) => Kind::Apart("server"),
        Some(Command::Proxy) => Kind::Apart("proxy"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Refused {
    Remote,
    NoCommand,
    StartServer,
    Kind(Kind),
}

pub(super) fn checked(cli: &Cli) -> std::result::Result<(), Refused> {
    if cli.remote.is_some() || cli.remote_command.is_some() {
        return Err(Refused::Remote);
    }
    let Some(command) = &cli.command else { return Err(Refused::NoCommand) };
    if matches!(command, Command::StartServer(_)) {
        return Err(Refused::StartServer);
    }
    match kind(Some(command)) {
        Kind::There => Ok(()),
        other => Err(Refused::Kind(other)),
    }
}

const NAME_CHARS: usize = 80;

pub(crate) fn command_name(argv: &[String]) -> String {
    let mut words = argv.iter().take_while(|word| !word.starts_with('-'));
    let name = match words.next() {
        Some(first) if first == "plugin" => {
            [Some(first.as_str()), words.next().map(String::as_str)].into_iter().flatten().collect::<Vec<_>>().join(" ")
        }
        Some(first) => first.clone(),
        None => String::new(),
    };
    if name.chars().count() <= NAME_CHARS {
        return name;
    }
    let cut: String = name.chars().take(NAME_CHARS).collect();
    format!("{cut}…")
}

fn forbidden(argv: &[String], flag: &str) -> bool {
    argv.iter().any(|word| word == flag || word.strip_prefix(flag).is_some_and(|rest| rest.starts_with('=')))
}

pub(crate) fn over_cli(argv: &[String]) -> std::result::Result<(), String> {
    for flag in ["--remote", "--remote-command"] {
        if forbidden(argv, flag) {
            return Err(format!("`{flag}` is not allowed through /cli: it runs commands on this computer"));
        }
    }
    let parsed = Cli::try_parse_from(once("cornercase").chain(argv.iter().map(String::as_str)));
    let cli = match parsed {
        Ok(cli) => cli,
        Err(e) if !e.use_stderr() => return Ok(()),
        Err(e) => return Err(usage(&e).lines().next().unwrap_or_default().to_string()),
    };
    checked(&cli).map_err(|refused| match refused {
        Refused::NoCommand => "no command: /cli does not open the window".to_string(),
        Refused::StartServer => "`start-server` is not allowed through /cli: the server is already running".into(),
        Refused::Kind(_) | Refused::Remote => format!("`{}` is not allowed through /cli", command_name(argv)),
    })
}

fn refused(kind: Kind, destination: &str, variable: bool) -> Error {
    let cause = if variable {
        format!(", which {REMOTE_ENV}={destination} sets: unset it, or run it")
    } else {
        ": run it".into()
    };
    Error::WrongUsage(match kind {
        Kind::Apart(name) => format!("`{name}` takes no --remote"),
        Kind::BesideTheWindow => {
            format!("`plugin windows` runs on the machine that shows the window, so it takes no --remote{cause} there")
        }
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

fn logs_flags(logs: &LogsArgs) -> Vec<String> {
    let LogsArgs { lines, follow, path } = logs;
    if *path {
        return vec!["--path".to_string()];
    }
    let mut args: Vec<String> = follow.then(|| "--follow".to_string()).into_iter().collect();
    args.extend(["--lines".to_string(), lines.to_string()]);
    args
}

pub(super) fn there(command: &Command, remote: &Remote) -> Option<Result<Vec<String>>> {
    Some(Ok(match command {
        Command::KillServer(KillServerArgs { yes }) => words("kill-server", &[("--yes", *yes)]),
        Command::Restart(RestartArgs { yes, when }) => {
            [words("restart", &[("--yes", *yes)]), when_words(when)].concat()
        }
        Command::Update(UpdateArgs { check, yes, when, json }) => {
            [words("update", &[("--check", *check), ("--yes", *yes), ("--json", *json)]), when_words(when)].concat()
        }
        Command::Logs(logs) => [vec!["logs".to_string()], logs_flags(logs)].concat(),
        Command::Plugin(PluginArgs { action }) => return plugin_there(action, remote),
        _ => return None,
    }))
}

fn after_dashes(mut args: Vec<String>, positional: Option<&str>) -> Vec<String> {
    if let Some(positional) = positional {
        args.extend(["--".to_string(), positional.to_string()]);
    }
    args
}

fn commit_words(commit: Option<&str>) -> Vec<String> {
    commit.map(|commit| ["--commit".to_string(), commit.to_string()]).into_iter().flatten().collect()
}

fn plugin_words(action: &str, flags: &[(&str, bool)], extra: Vec<String>, positional: Option<&str>) -> Vec<String> {
    let args = [vec!["plugin".to_string()], words(action, flags), extra].concat();
    after_dashes(args, positional)
}

fn plugin_there(action: &PluginAction, remote: &Remote) -> Option<Result<Vec<String>>> {
    Some(match action {
        PluginAction::Actions { .. }
        | PluginAction::Status(_)
        | PluginAction::Run(_)
        | PluginAction::PopupClose
        | PluginAction::Windows(_) => return None,
        PluginAction::Link { folder } => open_path_on(folder, Some(remote))
            .map(|folder| plugin_words("link", &[], vec![], Some(&folder.to_string_lossy()))),
        PluginAction::Install(InstallArgs { source, yes, preview, json, commit }) => {
            let flags = [("--yes", *yes), ("--preview", *preview), ("--json", *json)];
            super::plugin::source_there(source, remote)
                .map(|source| plugin_words("install", &flags, commit_words(commit.as_deref()), Some(&source)))
        }
        PluginAction::Update(UpdatePluginArgs { id, yes, preview, json, commit }) => {
            let flags = [("--yes", *yes), ("--preview", *preview), ("--json", *json)];
            Ok(plugin_words("update", &flags, commit_words(commit.as_deref()), id.as_deref()))
        }
        PluginAction::Remove { id } => Ok(plugin_words("remove", &[], vec![], Some(id))),
        PluginAction::Unlink { id } => Ok(plugin_words("unlink", &[], vec![], Some(id))),
        PluginAction::Enable { id } => Ok(plugin_words("enable", &[], vec![], Some(id))),
        PluginAction::Disable { id } => Ok(plugin_words("disable", &[], vec![], Some(id))),
        PluginAction::List(Print { json }) => Ok(plugin_words("list", &[("--json", *json)], vec![], None)),
        PluginAction::Logs { id, logs } => Ok(plugin_words("logs", &[], logs_flags(logs), Some(id))),
    })
}

pub(super) fn run_there(remote: &Remote, args: &[String]) -> Result<bool> {
    let run = if stdin().is_terminal() && stdout().is_terminal() {
        Run::Terminal
    } else if args.iter().any(|arg| arg == "--when-idle") {
        Run::Tied
    } else {
        Run::Plain
    };
    outcome(remote, remote.run(true, run, args)?)
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
    path_on(path, client::used_remote(), cfg!(windows), &far_windows)
}

pub(super) fn far_windows() -> bool {
    client::used_remote().is_some_and(Remote::on_windows)
}

pub(super) fn open_path(path: &Path) -> Result<PathBuf> {
    if path.starts_with("~") { Ok(path.to_path_buf()) } else { server_path(path) }
}

pub(super) fn open_path_on(path: &Path, remote: Option<&Remote>) -> Result<PathBuf> {
    if path.starts_with("~") {
        return Ok(path.to_path_buf());
    }
    path_on(path, remote, cfg!(windows), &|| remote.is_some_and(Remote::on_windows))
}

pub(super) fn path_on(
    path: &Path,
    remote: Option<&Remote>,
    windows: bool,
    far_is_windows: &dyn Fn() -> bool,
) -> Result<PathBuf> {
    match remote {
        None => Ok(std::path::absolute(path)?),
        Some(remote) if windows_path(&path.to_string_lossy()) && far_is_windows() => windows_there(path, remote),
        Some(remote) => path_there(path, remote, windows),
    }
}

fn windows_there(path: &Path, remote: &Remote) -> Result<PathBuf> {
    let text = path.to_string_lossy().replace('/', "\\");
    if windows_root(&text).is_none() {
        return Err(Error::WrongUsage(format!(
            "`{}` is relative: {} is a Windows machine, so give its absolute path there, such as C:\\Users\\me\\src",
            path.display(),
            remote.host()
        )));
    }
    Ok(PathBuf::from(text))
}

pub(super) fn windows_root(text: &str) -> Option<(&str, &str)> {
    if let Some(share) = text.strip_prefix(r"\\") {
        let host = share.find('\\').filter(|at| *at > 0)?;
        let end = share[host + 1..].find('\\').map_or(share.len(), |at| host + 1 + at);
        return (end > host + 1).then(|| text.split_at(end + 2));
    }
    let mut chars = text.chars();
    let drive = chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.next() == Some(':');
    (drive && chars.next() == Some('\\')).then(|| text.split_at(2))
}

fn path_there(path: &Path, remote: &Remote, windows: bool) -> Result<PathBuf> {
    let text = path.to_string_lossy();
    if windows && windows_path(&text) {
        return Err(Error::WrongUsage(format!(
            "`{}` is a Windows path, and {} takes Unix ones, such as /srv/x; \
             Git Bash turns /srv/x into a Windows path: write //srv/x there, or set MSYS_NO_PATHCONV=1",
            path.display(),
            remote.host()
        )));
    }
    if !text.starts_with('/') {
        return Err(Error::WrongUsage(format!(
            "`{}` is relative: with --remote, give its absolute path on {}",
            path.display(),
            remote.host()
        )));
    }
    Ok(if windows { PathBuf::from(text.replace('\\', "/")) } else { path.to_path_buf() })
}

pub(super) fn windows_path(text: &str) -> bool {
    let mut chars = text.chars();
    let drive = chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.next() == Some(':');
    drive || text.starts_with('\\')
}

#[cfg(test)]
mod tests;
