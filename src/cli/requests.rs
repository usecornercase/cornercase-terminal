use std::fs::File;
use std::io::{self, Write};
use std::iter::once;
use std::os::fd::AsFd as _;

use clap::Parser as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::args::requests::RequestsArgs;
use super::args::{Cli, Command};
use super::line::{self, Printed};
use super::lines::{self, Lines};
use super::reach::{self, Kind};
use super::{quietly, usage};
use crate::client;
use crate::error::{Error, Result};

const ONLY_STANDARD_INPUT: &str =
    "--follow reads its commands from standard input, one per line: give - as the only argument";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestLine {
    args: Vec<String>,
    stdin: Option<String>,
}

#[derive(Debug, Default, Serialize)]
struct Answer {
    status: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    json: Option<Value>,
    #[serde(skip_serializing_if = "String::is_empty")]
    text: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    stderr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl Answer {
    fn refused(error: impl Into<String>) -> Self {
        Self { status: 2, error: Some(error.into()), ..Self::default() }
    }
}

enum Ran {
    Answer(Answer),
    Ended(Error),
}

pub(super) fn run(args: &RequestsArgs) -> Result<()> {
    if args.input != "-" {
        return Err(Error::WrongUsage(ONLY_STANDARD_INPUT.into()));
    }
    let link = client::connect()?;
    let (mut answers, input) = isolate()?;
    let watch = client::watched(answers.as_fd()).map(|fd| fd.try_clone_to_owned()).transpose()?;
    client::share(link, watch);
    let served = serve(&lines::read(input), &mut answers, ran);
    client::unshare();
    served
}

fn isolate() -> Result<(File, File)> {
    let answers = rustix::io::dup(rustix::stdio::stdout()).map_err(io::Error::from)?;
    let input = rustix::io::dup(rustix::stdio::stdin()).map_err(io::Error::from)?;
    rustix::stdio::dup2_stdin(File::open("/dev/null")?).map_err(io::Error::from)?;
    rustix::stdio::dup2_stdout(rustix::stdio::stderr()).map_err(io::Error::from)?;
    Ok((answers.into(), input.into()))
}

fn serve(lines: &Lines, out: &mut impl Write, mut run: impl FnMut(&[u8]) -> Option<Ran>) -> Result<()> {
    let mut failed = false;
    for read in lines {
        let Some(ran) = run(&read?) else { continue };
        match ran {
            Ran::Ended(Error::ReaderGone) => return Ok(()),
            Ran::Ended(error) => return Err(error),
            Ran::Answer(answer) => {
                failed |= answer.status != 0;
                if told(out, &answer)? {
                    return Ok(());
                }
            }
        }
    }
    if failed { Err(Error::Partial) } else { Ok(()) }
}

fn told(out: &mut impl Write, answer: &Answer) -> Result<bool> {
    let text = serde_json::to_string(answer).map_err(|e| Error::Control(e.to_string()))?;
    quietly(writeln!(out, "{text}").and_then(|()| out.flush()))
}

fn ran(bytes: &[u8]) -> Option<Ran> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return None;
    }
    let line: RequestLine = match serde_json::from_slice(bytes) {
        Ok(line) => line,
        Err(e) => {
            let text = String::from_utf8_lossy(bytes);
            return Some(Ran::Answer(Answer::refused(format!("not a request line: {e}: {}", text.trim_end()))));
        }
    };
    let name = name_of(&line.args);
    Some(match parsed(&line.args).and_then(|cli| checked(cli, &name)) {
        Ok(command) => run_line(command, line.stdin, &name),
        Err(answer) => Ran::Answer(answer),
    })
}

fn name_of(args: &[String]) -> String {
    args.iter().take_while(|word| !word.starts_with('-')).cloned().collect::<Vec<_>>().join(" ")
}

fn parsed(args: &[String]) -> std::result::Result<Cli, Answer> {
    Cli::try_parse_from(once("cornercase").chain(args.iter().map(String::as_str))).map_err(|e| {
        if e.use_stderr() {
            Answer::refused(usage(&e))
        } else {
            Answer { status: u8::try_from(e.exit_code()).unwrap_or(2), text: e.to_string(), ..Answer::default() }
        }
    })
}

fn checked(cli: Cli, name: &str) -> std::result::Result<Command, Answer> {
    if cli.remote.is_some() || cli.remote_command.is_some() {
        return Err(Answer::refused(
            "a line takes no --remote or --remote-command: the connection was chosen when `requests` started",
        ));
    }
    let Some(command) = cli.command else { return Err(Answer::refused("a line needs a command")) };
    if matches!(command, Command::StartServer(_)) {
        return Err(Answer::refused(
            "`start-server` cannot run on a line, since `requests` needs a running server: run `cornercase start-server` first",
        ));
    }
    match reach::kind(Some(&command)) {
        Kind::There => Ok(command),
        Kind::Run => Err(Answer::refused(alone(name))),
        Kind::Here(name) => Err(Answer::refused(format!("`{name}` acts on this machine, so it cannot run on a line"))),
        Kind::Apart(name) => Err(Answer::refused(format!("`{name}` cannot run on a line of `cornercase requests`"))),
    }
}

fn alone(name: &str) -> String {
    let line = match client::used_remote() {
        Some(remote) => remote.line(name),
        None => format!("cornercase {name}"),
    };
    format!("`{name}` runs as a process of its own: run `{line}` on its own")
}

fn run_line(command: Command, stdin: Option<String>, name: &str) -> Ran {
    let (result, printed) = line::run(stdin, || super::run_command(command));
    match client::line_done() {
        Some(error) => Ran::Ended(error),
        None => Ran::Answer(outcome(result, printed, name)),
    }
}

fn outcome(result: Result<bool>, printed: Printed, name: &str) -> Answer {
    let (status, error) = match result {
        Ok(true) => (0, None),
        Ok(false) => (1, None),
        Err(Error::Partial) => (3, None),
        Err(Error::Exited(code)) => (code, None),
        Err(Error::WrongUsage(message)) => (2, Some(message)),
        Err(Error::OwnConnection) => (2, Some(own_connection(name))),
        Err(e) => (1, Some(e.to_string())),
    };
    Answer { status, json: printed.json, text: printed.text, stderr: printed.stderr, error }
}

fn own_connection(name: &str) -> String {
    format!("`{name}` keeps a connection of its own (a stream): run `cornercase {name}` as a command of its own")
}

#[cfg(test)]
mod tests;
