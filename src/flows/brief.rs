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
        told.push_str(&answers(role, dir));
    }
    if prompt.is_empty() { format!("{told} {WAIT_FIRST}") } else { format!("{prompt}\n\n{told} {WAIT_FIRST}") }
}

fn answers(role: &Role, dir: &Path) -> String {
    let stem = crate::issues::slug(&role.name, 40);
    let stem = if stem.is_empty() { "answer" } else { stem.as_str() };
    format!(
        " The run's folder is {dir} (also `${env}`). The task in full, exactly as it was given, is `{file}` there \
         ({task}): read it before you start on the work you are sent, since the orchestrator's messages say only \
         what each step adds. Write each answer to a file of its own in that folder, named after your role and the \
         next number free there (`{stem}-1.md`, then `{stem}-2.md`), and end your turn with that file's path and a \
         summary of a few lines. Anything else too long for a message (a design, logs, a script that shows a bug) \
         goes there too, never in the worktree; if your file tools cannot reach the folder, use your shell.",
        dir = dir.display(),
        env = super::folder::ENV,
        file = super::folder::TASK,
        task = dir.join(super::folder::TASK).display()
    )
}

fn plan(dir: Option<&Path>) -> String {
    let give = if dir.is_some() {
        format!(
            "the path of `{}` (see Files) and what this stage adds, such as the paths of what the stages before \
             produced, never a retelling of the task,",
            super::folder::TASK
        )
    } else {
        "what it needs (the task, what the stages before produced)".to_string()
    };
    format!(
        "\n## Plan\n\nRun the stages in order. At each stage, send each of its roles {give} and wait for all their \
         answers. When the work goes back to an earlier stage, that stage and every stage after it run again, in \
         order, on what changed, however small the change (a fix of docs only, a rebase or a merge of a moved branch \
         included), before the work goes on, so no change skips a stage the first version went through.\n\n"
    )
}

const END_WHEN: &str = "once every stage after the last change has run on it, and each of them that has a loop \
    answered clean or reached its limit (then list what is left)";

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
    out.push_str(&plan(dir));
    for (i, stage) in flow.stages.iter().enumerate() {
        let _ = writeln!(out, "{}. {}", i + 1, stage_line(flow, stage));
    }
    let _ = writeln!(out, "\nAt the end, {END_WHEN}: {}.", end_line(flow.end, branch));
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
    let compact = crate::agents::Known::of(&role.agent).and_then(crate::agents::Known::compact).map_or_else(
        String::new,
        |compact| {
            let keeps = if compact.keeps { "and what it must keep" } else { "alone" };
            format!(", compacts with `{}` {keeps}", compact.command)
        },
    );
    format!("- {} ({at}): {}, {edits}{place}{compact}\n", role.name, role.summary())
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

pub(crate) fn owed(flow: &Flow, stages: &[usize], changed: Option<usize>) -> Option<String> {
    if stages.is_empty() {
        return None;
    }
    let named: Vec<String> = stages
        .iter()
        .map(|stage| {
            let roles = flow.stages.get(*stage).map(|s| s.roles.join(", ")).unwrap_or_default();
            format!("stage {} ({roles})", stage + 1)
        })
        .collect();
    let (verb, pronoun) = if stages.len() == 1 { ("has", "it") } else { ("have", "them") };
    let since = changed
        .map_or_else(|| "since the last change".to_string(), |at| format!("since the last change in stage {}", at + 1));
    Some(format!("{} {verb} not run {since}: run {pronoun} first, or say in your summary why not", names(&named)))
}

const OWN_FAILURE: &str = "Your last turn ended on an error (such as an API error) before you were done, so what \
    you were doing then was not done: go on with the plan from where you stopped.";

pub(crate) fn failed(roles: &[(String, u64)]) -> String {
    let named: Vec<String> = roles.iter().map(|(role, pane)| format!("{role} (pane {pane})")).collect();
    let panes: Vec<String> = roles.iter().map(|(_, pane)| format!("--pane {pane}")).collect();
    let (their, them, wrote) =
        if roles.len() == 1 { ("its", "it", "it last wrote") } else { ("their", "them", "they last wrote") };
    format!(
        "{} ended {their} turn on an error (such as an API error) before finishing the work you sent {them}, so that \
         work was not done. Read what {wrote} with `cornercase read --last-message {}`, then send {them} the work \
         again; if it fails again, tell the user with `cornercase notify`.",
        names(&named),
        panes.join(" ")
    )
}

pub(crate) fn back(roles: &[(String, u64)]) -> String {
    let named: Vec<String> = roles.iter().map(|(role, pane)| format!("{role} (pane {pane})")).collect();
    let panes: Vec<String> = roles.iter().map(|(_, pane)| format!("--pane {pane}")).collect();
    let (them, panes_of, wrote, had) = if roles.len() == 1 {
        ("it", "its pane", "it last wrote", "it had")
    } else {
        ("them", "their panes", "they last wrote", "they had")
    };
    format!(
        "{} exited, and cornercase started {them} again in {panes_of}. Read what {wrote} with `cornercase read \
         --last-message {}`, then send {them} again any work {had} not finished.",
        names(&named),
        panes.join(" ")
    )
}

pub(crate) fn woken(
    own: bool,
    finished: &[(String, u64)],
    failed_roles: &[(String, u64)],
    came_back: &[(String, u64)],
) -> String {
    let mut parts = Vec::new();
    if !finished.is_empty() {
        parts.push(self::finished(finished));
    }
    if !failed_roles.is_empty() {
        parts.push(failed(failed_roles));
    }
    if !came_back.is_empty() {
        parts.push(back(came_back));
    }
    if own {
        parts.push(OWN_FAILURE.to_string());
    }
    parts.join(" ")
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

pub(crate) struct Exited<'a> {
    pub(crate) back: &'a [String],
    pub(crate) fresh: &'a [String],
}

fn exited_notes(exited: &Exited) -> String {
    let mut notes = who(exited.back, "exited too and was", "exited too and were", "started again.");
    notes.push_str(&reopened_notes(&Closed { cut: &[], fresh: exited.fresh }));
    notes
}

pub(crate) fn exited(members: &[Member], dir: Option<&Path>, notes: &Exited) -> String {
    format!(
        "Your agent exited, and cornercase started it again in your pane, in your own conversation. Your team: {}. \
         Use these from now on.{}{} Any wait you had running ended: check what each one last answered with \
         `cornercase read --last-message`, then go on with the plan. If the plan was already done, answer `ok` and \
         do nothing else.",
        ids(members),
        folder_note(dir),
        exited_notes(notes)
    )
}

pub(crate) fn exited_stands(members: &[Member], dir: Option<&Path>, notes: &Exited) -> String {
    format!(
        "\n## Where the run stands\n\nThe orchestrator before you exited, so you take the run over where it stands. \
         Your team has these pane ids: {}.{}{} Check what each one last answered with `cornercase read \
         --last-message` before you go on.\n",
        ids(members),
        folder_note(dir),
        exited_notes(notes)
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
    let (when, n) = (back.when.trim(), back.to);
    let _ = write!(
        line,
        " If {who} {when}, send them to {to} and go back to stage {n}, then run every stage after it again, in \
         order. Do this at most {times}; after that, go on even if {who} {when} again, and list what is left in \
         your final summary."
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
         outside the worktree, so nothing in it is ever committed. The task is in it as `{file}` ({task}), exactly as \
         it was given, and every role's prompt names that file. Each agent writes every answer to a file of its \
         own there and replies with the file's path and a few lines. Pass work on by path rather than pasting \
         text, and read a file only when the summary is not enough. Write your own decisions there too, one file \
         each (`orchestrator-<n>.md`: what you asked, what came back, what you decided and why).\n",
        dir = dir.display(),
        env = super::folder::ENV,
        file = super::folder::TASK,
        task = dir.join(super::folder::TASK).display()
    )
}

const CONTEXT_RULE: &str = "- Check your team's context before you send a role a large piece of work: \
    `cornercase status --panes | cut -f 1,7,12,13,26,32` prints each pane's id, status, context (the share of its \
    agent's context window used, in percent), caller (true for you), role and compacted. Above about 70 %, first \
    send the role its compact command from the team list (with what it must keep when it takes that: the branches, \
    the checks to run, what is left), as `cornercase send --pane N --enter '/compact …' --timeout 540` in the \
    foreground with your shell tool's longest time limit, since it can take minutes, then wait for its turn to end \
    as after any send and send the work. Right after a compaction its context is empty and compacted is true until \
    its next reply: that is not \"not available\". You cannot compact yourself: when your own context passes about \
    80 %, tell the user with `cornercase notify`.\n";

const FAILED_RULE: &str = "- An agent whose turn ended `failed` (its last message is an `API Error` line, for \
    instance) did not do the work: read its last message first, since the error can come after it did part of \
    it, send it the work again once, and if it fails again, tell the user with `cornercase notify`. If \
    `cornercase send` or `cornercase mark` warns that a stage has not run since the last change, run that stage \
    first, or say in your final summary why not.\n";

fn pane_flags(flow: &Flow, members: &[Member]) -> String {
    let flags: Vec<String> = flow
        .roles
        .iter()
        .filter_map(|role| members.iter().find(|m| m.role == role.name))
        .map(|m| format!("--pane {}", m.pane))
        .collect();
    flags.join(" ")
}

fn rules(flow: &Flow, members: &[Member], own: Option<u64>, dir: Option<&Path>) -> String {
    let others = pane_flags(flow, members);
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
         {context}\
         - Never edit {edit}, commit or run the project's checks yourself: the roles do that.{only}\n\
         - Never answer another agent's permission prompt or question (status `waiting` in \
         `cornercase status --panes`); tell the user with `cornercase notify` instead.\n\
         - Stay in this worktree: do not open, close or focus tabs, workspaces or worktrees. Each role's tab is \
         named after it.\n\
         - When the plan is done, do the final action, then run `{mark}` so the user sees the run waits for review, \
         and end your turn with a short summary.\n\
         - These rules hold after the plan too: when the user asks for more work, send it to the agent that does \
         it, wait in the foreground until it answers, then report.\n",
        context = [CONTEXT_RULE, FAILED_RULE].concat(),
        panes = others,
        edit = if dir.is_some() { "the worktree's files" } else { "files" },
        only = if dir.is_some() { " The run's folder is the only place you write." } else { "" }
    )
}

#[cfg(test)]
mod tests;
