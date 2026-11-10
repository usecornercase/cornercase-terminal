use super::App;
use crate::control;
use crate::project::Hierarchy;
use crate::recap::dossier::{Live, LivePane, PullLive};
use crate::recap::journal::{Pointer, key_of};
use crate::recap::outdated::{Present, PresentPane};
use crate::term::Term;

const SCREEN_LINES: usize = 15;

pub(super) struct Tracked {
    pub(super) agent: String,
    pub(super) conversation: Option<String>,
    pub(super) key: Option<String>,
    pub(super) status: Option<String>,
}

pub(super) fn tracked(term: &Term) -> Option<Tracked> {
    let kind = term.context.kind()?;
    let record = term.context.record_for(kind).as_ref().map(|record| Pointer::of(record).path());
    let conversation = term.context.conversation().map(str::to_string);
    let key = key_of(kind.name(), conversation.as_deref(), record);
    let status = term.agent.status().map(|status| status.name().to_string());
    Some(Tracked { agent: kind.name().to_string(), conversation, key, status })
}

impl App {
    pub(super) fn recap_live(&self) -> Live {
        let mut live = Live::default();
        for term in self.projects.terms() {
            let Some(found) = tracked(term) else { continue };
            let Some(key) = found.key else { continue };
            let Some(template) = self.journal_entry(term.id) else { continue };
            let status = found.status;
            let screen = if status.as_deref() == Some("waiting") { screen_lines(term) } else { Vec::new() };
            live.panes.push(LivePane { key, status, template, screen });
        }
        live.pulls = self.recap_pulls();
        live
    }

    pub(super) fn recap_present(&self) -> Present {
        let panes = self
            .projects
            .terms()
            .filter_map(|term| {
                let found = tracked(term)?;
                found.key.as_ref()?;
                Some(PresentPane {
                    agent: found.agent,
                    conversation: found.conversation,
                    pane: term.id,
                    status: found.status,
                })
            })
            .collect();
        Present { panes, pulls: self.recap_pulls(), boot: control::server_token().to_string() }
    }

    fn recap_pulls(&self) -> Vec<PullLive> {
        self.projects
            .workspaces()
            .filter(|workspace| workspace.open())
            .filter_map(|workspace| {
                let pull = self.pull_request(workspace.id)?;
                Some(PullLive {
                    workspace: workspace.path.display().to_string(),
                    number: pull.number,
                    title: Some(pull.title.clone()),
                    state: pull.state.name().to_string(),
                    checks: pull.ci().map(|ci| ci.name().to_string()),
                    url: pull.url.clone(),
                })
            })
            .collect()
    }
}

fn screen_lines(term: &Term) -> Vec<String> {
    let text = term.emulator.last_lines(SCREEN_LINES * 2).unwrap_or_default();
    let lines: Vec<String> = text.lines().filter(|line| !line.trim().is_empty()).map(str::to_string).collect();
    let skip = lines.len().saturating_sub(SCREEN_LINES);
    lines[skip..].to_vec()
}
