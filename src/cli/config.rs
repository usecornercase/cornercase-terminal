use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{IsTerminal as _, Read as _};
use std::path::PathBuf;

use serde_json::Value;

use super::args::config::{ConfigAction, ConfigArgs};
use super::{print_json, print_out};
use crate::client::{answer, ask};
use crate::control::Command;
use crate::control::config::{Configure, Connection, Described, Kind, ProjectChange, SecretChange, Setting, State};
use crate::error::{Error, Result};
use crate::issues::Secret;

mod share;

fn request(change: Configure) -> Result<Value> {
    ask("config", Command::Config(change))
}

pub(super) fn run(args: ConfigArgs) -> Result<()> {
    let changing = args.action.is_some();
    let value = match args.action {
        None => request(Configure::Get)?,
        Some(ConfigAction::Set { pairs }) => request(Configure::Set(converted(&pairs)?))?,
        Some(ConfigAction::Project { path, project, setup, teardown, builds }) => {
            let path = match (&path, project) {
                (None, None) => return Err(Error::WrongUsage("give the project's FOLDER or --project ID".into())),
                (path, _) => path.as_deref().map(absolute).transpose()?,
            };
            let builds = builds.as_deref().map(builds_of).transpose()?;
            request(Configure::Project(ProjectChange { project, path, setup, teardown, builds }))?
        }
        Some(ConfigAction::Secret { key }) => {
            let value = Secret(read_secret(&key)?);
            request(Configure::Secret(SecretChange { key, value }))?
        }
        Some(ConfigAction::Remove { key }) => request(Configure::Remove(key))?,
        Some(ConfigAction::Export { to }) => return share::export(to.as_deref(), args.json),
        Some(ConfigAction::Import { source, only, yes, preview }) => {
            return share::import(&share::Import { source, only, yes, preview }, args.json);
        }
    };
    if args.json {
        return print_json(&value);
    }
    print_out(&render(&answer::<Described>(value)?, changing))
}

fn absolute(path: &str) -> Result<PathBuf> {
    if path.starts_with('~') || std::path::Path::new(path).is_absolute() {
        return Ok(PathBuf::from(path));
    }
    super::reach::server_path(std::path::Path::new(path))
}

fn converted(pairs: &[String]) -> Result<BTreeMap<String, Value>> {
    if !pairs.len().is_multiple_of(2) {
        return Err(Error::WrongUsage("`config set` takes KEY VALUE pairs, and the last key has no value".into()));
    }
    let described: Described = answer(request(Configure::Get)?)?;
    let mut values = BTreeMap::new();
    for pair in pairs.chunks(2) {
        let key = &pair[0];
        let Some(setting) = described.settings.iter().find(|s| s.key == *key) else {
            let message = if described.connections.iter().any(|c| c.key == *key) {
                format!(
                    "`{key}` is a connection, not a setting: use `cornercase config secret {key}` or `remove {key}`"
                )
            } else {
                format!("no setting `{key}`; `cornercase config` lists them")
            };
            return Err(Error::WrongUsage(message));
        };
        values.insert(key.clone(), convert(setting, &pair[1])?);
    }
    Ok(values)
}

fn convert(setting: &Setting, text: &str) -> Result<Value> {
    let wrong = |what: &str| Error::WrongUsage(format!("`{}` takes {what}", setting.key));
    Ok(match setting.kind {
        Kind::Switch => match text.to_lowercase().as_str() {
            "true" | "on" | "yes" | "1" => Value::Bool(true),
            "false" | "off" | "no" | "0" => Value::Bool(false),
            _ => return Err(wrong("true or false")),
        },
        Kind::Number => Value::from(text.trim().parse::<u64>().map_err(|_| wrong("a whole number"))?),
        Kind::List if text.trim_start().starts_with('[') => {
            serde_json::from_str(text).map_err(|_| wrong("a comma-separated list or a JSON array"))?
        }
        Kind::List => Value::Array(
            text.split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(|item| Value::String(item.into()))
                .collect(),
        ),
        Kind::Choice | Kind::Text | Kind::Key | Kind::Other => Value::String(text.to_string()),
    })
}

fn read_secret(key: &str) -> Result<String> {
    let stdin = std::io::stdin();
    let mut text = String::new();
    if stdin.is_terminal() {
        eprint!("{key} token: ");
        let hidden = Hidden::on();
        let read = stdin.read_line(&mut text);
        drop(hidden);
        eprintln!();
        read?;
    } else {
        stdin.lock().take(1 << 16).read_to_string(&mut text)?;
    }
    let token = text.trim().to_string();
    if token.is_empty() {
        return Err(Error::WrongUsage(format!(
            "no token on standard input: `printf '%s' \"$TOKEN\" | cornercase config secret {key}`"
        )));
    }
    Ok(token)
}

struct Hidden(Option<rustix::termios::Termios>);

impl Hidden {
    fn on() -> Self {
        use rustix::termios::{LocalModes, OptionalActions, tcgetattr, tcsetattr};
        let stdin = rustix::stdio::stdin();
        let Ok(before) = tcgetattr(stdin) else { return Self(None) };
        let mut quiet = before.clone();
        quiet.local_modes.remove(LocalModes::ECHO);
        let _ = tcsetattr(stdin, OptionalActions::Now, &quiet);
        Self(Some(before))
    }
}

impl Drop for Hidden {
    fn drop(&mut self) {
        if let Some(before) = &self.0 {
            let _ = rustix::termios::tcsetattr(rustix::stdio::stdin(), rustix::termios::OptionalActions::Now, before);
        }
    }
}

fn shown(value: &Value) -> String {
    match value {
        Value::String(text) if text.is_empty() => "-".into(),
        Value::String(text) => text.clone(),
        Value::Array(items) if items.iter().all(Value::is_string) => {
            items.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(",")
        }
        other => other.to_string(),
    }
}

fn connection(c: &Connection) -> String {
    let state = match c.state {
        State::Missing => "not set".to_string(),
        State::Refused => "refused".to_string(),
        State::Connected | State::Other => "connected".to_string(),
    };
    let mut line = state;
    if let Some(account) = &c.account {
        let _ = write!(line, " as {account}");
    }
    if let Some(count) = c.count {
        let _ = write!(line, " ({count})");
    }
    match (&c.from, &c.env) {
        (_, Some(env)) => {
            let _ = write!(line, " from {env}");
        }
        (Some(from), None) => {
            let _ = write!(line, " ({from})");
        }
        _ => {}
    }
    line
}

pub(super) fn render(described: &Described, changed: bool) -> String {
    let mut out = String::new();
    if changed {
        if described.notices.is_empty() {
            out.push_str("nothing changed\n");
        }
        for notice in &described.notices {
            let _ = writeln!(out, "{notice}");
        }
        return out;
    }
    if let Some(problem) = &described.problem {
        let _ = writeln!(out, "{} cannot be read: {problem}\n", described.path.display());
    }
    let key_width = described
        .settings
        .iter()
        .map(|s| s.key.len())
        .chain(described.connections.iter().map(|c| c.key.len()))
        .max()
        .unwrap_or(0);
    write_settings(&mut out, described, key_width);
    let _ = writeln!(out, "\nConnections");
    for c in &described.connections {
        let _ = writeln!(out, "  {:key_width$}  {}", c.key, connection(c));
    }
    write_projects(&mut out, described);
    out
}

fn write_settings(out: &mut String, described: &Described, key_width: usize) {
    let value_width = described.settings.iter().map(|s| shown(&s.value).chars().count()).max().unwrap_or(0).min(40);
    let mut page = "";
    for setting in &described.settings {
        if setting.page != page {
            page = &setting.page;
            let _ = writeln!(out, "{}{page}", if out.is_empty() { "" } else { "\n" });
        }
        let value = shown(&setting.value);
        let _ = writeln!(out, "  {:key_width$}  {value:value_width$}  {}", setting.key, setting.note);
    }
}

fn builds_of(text: &str) -> Result<Vec<String>> {
    if text.trim() == "default" {
        return Ok(crate::config::DEFAULT_BUILDS.iter().map(|name| (*name).to_string()).collect());
    }
    let names = crate::config::parse_builds(text).map_err(Error::WrongUsage)?;
    Ok(names.unwrap_or_default())
}

fn write_projects(out: &mut String, described: &Described) {
    if described.projects.is_empty() {
        return;
    }
    let _ = writeln!(out, "\nProjects");
    for p in &described.projects {
        let line = |text: &str| if text.is_empty() { "-".to_string() } else { text.replace('\n', "; ") };
        let builds = p.builds.join(" ");
        let (setup, teardown) = (line(&p.setup), line(&p.teardown));
        let _ =
            writeln!(out, "  {}  setup: {setup}  teardown: {teardown}  builds: {}", p.folder.display(), line(&builds));
    }
}

#[cfg(test)]
mod tests;
