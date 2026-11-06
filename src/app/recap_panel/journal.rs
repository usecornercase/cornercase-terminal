use std::time::SystemTime;

use super::App;
use crate::control;
use crate::log::{Job, Level};
use crate::project::{Hierarchy, Project};
use crate::recap::journal::{Entry, Observed, Pointer, PullPlace, WorkspacePlace, files, key_of};
use crate::recap::{KEPT_DAYS, Named};

impl App {
    pub(in crate::app) fn journal_agents(&mut self, clock: SystemTime, prompts: &[(u64, Vec<SystemTime>)]) {
        let Some(folders) = self.panels.recap.folders.clone() else { return };
        let observed: Vec<Observed> = self
            .projects
            .terms()
            .filter_map(|term| {
                let kind = term.context.kind()?;
                let record = term.context.record_for(kind).as_ref().map(|record| Pointer::of(record).path());
                let conversation = term.context.conversation().map(str::to_string);
                let key = key_of(kind.name(), conversation.as_deref(), record);
                let status = term.agent.status().map(|status| status.name().to_string());
                let prompts = prompts.iter().find(|(pane, _)| *pane == term.id).map(|(_, p)| p.clone());
                Some(Observed { pane: term.id, key, conversation, status, prompts: prompts.unwrap_or_default() })
            })
            .collect();
        let mut journal = std::mem::take(&mut self.panels.recap.journal);
        let entries = journal.observe(clock, &observed, &mut |pane| self.journal_entry(pane));
        self.panels.recap.journal = journal;
        if let Err(error) = files::append(&folders.journal, &entries) {
            crate::log::warning!("recap", "journal write failed", error = error.to_string());
        }
        self.prune_journal(&folders.journal);
    }

    fn prune_journal(&mut self, dir: &std::path::Path) {
        let now = crate::time::now();
        let day = now.div_euclid(86_400);
        if self.panels.recap.pruned_day == Some(day) {
            return;
        }
        self.panels.recap.pruned_day = Some(day);
        let dir = dir.to_path_buf();
        let job = Job::new(Level::Info, "recap", "prune journal");
        self.spawn_job(job, move || Ok(files::prune(&dir, now, KEPT_DAYS)), |_, _| None);
    }

    pub(super) fn journal_entry(&self, pane: u64) -> Option<Entry> {
        let term = self.projects.pane_by(pane)?;
        let kind = term.context.kind()?;
        let at = self.projects.tab_of_pane(pane)?;
        let project: &Project = &self.projects[at.project()];
        let workspace = self.projects.workspace(at.workspace());
        let mut entry = Entry::blank(kind.name());
        entry.account = term.account().map(str::to_string);
        entry.conversation = term.context.conversation().map(str::to_string);
        entry.record = term.context.record_for(kind).as_ref().map(Pointer::of);
        entry.pane = Some(pane);
        entry.boot = Some(control::server_token().to_string());
        entry.project = Some(Named { label: self.project_label(project), path: project.path.display().to_string() });
        entry.workspace = Some(WorkspacePlace {
            label: workspace.label(),
            branch: crate::git::branch(&workspace.path),
            path: workspace.path.display().to_string(),
            worktree: workspace.worktree,
        });
        entry.tab = Some(self.projects.tab(at).label(&self.config));
        entry.pull_request = self.pull_request(workspace.id).map(|pull| PullPlace {
            number: pull.number,
            url: pull.url.clone(),
            state: pull.state.name().to_string(),
            ci: pull.ci().map(|ci| ci.name().to_string()),
        });
        Some(entry)
    }
}
