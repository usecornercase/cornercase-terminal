use std::path::Path;

use super::table::{FLOW_RUN, OWN_ONLY};
use super::{failed, never_asks};
use crate::agents;
use crate::app::App;
use crate::bots::turn;
use crate::control::Item;
use crate::control::contract::Refusal;
use crate::project::Hierarchy;

const NO_BOTS_FOLDER: &str = "a bot cannot open a bot's folder as a project";
const NO_BOTS_WORKTREE: &str = "a bot cannot measure a folder inside a bot's folder";
const NO_CODE_WORKSPACE: &str = "a bot cannot open a .code-workspace file: open its folders one by one";

impl App {
    pub(super) fn closes_own(&self, client: u64, item: Item) -> Result<(), Refusal> {
        let own = self.clients.bots.bot_of(client);
        let started = |term: &crate::term::Term| term.bot.is_some() && term.bot.as_deref() == own;
        let theirs = match item {
            Item::Pane(id) => self.projects.pane_by(id).is_none_or(started),
            Item::Tab(id) => self.projects.tab_index(id).is_none_or(|at| {
                let panes = &self.projects.tab(at).panes;
                !panes.is_empty() && panes.iter().all(started)
            }),
            Item::Workspace(_) | Item::Project(_) | Item::Group(_) => {
                return Err(failed(&format!("{OWN_ONLY}; ask the user to close a workspace or a project")));
            }
        };
        if theirs { Ok(()) } else { Err(failed(&format!("{OWN_ONLY}; {item} was not started by this bot"))) }
    }

    pub(in crate::app::control) fn bot_reopens(&self, client: u64, workspace: u64) -> Result<(), Refusal> {
        if !self.clients.bots.is_bot(client) {
            return Ok(());
        }
        let kept = self.agents.closed.of(workspace);
        let flow = match kept {
            Some(kept) => kept.run.is_some(),
            None => self.runs.of_workspace(workspace).is_some_and(|run| !run.exited.is_empty()),
        };
        if flow {
            return Err(failed(FLOW_RUN));
        }
        let closed = kept.into_iter().flat_map(|kept| &kept.tabs).flat_map(|tab| &tab.panes);
        let mut modes = closed.filter_map(|pane| pane.closed.as_ref()).filter_map(|closed| {
            let mode = closed.mode.as_deref().unwrap_or(agents::DEFAULT_MODE);
            agents::never_asks(&self.config, &closed.kind, Some(mode))
        });
        modes.next().map_or(Ok(()), |name| Err(never_asks(&name)))
    }

    pub(in crate::app::control) fn bot_imports(&self, client: u64) -> Result<(), Refusal> {
        if self.clients.bots.is_bot(client) { Err(failed(NO_CODE_WORKSPACE)) } else { Ok(()) }
    }

    pub(in crate::app::control) fn bot_opens(&self, client: u64, canonical: &Path) -> Result<(), Refusal> {
        if self.in_bots_folder(client, canonical) { Err(failed(NO_BOTS_FOLDER)) } else { Ok(()) }
    }

    pub(in crate::app::control) fn bot_measures(&self, client: u64, canonical: &Path) -> Result<(), Refusal> {
        if self.in_bots_folder(client, canonical) { Err(failed(NO_BOTS_WORKTREE)) } else { Ok(()) }
    }

    fn in_bots_folder(&self, client: u64, canonical: &Path) -> bool {
        self.clients.bots.is_bot(client) && turn::inside(self.panels.bots.dir.as_deref(), canonical)
    }
}
