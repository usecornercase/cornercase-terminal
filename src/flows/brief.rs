use std::fmt::Write as _;

use super::run::Member;
use super::{End, Flow, ORCHESTRATOR, Role, Stage};

const WAIT_FIRST: &str = "Wait for the orchestrator's first message before doing anything.";

pub(crate) fn member(role: &Role) -> String {
    let prompt = role.prompt.trim();
    let told = if role.edits {
        format!("You are `{}` in this team, and you may edit files and commit in this worktree.", role.name)
    } else {
        format!("You are `{}` in this team: do not edit files and do not commit.", role.name)
    };
    if prompt.is_empty() { format!("{told} {WAIT_FIRST}") } else { format!("{prompt}\n\n{told}") }
}

pub(crate) fn brief(flow: &Flow, task: &str, branch: &str, members: &[Member]) -> String {
    let pane = |role: &str| members.iter().find(|m| m.role == role).map(|m| m.pane);
    let mut out = format!(
        "You are the orchestrator of a team of coding agents that cornercase started for the flow \"{}\", all in \
         this git worktree, on branch {branch}. You run the plan below with the `cornercase` command; the other \
         agents do the work.\n\n## Task\n\n{}\n\n## Team\n\n",
        flow.name,
        task.trim()
    );
    for role in &flow.roles {
        let at = pane(&role.name).map_or_else(|| "no pane".to_string(), |id| format!("pane {id}"));
        let edits = if role.edits { "edits files" } else { "must not edit files" };
        let _ = writeln!(out, "- {} ({at}): {}, {edits}", role.name, agent(role));
    }
    let own = pane(ORCHESTRATOR);
    let you = own.map_or_else(String::new, |id| format!(" (pane {id})"));
    let _ = writeln!(out, "- you, the orchestrator{you}: {}", agent(&flow.orchestrator));
    out.push_str(
        "\n## Plan\n\nRun the stages in order. At each stage, send each of its roles what it needs (the task, what \
         the stages before produced) and wait for all their answers.\n\n",
    );
    for (i, stage) in flow.stages.iter().enumerate() {
        let _ = writeln!(out, "{}. {}", i + 1, stage_line(flow, stage));
    }
    let _ = writeln!(out, "\nAt the end: {}.", end_line(flow.end, branch));
    let notes: Vec<&str> =
        [flow.notes.trim(), flow.orchestrator.prompt.trim()].into_iter().filter(|n| !n.is_empty()).collect();
    if !notes.is_empty() {
        let _ = write!(out, "\n## Notes\n\n{}\n", notes.join("\n\n"));
    }
    out.push_str(&rules(flow, members, own));
    out
}

pub(crate) fn restored(members: &[Member]) -> String {
    let own = members.iter().find(|m| m.role == ORCHESTRATOR).map(|m| m.pane);
    let team: Vec<String> =
        members.iter().filter(|m| m.role != ORCHESTRATOR).map(|m| format!("{} is pane {}", m.role, m.pane)).collect();
    let you = own.map_or_else(String::new, |id| format!(", and you are pane {id}"));
    format!(
        "cornercase restarted, so your team has new pane ids: {}{you}. Use these from now on. Any wait you had \
         running ended, and an agent may have lost the turn it was in: check what each one last answered with \
         `cornercase read --last-message`, then go on with the plan. If the plan was already done, answer `ok` and \
         do nothing else.",
        team.join(", ")
    )
}

fn agent(role: &Role) -> String {
    match role.model.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
        Some(model) => format!("{} · {model}", role.agent),
        None => role.agent.clone(),
    }
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
            "push branch {branch} and open a pull request with `gh pr create`, saying what changed and what the \
             reviews and tests found"
        ),
        End::Commits => format!("leave the commits on branch {branch}; do not push and do not open a pull request"),
    }
}

fn rules(flow: &Flow, members: &[Member], own: Option<u64>) -> String {
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
         `cornercase wait --all {panes} --until turn-over --timeout 300`.\n\
         - Send work with `cornercase send --pane N --enter 'the full instruction'`: one self-contained message \
         saying what to do and what to answer with. An agent knows only what you send it.\n\
         - Wait with `cornercase wait --pane N --until turn-over --timeout 500` (several agents: one wait with a \
         `--pane` each and `--any` or `--all`), in the foreground, with a `--timeout` shorter than your own time \
         limit for a command, and run it again when it times out. Never wait in the background: that ends your \
         turn, and cornercase then shows you as finished while your team works.\n\
         - Read what agents answered with `cornercase read --last-message --pane N` (a `--pane` each for several), \
         not their screens.\n\
         - If `send` fails with `not confirmed`, run `cornercase read --pane N` before sending again: the agent may \
         have it.\n\
         - Never edit files, commit or run the project's checks yourself: the roles do that.\n\
         - Never answer another agent's permission prompt or question (status `waiting` in \
         `cornercase status --panes`); tell the user with `cornercase notify` instead.\n\
         - Stay in this worktree: do not open, close or focus tabs, workspaces or worktrees. Each role's tab is \
         named after it.\n\
         - When the plan is done, do the final action, then run `{mark}` so the user sees the run waits for review, \
         and end your turn with a short summary.\n",
        panes = others.join(" ")
    )
}

#[cfg(test)]
mod tests;
