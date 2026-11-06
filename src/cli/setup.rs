use super::args::setup::SetupArgs;
use super::{id, say, workspace};
use crate::client::ask;
use crate::control::{Command, RunSetup};
use crate::error::Result;

pub(super) fn run(args: &SetupArgs) -> Result<()> {
    let workspace = workspace("setup", args.workspace, args.worktree.as_deref())?;
    let request = RunSetup { workspace, focus: args.create.focus };
    let value = ask("setup", Command::RunSetup(request))?;
    say(value, args.create.print.json, |done| id(done.ids.pane))
}

#[cfg(test)]
mod tests;
