use std::sync::Arc;
use std::time::Instant;

use super::{App, SearchEvent};
use crate::agents::Known;
use crate::app::AppEvent;
use crate::app::overlay::Overlay;
use crate::log::{Job, Level};
use crate::project::search::said::{self, Found, MAX_MATCHES, Match, Place, Source};
use crate::project::search::{Finder, Mode};
use crate::project::{Group, Hierarchy, Project, Tab, Workspace, sidebar_order};
use crate::term::Term;
use crate::text::{CRUMB_SEPARATOR, Needle};
use crate::ui;

impl App {
    pub(in crate::app) fn build_sources(&mut self, cached: bool) -> Vec<Source> {
        let mut sources = Vec::new();
        for (project, workspace, tab) in open_tabs(&self.projects, &self.groups) {
            let label =
                [project.label(self.home.as_deref()), workspace.label(), tab.label(&self.config)].join(CRUMB_SEPARATOR);
            for term in &tab.panes {
                let kind = term.agent.agent().map(str::to_string);
                let who = kind.clone().or_else(|| term.program(&self.config)).unwrap_or_else(|| "shell".into());
                let record = kind.as_deref().and_then(Known::of).and_then(|kind| term.context.record_for(kind));
                let screen = pane_screen(&mut self.sessions, term, cached);
                let place = Place {
                    project: project.id,
                    workspace: workspace.id,
                    tab: tab.id,
                    pane: term.id,
                    label: label.clone(),
                    who,
                };
                sources.push(Source { place, screen, record });
            }
        }
        sources
    }

    pub(in crate::app) fn ask_sessions(&mut self, now: Instant) {
        let Some(Overlay::Search(Finder { mode: Mode::Sessions, field, .. })) = &self.overlay else {
            if !self.sessions.idle() {
                self.sessions.forget();
            }
            return;
        };
        let query = field.query().trim().to_string();
        let Some((generation, cancel)) = self.sessions.request(&query, now) else { return };
        let sources = if let Some(sources) = self.sessions.corpus(now) {
            sources
        } else {
            let started = Instant::now();
            let built = self.build_sources(true);
            let bytes: usize = built.iter().map(|source| source.screen.len()).sum();
            crate::log::debug!("search", "corpus built", ms = started.elapsed().as_millis(), bytes = bytes);
            self.sessions.built(now, built)
        };
        let job = Job::new(Level::Debug, "search", "sessions");
        let asked = query.clone();
        self.spawn_job(
            job,
            move || Ok(said::search(&sources, &query, &cancel)),
            move |found, job| {
                let found = found.unwrap_or_default();
                job.note("found", found.matches.len());
                Some(AppEvent::Search(SearchEvent::Found { generation, query: asked, found }))
            },
        );
    }

    pub(in crate::app) fn search_answered(&mut self, event: SearchEvent) {
        match event {
            SearchEvent::Found { generation, query, found } => {
                if !self.sessions.answered(generation, &query, found) {
                    return;
                }
                let waiting = matches!(
                    &self.overlay,
                    Some(Overlay::Search(Finder { mode: Mode::Sessions, enter: true, field, .. }))
                        if field.query().trim() == query
                );
                if waiting && let Some(Overlay::Search(dialog)) = &mut self.overlay {
                    dialog.enter = false;
                }
                let first = self.sessions.answer(&query).and_then(|found| found.matches.first().cloned());
                if waiting && let Some(first) = first {
                    self.overlay = None;
                    self.open_match(&first, &query);
                }
            }
            SearchEvent::Request { request, found } => self.sessions_replied(request, found),
        }
    }

    pub(super) fn sessions_rows(
        &self,
        query: &str,
        found: Option<&Found>,
        selected: usize,
    ) -> (Vec<ui::ResultRow>, String, bool) {
        let searching = !query.trim().is_empty() && (found.is_none() || self.sessions.searching());
        let Some(found) = found else {
            return (Vec::new(), if searching { "searching…".into() } else { String::new() }, searching);
        };
        let rows = found.matches.iter().map(|m| ui::ResultRow {
            name: m.snippet.clone(),
            context: m.place.label.clone(),
            detail: format!("{} · {}", m.place.who, m.origin.name()),
            hit: Some(m.hit.clone()),
        });
        let hint = if found.capped {
            format!("the first {MAX_MATCHES} matches")
        } else {
            found.matches.get(selected).map(|m| format!("enter goes to pane {}", m.place.pane)).unwrap_or_default()
        };
        (rows.collect(), hint, false)
    }

    pub(super) fn open_match(&mut self, found: &Match, query: &str) {
        let pane = found.place.pane;
        if self.projects.pane_by(pane).is_none() {
            return;
        }
        self.jump_to_pane(pane);
        if let Some(needle) = Needle::new(query) {
            self.reveal_in(pane, &needle, found.line);
        }
    }

    pub(in crate::app) fn reveal_in(&mut self, pane: u64, needle: &Needle, near: Option<usize>) -> bool {
        let Some(term) = self.projects.pane_by_mut(pane) else { return false };
        term.emulator.reveal(needle, near).unwrap_or(false)
    }
}

fn open_tabs<'a>(projects: &'a [Project], groups: &[Group]) -> Vec<(&'a Project, &'a Workspace, &'a Tab)> {
    sidebar_order(projects, groups)
        .into_iter()
        .map(|p| &projects[p])
        .filter(|project| !project.closing)
        .flat_map(|project| {
            let open = project.workspaces.iter().filter(|w| w.open());
            open.flat_map(move |workspace| workspace.tabs.iter().map(move |tab| (project, workspace, tab)))
        })
        .collect()
}

fn pane_screen(sessions: &mut said::Sessions, term: &Term, cached: bool) -> Arc<str> {
    let read = || term.emulator.text().unwrap_or_default();
    match term.emulator.size() {
        Ok(size) if cached => sessions.screen(term.id, term.output_at, size, read),
        _ => read().into(),
    }
}
