use std::fmt::Write as _;
use std::io::{self, IsTerminal};

use super::args::agents::{AgentsAction, AgentsArgs};
use super::args::changes::Where;
use super::{print_json, print_out, workspace};
use crate::client::{self, answer, ask};
use crate::control::closed_agents::{AgentsClosed, AgentsForgot, AgentsReopened, ClosedPane};
use crate::control::{AgentsClose, AgentsForget, AgentsReopen, Command};
use crate::error::{Error, Result};

pub(super) fn run(args: AgentsArgs) -> Result<()> {
    match args.action {
        AgentsAction::Close { place, check, force, print } => close(&place, (check, force), print.json),
        AgentsAction::Reopen { place, print } => reopen(&place, print.json),
        AgentsAction::Forget { place, yes, print } => forget(&place, yes, print.json),
    }
}

fn place(command: &'static str, at: &Where) -> Result<Option<u64>> {
    workspace(command, at.workspace, at.worktree.as_deref())
}

fn agents(count: usize) -> String {
    if count == 1 { "1 agent".to_string() } else { format!("{count} agents") }
}

fn close(at: &Where, (check, force): (bool, bool), json: bool) -> Result<()> {
    let workspace = place("agents close", at)?;
    let value = ask("agents-close", Command::AgentsClose(AgentsClose { workspace, check, force }))?;
    if json {
        return print_json(&value);
    }
    let closed: AgentsClosed = answer(value)?;
    print_out(&closed_text(&closed))
}

pub(super) fn closed_text(closed: &AgentsClosed) -> String {
    let id = closed.workspace;
    if closed.closed {
        let count = agents(closed.agents.len());
        return format!("closed {count} in workspace {id}; cornercase agents reopen --workspace {id} reopens them\n");
    }
    let mut text = format!("would close {} in workspace {id}:\n", agents(closed.agents.len()));
    for pane in &closed.agents {
        let _ = writeln!(text, "  {}", pane_line(pane, pane.status.as_deref().unwrap_or("idle")));
    }
    for stop in &closed.stops {
        let _ = writeln!(text, "  also stops pane {} in tab {}", stop.pane, stop.tab);
    }
    text
}

fn pane_line(pane: &ClosedPane, status: &str) -> String {
    let resumes = if pane.resumes { "resumes" } else { "starts over" };
    let role = pane.role.as_deref().unwrap_or("-");
    format!("pane {}  {}  {role}  {status}  {resumes}", pane.pane, pane.agent.as_deref().unwrap_or("-"))
}

fn reopen(at: &Where, json: bool) -> Result<()> {
    let workspace = place("agents reopen", at)?;
    let value = ask("agents-reopen", Command::AgentsReopen(AgentsReopen { workspace }))?;
    if json {
        return print_json(&value);
    }
    let reopened: AgentsReopened = answer(value)?;
    print_out(&reopened_text(&reopened))
}

pub(super) fn reopened_text(reopened: &AgentsReopened) -> String {
    let mut text = String::new();
    for pane in &reopened.panes {
        let resumes = if pane.resumes { "resumes" } else { "starts over" };
        let role = pane.role.as_deref().unwrap_or("-");
        let _ = writeln!(text, "pane {}  {}  {role}  {resumes}", pane.pane, pane.agent.as_deref().unwrap_or("-"));
    }
    text
}

fn forget(at: &Where, yes: bool, json: bool) -> Result<()> {
    let workspace = place("agents forget", at)?;
    if !yes && !confirmed()? {
        return print_out("nothing forgotten\n");
    }
    let value = ask("agents-forget", Command::AgentsForget(AgentsForget { workspace }))?;
    if json {
        return print_json(&value);
    }
    let forgot: AgentsForgot = answer(value)?;
    print_out(&format!("forgot {} closed\n", agents(forgot.forgot)))
}

fn confirmed() -> Result<bool> {
    if !io::stdin().is_terminal() {
        return Err(Error::Control("no terminal to ask in; pass --yes to forget them".into()));
    }
    Ok(client::confirm("Forget the closed agents? They can no longer be reopened [y/N] "))
}

#[cfg(test)]
mod tests;
