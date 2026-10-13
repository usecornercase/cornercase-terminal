use std::path::Path;
use std::time::{Duration, Instant};

use ratatui::layout::Rect;

use super::App;
use crate::agents;
use crate::error::Result;
use crate::launch::Launch;
use crate::log::{self};
use crate::project::{Tab, Workspace};
use crate::state::{AgentState, TabState, WorkspaceState};
use crate::term::Term;

pub(super) const RESUME_GRACE: Duration = Duration::from_secs(30);

impl App {
    pub(super) fn restore_workspace(&mut self, saved: &WorkspaceState, area: Rect) -> Option<(Workspace, usize)> {
        let path = saved.path.canonicalize().unwrap_or_else(|_| saved.path.clone());
        if !path.is_dir() {
            return None;
        }
        let mut workspace = Workspace::new(self.take_id(), path.clone(), saved.name.clone(), saved.worktree);
        workspace.base.clone_from(&saved.base);
        workspace.collapsed = saved.collapsed;
        let mut lost = 0;
        for saved_tab in saved.tabs.iter().filter(|t| !t.panes.is_empty()) {
            match self.restore_tab(saved_tab, &path, area) {
                Ok(tab) => workspace.tabs.push(tab),
                Err(e) => {
                    log::error!("server", "could not restore a tab", workspace = path.display(), error = e);
                    lost += 1;
                }
            }
        }
        workspace.active = saved.active.min(workspace.tabs.len().saturating_sub(1));
        Some((workspace, lost))
    }

    pub(super) fn restore_tab(&mut self, saved: &TabState, dir: &Path, area: Rect) -> Result<Tab> {
        let mut panes = Vec::new();
        for pane in &saved.panes {
            let cwd = pane.cwd.clone().filter(|cwd| cwd.is_dir()).unwrap_or_else(|| dir.to_path_buf());
            let mut term = self.spawn(area, cwd)?;
            self.resume(&mut term, pane.agent.as_ref());
            panes.push(term);
        }
        let mut tab = Tab::restored(self.take_id(), saved.name.clone(), panes, saved.layout.as_ref());
        tab.active = saved.active.min(tab.panes.len() - 1);
        tab.right_clicks =
            saved.panes.iter().zip(&tab.panes).filter(|(s, _)| s.right_clicks).map(|(_, t)| t.id).collect();
        Ok(tab)
    }

    fn resume(&mut self, term: &mut Term, agent: Option<&AgentState>) {
        let Some(agent) = agent.filter(|_| self.config.resume_agents) else { return };
        let Some(line) = agents::resume_line(&self.config, agent) else { return };
        log::info!("app", "resuming a conversation", pane = term.id, agent = agent.kind);
        let now = Instant::now();
        self.launches.push(Launch::command(term.id, line, now));
        term.resume = Some(agent.clone());
        term.resuming = Some(now + RESUME_GRACE);
    }
}

pub(super) fn not_restored(missed: &[(String, usize)]) -> String {
    let tabs = match missed.iter().map(|(_, n)| n).sum::<usize>() {
        1 => "1 tab".to_string(),
        n => format!("{n} tabs"),
    };
    let projects: Vec<&str> = missed.iter().map(|(name, _)| name.as_str()).collect();
    format!("could not restore {tabs} ({}), see server.log", projects.join(", "))
}
