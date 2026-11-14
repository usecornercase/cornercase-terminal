use std::io::Write as _;

use clap::{ArgMatches, CommandFactory as _, FromArgMatches as _};
use serde::Serialize;

use super::args::Cli;
use crate::client;
use crate::error::Error;
use crate::remote::Remote;
use crate::version;

#[derive(Debug, Serialize)]
struct Refusal {
    error: Older,
}

#[derive(Debug, Serialize)]
struct Older {
    code: &'static str,
    message: String,
    destination: String,
    there: Option<String>,
    here: &'static str,
    update: Vec<String>,
}

pub fn parse() -> (Cli, bool) {
    let matches = Cli::command().get_matches();
    let json = wants_json(&matches);
    let mut cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    if std::env::var_os(crate::bots::BOT_ENV).is_some_and(|id| !id.is_empty()) {
        cli.refused = super::guard::refusal(&Cli::command(), &matches);
    }
    (cli, json)
}

fn wants_json(matches: &ArgMatches) -> bool {
    matches.try_get_one::<bool>("json").ok().flatten().is_some_and(|on| *on)
        || matches.subcommand().is_some_and(|(_, sub)| wants_json(sub))
}

pub fn older(error: &Error) -> bool {
    matches!(error, Error::RemoteBehind(_) | Error::RemoteTooOld(_) | Error::RemoteOlder { .. })
}

pub fn print_older(error: &Error, json: bool) {
    let line = client::used_remote().filter(|_| json).and_then(|remote| refusal(error, remote));
    if let Some(line) = line.and_then(|refusal| serde_json::to_string(&refusal).ok()) {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    }
    eprintln!("error: {error}");
}

fn refusal(error: &Error, remote: &Remote) -> Option<Refusal> {
    let there = match error {
        Error::RemoteBehind(behind) => Some(behind.there.clone()),
        Error::RemoteTooOld(_) => None,
        _ => return None,
    };
    Some(Refusal {
        error: Older {
            code: "older",
            message: error.to_string(),
            destination: remote.destination.clone(),
            there,
            here: version::CURRENT,
            update: remote.update_words(),
        },
    })
}

#[cfg(test)]
mod tests;
