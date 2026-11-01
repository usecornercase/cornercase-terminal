use std::collections::HashSet;

use super::rows::status;
use super::selection::RemoteItem;
use crate::agents::activity::Status;
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
        let there = There { name: held.name.clone(), boot: after.boot.clone() };
        let shown = self.shown_remote().filter(|shown| shown.link == held.link.id()).map(|shown| shown.workspace);
        for turn in turns(before, after).into_iter().filter(|turn| Some(turn.workspace) != shown) {
            self.tell_turn(turn, there.clone());
        }
    }

    fn tell_turn(&mut self, turn: Turn, there: There) {
        let (what, kind) = notifications::wording(turn.status);
        let sound = self.sound_for(kind, turn.principal);
        let message = notify::clean(&format!("{} on {} {what} in {}", turn.agent, there.name, turn.place));
        log::info!(
            "activity",
            "notify",
            agent = turn.agent,
            status = turn.status.name(),
            machine = there.name,
            workspace = turn.workspace,
            panes = turn.panes.len(),
            sound = sound.map_or("none", sound::Kind::id)
        );
        let Turn { agent, status, project, workspace, place, panes, .. } = turn;
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
        });
    }

    pub(in crate::app) fn go_there(&mut self, there: &There, workspace: u64) -> bool {
        let same =
            self.machines.held.iter().any(|held| {
                held.name == there.name && held.seen.as_deref().is_some_and(|seen| seen.boot == there.boot)
            });
        let went = same && self.select_remote(&there.name, RemoteItem::Workspace(workspace));
        if went {
            self.screen.nav = None;
        }
        went
    }
}

fn turns(before: &control::Report, after: &control::Report) -> Vec<Turn> {
    let working: HashSet<u64> =
        before.panes().iter().filter(|row| watched(row) == Some(Status::Working)).map(|row| row.pane).collect();
    let mut turns = Vec::new();
    for row in after.panes().into_iter().filter(|row| working.contains(&row.pane)) {
        if let Some(status) =
            watched(&row).filter(|status| matches!(status, Status::Waiting | Status::Failed | Status::Done))
        {
            add(&mut turns, row, status);
        }
    }
    turns
}

fn watched(row: &PaneRow) -> Option<Status> {
    status(row.status.as_deref()).filter(|_| row.agent.is_some() && !row.background_shell)
}

fn add(turns: &mut Vec<Turn>, row: PaneRow, status: Status) {
    let place = format!("{} › {}", row.project_name, row.workspace_name);
    let agent = row.agent.unwrap_or_default();
    let principal = row.parent.is_none();
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
