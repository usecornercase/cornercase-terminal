use std::fmt::Write as _;
use std::path::Path;

use super::run::{Member, RoleWorktree};
use super::{End, Flow, ORCHESTRATOR, Role, Stage};

pub(super) const WAIT_FIRST: &str = "Wait for the orchestrator's first message before doing anything.";

pub(crate) fn member(role: &Role, branch: Option<&str>, dir: Option<&Path>) -> String {
    let prompt = role.prompt.trim();
    let prompt = prompt.strip_suffix(WAIT_FIRST).map_or(prompt, str::trim_end);
    let mut told = match (role.edits, branch) {
        (true, None) => {
            format!("You are `{}` in this team, and you may edit files and commit in this worktree.", role.name)
        }
        (true, Some(branch)) => format!(
            "You are `{}` in this team, and you may edit files and commit in this worktree, which is yours alone, on \
             branch {branch}: commit your work there, since the others read it from that branch.",
            role.name
        ),
        (false, None) => {
            format!("You are `{}` in this team: do not edit the worktree's files and do not commit.", role.name)
        }
        (false, Some(branch)) => format!(
            "You are `{}` in this team: do not edit the worktree's files and do not commit. This worktree is yours \
             alone, on branch {branch}.",
            role.name
        ),
    };
    if let Some(dir) = dir {
        let stem = crate::issues::slug(&role.name, 40);
        let stem = if stem.is_empty() { "answer" } else { stem.as_str() };
        let _ = write!(
            told,
            " Write each answer to a file of its own in the run's folder, {} (also `${}`), named after your role \
             and the next number free there (`{stem}-1.md`, then `{stem}-2.md`), and end your turn with that \
             file's path and a summary of a few lines. Anything else too long for a message (a design, logs, a \
             script that shows a bug) goes there too, never in the worktree; if your file tools cannot reach the \
             folder, use your shell.",
            dir.display(),
            super::folder::ENV
        );
    }
    if prompt.is_empty() { format!("{told} {WAIT_FIRST}") } else { format!("{prompt}\n\n{told} {WAIT_FIRST}") }
}

#[derive(Clone, Copy)]
pub(crate) struct Team<'a> {
    pub(crate) members: &'a [Member],
    pub(crate) trees: &'a [RoleWorktree],
}

pub(crate) fn brief(flow: &Flow, task: &str, branch: &str, team: Team, dir: Option<&Path>) -> String {
    let Team { members, trees } = team;
    let pane = |role: &str| members.iter().find(|m| m.role == role).map(|m| m.pane);
    let mut out = format!(
        "You are the orchestrator of a team of coding agents that cornercase started for the flow \"{}\", all in \
         this git worktree, on branch {branch}. You run the plan below with the `cornercase` command; the other \
         agents do the work.\n\n## Task\n\n{}\n\n## Team\n\n",
        flow.name,
        task.trim()
    );
    for role in &flow.roles {
        out.push_str(&team_line(role, pane(&role.name), trees));
    }
    let own = pane(ORCHESTRATOR);
    let you = own.map_or_else(String::new, |id| format!(" (pane {id})"));
    let _ = writeln!(out, "- you, the orchestrator{you}: {}", flow.orchestrator.summary());
    out.push_str(
        "\n## Plan\n\nRun the stages in order. At each stage, send each of its roles what it needs (the task, what \
         the stages before produced) and wait for all their answers.\n\n",
    );
    for (i, stage) in flow.stages.iter().enumerate() {
        let _ = writeln!(out, "{}. {}", i + 1, stage_line(flow, stage));
    }
    let _ = writeln!(out, "\nAt the end: {}.", end_line(flow.end, branch));
    if !trees.is_empty() {
        out.push_str(&worktrees(branch));
    }
    let notes: Vec<&str> =
        [flow.notes.trim(), flow.orchestrator.prompt.trim()].into_iter().filter(|n| !n.is_empty()).collect();
    if !notes.is_empty() {
        let _ = write!(out, "\n## Notes\n\n{}\n", notes.join("\n\n"));
    }
    if let Some(dir) = dir {
        out.push_str(&files(dir));
    }
    out.push_str(&rules(flow, members, own, dir));
    out
}

fn team_line(role: &Role, pane: Option<u64>, trees: &[RoleWorktree]) -> String {
    let at = pane.map_or_else(|| "no pane".to_string(), |id| format!("pane {id}"));
    let edits = if role.edits { "edits files" } else { "must not edit files" };
    let place = trees.iter().find(|tree| tree.role == role.name).map_or_else(String::new, |tree| {
        format!(", in its own worktree {} on branch {}", tree.path.display(), tree.branch)
    });
    format!("- {} ({at}): {}, {edits}{place}\n", role.name, role.summary())
}

fn ids(members: &[Member]) -> String {
    let team: Vec<String> =
        members.iter().filter(|m| m.role != ORCHESTRATOR).map(|m| format!("{} is pane {}", m.role, m.pane)).collect();
    let own = members.iter().find(|m| m.role == ORCHESTRATOR).map(|m| m.pane);
    let you = own.map_or_else(String::new, |id| format!(", and you are pane {id}"));
    format!("{}{you}", team.join(", "))
}

fn folder_note(dir: Option<&Path>) -> String {
    dir.map_or_else(String::new, |dir| format!(" The run's folder is still {}.", dir.display()))
}

pub(crate) fn restored(members: &[Member], dir: Option<&Path>) -> String {
    format!(
        "cornercase restarted, so your team has new pane ids: {}. Use these from now on.{} Any wait you had \
         running ended, and an agent may have lost the turn it was in: check what each one last answered with \
         `cornercase read --last-message`, then go on with the plan. If the plan was already done, answer `ok` and \
         do nothing else.",
        ids(members),
        folder_note(dir)
    )
}

pub(crate) fn finished(roles: &[(String, u64)]) -> String {
    let named: Vec<String> = roles.iter().map(|(role, pane)| format!("{role} (pane {pane})")).collect();
    let panes: Vec<String> = roles.iter().map(|(_, pane)| format!("--pane {pane}")).collect();
    let (them, answers) = if roles.len() == 1 { ("it", "its answer") } else { ("them", "their answers") };
    format!(
        "{} finished the work you sent {them}. Read {answers} with `cornercase read --last-message {}`, then go on.",
        names(&named),
        panes.join(" ")
    )
}

pub(crate) struct Closed<'a> {
    pub(crate) cut: &'a [String],
    pub(crate) fresh: &'a [String],
}

fn who(roles: &[String], singular: &str, plural: &str, rest: &str) -> String {
    match roles {
        [] => String::new(),
        [one] => format!(" {one} {singular} {rest}"),
        many => format!(" {} {plural} {rest}", names(many)),
    }
}

fn reopened_notes(closed: &Closed) -> String {
    let cut = |roles: &[String]| if roles.len() == 1 { "its" } else { "their" };
    let mut notes = who(closed.cut, "was", "were", &format!("cut in the middle of {} turn.", cut(closed.cut)));
    notes.push_str(&who(
        closed.fresh,
        "starts",
        "start",
        &format!("over in a new conversation with only {} role.", cut(closed.fresh)),
    ));
    notes
}

pub(crate) fn reopened(members: &[Member], dir: Option<&Path>, closed: &Closed) -> String {
    format!(
        "Your team was closed and reopened, so it has new pane ids: {}. Use these from now on.{}{} Any wait you had \
         running ended: check what each one last answered with `cornercase read --last-message`, then go on with \
         the plan. If the plan was already done, answer `ok` and do nothing else.",
        ids(members),
        folder_note(dir),
        reopened_notes(closed)
    )
}

pub(crate) fn where_it_stands(members: &[Member], dir: Option<&Path>, closed: &Closed) -> String {
    format!(
        "\n## Where the run stands\n\nYour team was closed and reopened before you started, so it has these pane \
         ids: {}.{}{} Check what each one last answered with `cornercase read --last-message` before you go on.\n",
        ids(members),
        folder_note(dir),
        reopened_notes(closed)
    )
}

fn worktrees(branch: &str) -> String {
    format!(
        "\n## Worktrees\n\nA role marked as being in its own worktree works apart from this one; the others share \
         this worktree, on branch {branch}.\n\n\
         - Reading: what such a role has not committed is invisible from here, and its candidate is its branch B. \
         Read it with `git log {branch}..B`, `git diff {branch}...B` and `git show B:path`; to build or test it, run \
         the commands in its worktree folder.\n\
         - Integrating: ask a role that edits this worktree to `git merge B` (a fast-forward when {branch} has not \
         moved), or to take parts of it by hand or with `git cherry-pick`. Never check out a role's branch here, and \
         never merge it yourself.\n\
         - Those worktrees stay until this workspace is removed.\n"
    )
}

fn names(roles: &[String]) -> String {
    match roles {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn stage_line(flow: &Flow, stage: &Stage) -> String {
    let mut line = names(&stage.roles);
    if stage.roles.len() > 1 {
        line.push_str(", in parallel");
    }
    line.push('.');
    let Some(back) = &stage.back else { return line };
    let who = match stage.roles.len() {
        1 => "it reports",
        2 => "either reports",
        _ => "any of them reports",
    };
    let to = flow.stages.get(back.to.wrapping_sub(1)).map(|s| names(&s.roles)).unwrap_or_default();
    let times = if back.at_most == 1 { "once".to_string() } else { format!("{} times", back.at_most) };
    let _ = write!(
        line,
        " If {who} {}, send them to {to} and go back to stage {}; at most {times}, then go on and list what is left \
         in your final summary.",
        back.when.trim(),
        back.to
    );
    line
}

fn end_line(end: End, branch: &str) -> String {
    match end {
        End::PullRequest => format!(
            "push branch {branch} and open a pull request with `gh pr create` (a merge request with `glab mr create` \
             when the repository is on GitLab), saying what changed and what the reviews and tests found"
        ),
        End::Commits => format!("leave the commits on branch {branch}; do not push and do not open a pull request"),
    }
}

fn files(dir: &Path) -> String {
    format!(
        "\n## Files\n\nThe run has a folder for its files, {dir} (also `${env}` in every pane of the team). It is \
         outside the worktree, so nothing in it is ever committed. Each agent writes every answer to a file of its \
         own there and replies with the file's path and a few lines. Pass work on by path rather than pasting \
         text, and read a file only when the summary is not enough. Write your own decisions there too, one file \
         each (`orchestrator-<n>.md`: what you asked, what came back, what you decided and why).\n",
        dir = dir.display(),
        env = super::folder::ENV
    )
}

fn rules(flow: &Flow, members: &[Member], own: Option<u64>, dir: Option<&Path>) -> String {
    let others: Vec<String> = flow
        .roles
        .iter()
        .filter_map(|role| members.iter().find(|m| m.role == role.name))
        .map(|m| format!("--pane {}", m.pane))
        .collect();
    let mark = own.map_or_else(|| "cornercase mark".to_string(), |id| format!("cornercase mark --pane {id}"));
    format!(
        "\n## How to drive the agents\n\n\
         - Every agent already has its role prompt and waits for you. First wait until they are all ready: \
         `cornercase wait --all {panes} --until turn-over --timeout 540`.\n\
         - Send work with `cornercase send --pane N --enter 'the full instruction'`: one self-contained message \
         saying what to do and what to answer with. An agent knows only what you send it.\n\
         - After each send, wait in the foreground until the agent answers: `cornercase wait --pane N --until \
         turn-over --timeout 540`. Run one wait at a time, with a `--pane` for each agent you wait for (and `--any` \
         or `--all`), never several identical waits. Give your shell tool its longest time limit (600000 ms on \
         Claude Code's Bash, which otherwise stops a command after 2 minutes), else use a `--timeout` under its \
         limit; when the wait times out, run it again. Never wait in the background: that ends your turn, and \
         cornercase then shows you as finished while your team works.\n\
         - If the user writes to you while you wait, answer, then wait again. A wait moved to the background that \
         ends is not news: check the agent with `cornercase status --panes`, and wait again while it works.\n\
         - Read what agents answered with `cornercase read --last-message --pane N` (a `--pane` each for several), \
         not their screens. If it fails for an agent (its record cannot be found), read its screen with \
         `cornercase read --pane N --lines 300` and take the answer from there.\n\
         - If `send` fails with `not confirmed`, run `cornercase read --pane N` before sending again: the agent may \
         have it.\n\
         - Never edit {edit}, commit or run the project's checks yourself: the roles do that.{only}\n\
         - Never answer another agent's permission prompt or question (status `waiting` in \
         `cornercase status --panes`); tell the user with `cornercase notify` instead.\n\
         - Stay in this worktree: do not open, close or focus tabs, workspaces or worktrees. Each role's tab is \
         named after it.\n\
         - When the plan is done, do the final action, then run `{mark}` so the user sees the run waits for review, \
         and end your turn with a short summary.\n\
         - These rules hold after the plan too: when the user asks for more work, send it to the agent that does \
         it, wait in the foreground until it answers, then report.\n",
        panes = others.join(" "),
        edit = if dir.is_some() { "the worktree's files" } else { "files" },
        only = if dir.is_some() { " The run's folder is the only place you write." } else { "" }
    )
}

#[cfg(test)]
mod tests;
