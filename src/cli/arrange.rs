use super::args::arrange::{FoldArgs, GroupAction, MoveArgs, MovePaneArgs};
use super::{id, print_json, print_out, say, workspace};
use crate::client::{answer, ask};
use crate::control::arrange::{
    Fold, GroupDelete, GroupNew, GroupStyle, Locked, Move, MovePane, Removal, RemovalCheck, Styled,
};
use crate::control::{Command, Item};
use crate::error::{Error, Result};

struct Row<'a> {
    group: Option<u64>,
    project: Option<u64>,
    workspace: Option<u64>,
    branch: Option<&'a str>,
    tab: Option<u64>,
}

impl Row<'_> {
    fn item(&self, command: &'static str) -> Result<Item> {
        let workspace = workspace(command, self.workspace, self.branch)?;
        self.group
            .map(Item::Group)
            .or_else(|| self.project.map(Item::Project))
            .or_else(|| workspace.map(Item::Workspace))
            .or_else(|| self.tab.map(Item::Tab))
            .ok_or_else(|| Error::WrongUsage(format!("say what `{command}` acts on")))
    }
}

pub(super) fn move_row(args: &MoveArgs) -> Result<()> {
    let row = Row {
        group: args.group,
        project: args.project,
        workspace: args.workspace,
        branch: args.worktree.as_deref(),
        tab: args.tab,
    };
    let request = Move { item: row.item("move")?, before: args.before, into: args.into, ungroup: args.ungroup };
    ask("move", Command::Move(request)).map(drop)
}

pub(super) fn fold(args: &FoldArgs, command: &'static str) -> Result<()> {
    let row = Row {
        group: args.group,
        project: args.project,
        workspace: args.workspace,
        branch: args.worktree.as_deref(),
        tab: None,
    };
    let request = Fold { item: row.item(command)? };
    let command_request = if command == "collapse" { Command::Collapse(request) } else { Command::Expand(request) };
    ask(command, command_request).map(drop)
}

pub(super) fn move_pane(args: &MovePaneArgs) -> Result<()> {
    let request = MovePane { pane: Some(args.pane), target: Some(args.target), place: Some(args.place.into()) };
    let value = ask("move-pane", Command::MovePane(request))?;
    if args.print.json {
        return print_json(&value);
    }
    Ok(())
}

pub(super) fn group(action: GroupAction) -> Result<()> {
    match action {
        GroupAction::New { name, print } => {
            let value = ask("group-new", Command::GroupNew(GroupNew { name: name.join(" ") }))?;
            say(value, print.json, |done| id(done.group))
        }
        GroupAction::Style { group, icon, colour, print } => {
            let request = GroupStyle { group: Some(group), icon, colour };
            let value = ask("group-style", Command::GroupStyle(request))?;
            if print.json {
                return print_json(&value);
            }
            let styled: Styled = answer(value)?;
            print_out(&format!("{} {}\n", styled.icon, styled.colour))
        }
        GroupAction::Delete { group, close_projects } => {
            let request = GroupDelete { group: Some(group), close_projects };
            ask("group-delete", Command::GroupDelete(request)).map(drop)
        }
    }
}

pub(super) fn check_removal(item: Item, json: bool) -> Result<()> {
    let Item::Workspace(workspace) = item else { return Ok(()) };
    let value = ask("removal-check", Command::RemovalCheck(RemovalCheck { workspace: Some(workspace) }))?;
    if json {
        return print_json(&value);
    }
    let removal: Removal = answer(value)?;
    print_out(&(removal_lines(&removal).join("\n") + "\n"))
}

fn lock_line(lock: &Locked) -> String {
    let mut parts = vec![format!("locked: {}", lock.reason)];
    if let Some(role) = &lock.role {
        parts.push(format!("the worktree of {role} in {}", lock.path.display()));
    }
    match (lock.pid, lock.running) {
        (Some(pid), Some(true)) => parts.push(format!("process {pid} still runs and may be using it")),
        (Some(pid), _) => parts.push(format!("process {pid} is gone")),
        (None, _) => {}
    }
    parts.join("; ") + " (--unlock)"
}

fn removal_lines(removal: &Removal) -> Vec<String> {
    let branch = removal.branch.as_deref().unwrap_or("-");
    let mut lines = vec![format!("workspace {}  {branch}  {}", removal.workspace, removal.path.display())];
    lines.push(if removal.changed {
        "changes: not committed, removing it deletes them (--force)".to_string()
    } else {
        "changes: none".to_string()
    });
    lines.push(removal.lock.as_ref().map_or_else(|| "locked: no".to_string(), lock_line));
    if !removal.roles.is_empty() {
        let roles: Vec<String> = removal
            .roles
            .iter()
            .map(|tree| format!("{} in {} (branch {})", tree.role, tree.path.display(), tree.branch))
            .collect();
        lines.push(format!("role worktrees: {}, removed with it", roles.join("; ")));
    }
    if removal.teardown {
        lines.push("teardown: the project's teardown runs first".to_string());
    }
    lines
}

#[cfg(test)]
mod tests;
