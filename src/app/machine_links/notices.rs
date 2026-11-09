use std::collections::HashSet;

use super::rows::status;
use super::selection::RemoteItem;
use crate::agents::activity::Status;
use crate::agents::tree;
use crate::app::App;
use crate::app::notifications::{self, Said, There};
use crate::control::{self, PaneRow};
use crate::host::{notify, sound};
use crate::log;

struct Turn {
    agent: String,
    status: Status,
    project: u64,
    workspace: u64,
    place: String,
    panes: Vec<u64>,
    principal: bool,
}

impl App {
    pub(super) fn tell_turns(&mut self, at: usize, before: Option<&control::Report>) {
        let held = &self.machines.held[at];
        let Some((before, after)) =
            before.zip(held.seen.as_deref()).filter(|(before, after)| before.boot == after.boot)
        else {
            return;
        };
        let there = There { link: held.link.id(), name: held.name.clone(), boot: after.boot.clone() };
        let shown = self.shown_remote().filter(|shown| shown.link == held.link.id()).map(|shown| shown.workspace);
        for turn in turns(before, after).into_iter().filter(|turn| Some(turn.workspace) != shown) {
            self.tell_turn(turn, there.clone());
        }
    }

    fn tell_turn(&mut self, turn: Turn, there: There) {
        let (what, kind) = notifications::wording(turn.status);
        let sent = self.tells(turn.principal);
        let sound = self.sound_for(kind, turn.principal).filter(|_| sent);
        let message = notify::clean(&format!("{} on {} {what} in {}", turn.agent, there.name, turn.place));
        log::info!(
            "activity",
            "notify",
            agent = turn.agent,
            status = turn.status.name(),
            machine = there.name,
            workspace = turn.workspace,
            panes = turn.panes.len(),
            sound = sound.map_or("none", sound::Kind::id),
            subagent = !turn.principal,
            sent = sent
        );
        if !sent {
            return;
        }
        let Turn { agent, status, project, workspace, place, panes, principal } = turn;
        self.tell(Said {
            agent: &agent,
            status,
            place,
            message,
            project,
            workspace,
            panes,
            sound,
            machine: Some(there),
            subagent: !principal,
        });
    }

    pub(in crate::app) fn go_there(&mut self, there: &There, workspace: u64) -> bool {
        let same = self.machines.held.iter().any(|held| {
            held.link.id() == there.link && held.seen.as_deref().is_some_and(|seen| seen.boot == there.boot)
        });
        let went = same && self.select_on(there.link, RemoteItem::Workspace(workspace));
        if went {
            self.screen.nav = None;
        }
        went
    }
}

fn turns(before: &control::Report, after: &control::Report) -> Vec<Turn> {
    let working: HashSet<(u64, Option<String>)> = before
        .panes()
        .into_iter()
        .filter(|row| watched(row) == Some(Status::Working))
        .map(|row| (row.pane, row.agent))
        .collect();
    let nested = subagents(after);
    let mut turns = Vec::new();
    for row in after.panes().into_iter().filter(|row| working.contains(&(row.pane, row.agent.clone()))) {
        if let Some(status) =
            watched(&row).filter(|status| matches!(status, Status::Waiting | Status::Failed | Status::Done))
        {
            let principal = !nested.contains(&row.pane);
            add(&mut turns, row, status, principal);
        }
    }
    turns
}

fn subagents(report: &control::Report) -> HashSet<u64> {
    let rows: Vec<(u64, Option<u64>)> = report
        .panes()
        .into_iter()
        .filter(|row| status(row.status.as_deref()).is_some())
        .map(|row| (row.pane, row.parent))
        .collect();
    let nodes = tree::order(&rows, &HashSet::new(), false);
    nodes.into_iter().filter(|node| node.depth > 0).map(|node| rows[node.index].0).collect()
}

fn watched(row: &PaneRow) -> Option<Status> {
    status(row.status.as_deref()).filter(|_| row.agent.is_some() && !row.background_shell)
}

fn add(turns: &mut Vec<Turn>, row: PaneRow, status: Status, principal: bool) {
    let place = format!("{} › {}", row.project_name, row.workspace_name);
    let agent = row.agent.unwrap_or_default();
    let same = |turn: &&mut Turn| turn.agent == agent && turn.status == status && turn.workspace == row.workspace;
    if let Some(turn) = turns.iter_mut().find(same) {
        turn.panes.push(row.pane);
        turn.principal |= principal;
        return;
    }
    turns.push(Turn {
        agent,
        status,
        project: row.project,
        workspace: row.workspace,
        place,
        panes: vec![row.pane],
        principal,
    });
}
