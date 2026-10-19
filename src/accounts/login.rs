use std::ffi::OsString;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::agents::{self, Known};
use crate::process;

pub(crate) const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum State {
    SignedIn,
    SignedOut,
    #[default]
    Unknown,
    Failed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Login {
    pub(crate) state: State,
    pub(crate) email: Option<String>,
    pub(crate) plan: Option<String>,
}

impl State {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::SignedIn => "signed-in",
            Self::SignedOut => "signed-out",
            Self::Unknown => "unknown",
            Self::Failed => "failed",
        }
    }
}

impl Login {
    pub(crate) fn of(state: State) -> Self {
        Self { state, ..Self::default() }
    }

    pub(crate) fn text(&self) -> String {
        match (self.state, &self.email, &self.plan) {
            (State::SignedIn, Some(email), Some(plan)) => format!("signed in as {email} ({plan})"),
            (State::SignedIn, Some(email), None) => format!("signed in as {email}"),
            (State::SignedIn, None, Some(plan)) => format!("signed in ({plan})"),
            (State::SignedIn, None, None) => "signed in".into(),
            (State::SignedOut, ..) => "signed out".into(),
            (State::Unknown, ..) => "login unknown".into(),
            (State::Failed, ..) => "login check failed".into(),
        }
    }
}

pub(crate) fn check(kind: Known, program: &str, home: &Path) -> Login {
    run(kind, command(kind, program, home))
}

pub(crate) fn check_system(kind: Known, program: &str, home: Option<&Path>) -> Login {
    run(kind, system(kind, program, home))
}

fn run(kind: Known, command: Option<Command>) -> Login {
    let Some(mut command) = command else { return Login::default() };
    match process::output_within(&mut command, TIMEOUT) {
        Ok(Some(output)) => {
            let text = [output.stdout, output.stderr].concat();
            kind.login(&String::from_utf8_lossy(&text))
        }
        Ok(None) | Err(_) => Login::of(State::Failed),
    }
}

pub(crate) fn command(kind: Known, program: &str, home: &Path) -> Option<Command> {
    let mut command = system(kind, program, Some(home))?;
    command.env(kind.account()?.env, home);
    Some(command)
}

pub(crate) fn system(kind: Known, program: &str, home: Option<&Path>) -> Option<Command> {
    let account = kind.account()?;
    let mut command = Command::new(program);
    command.args(account.status?);
    if let Some(home) = home.filter(|home| home.is_dir()) {
        command.current_dir(home);
    }
    let session = agents::claude::CLAUDE_SESSION_ENV.iter().chain(&agents::copilot::SESSION_ENV);
    for name in account.remove.iter().chain(session) {
        command.env_remove(name);
    }
    Some(command)
}

pub(crate) fn tokens(var: impl Fn(&str) -> Option<OsString>) -> Vec<String> {
    super::supported()
        .filter_map(Known::account)
        .flat_map(|account| account.tokens.iter().copied())
        .filter(|name| var(name).is_some_and(|value| !value.is_empty()))
        .map(str::to_string)
        .collect()
}
