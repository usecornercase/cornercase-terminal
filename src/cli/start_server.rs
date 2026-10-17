use super::args::StartServerArgs;
use super::{print_json, print_out};
use crate::client::{answer, ask_starting};
use crate::control::start_server::{SizeFrom, Started};
use crate::control::{self, StartServer};
use crate::error::{Error, Result};
use crate::version;

pub(super) fn run(args: &StartServerArgs) -> Result<()> {
    let theme = args.theme.as_deref().map(super::theme::read).transpose()?;
    let sent = theme.is_some();
    let request = StartServer { cols: args.cols, rows: args.rows, theme };
    request.size().map_err(Error::Control)?;
    let value = ask_starting("start-server", control::Command::StartServer(request))?;
    let started: Started = answer(value.clone())?;
    ignored_theme(sent, &started)?;
    for warning in warnings(&started) {
        eprintln!("warning: {warning}");
    }
    if args.print.json { print_json(&value) } else { print_out(&summary(&started)) }
}

fn ignored_theme(sent: bool, started: &Started) -> Result<()> {
    if sent && started.theme.is_none() {
        let version = if started.version.is_empty() { "an older version" } else { started.version.as_str() };
        return Err(Error::Control(format!(
            "the running cornercase server ({version}) takes no theme; update cornercase and restart the server"
        )));
    }
    Ok(())
}

fn summary(started: &Started) -> String {
    let size = format!("{}×{}", started.cols, started.rows);
    if started.already {
        let from = match started.size {
            SizeFrom::Window => "window size",
            SizeFrom::StartServer => "start-server size",
        };
        return format!("already running ({from} {size})\n");
    }
    let what = if started.restored { "restored the session" } else { "no session to restore" };
    format!("started: {what} ({size})\n")
}

fn warnings(started: &Started) -> Vec<String> {
    let mut warnings = Vec::new();
    if !started.complete {
        warnings.push("the session came back incomplete: some tabs could not be restored, see server.log".to_string());
    }
    if !started.version.is_empty() && started.version != version::CURRENT {
        warnings.push(format!(
            "the running server is cornercase {} and this is {}; run `cornercase restart`",
            started.version,
            version::CURRENT
        ));
    }
    warnings
}

#[cfg(test)]
mod tests;
