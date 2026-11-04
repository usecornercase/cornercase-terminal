use super::App;
use crate::project::Hierarchy;
use crate::recap::dossier::{Live, LivePane, PullLive};

const SCREEN_LINES: usize = 15;

impl App {
    pub(super) fn recap_live(&self) -> Live {
        let mut live = Live::default();
        for term in self.projects.terms() {
            let Some(kind) = term.context.kind() else { continue };
            let Some(template) = self.journal_entry(term.id) else { continue };
            let Some(key) = template.key() else { continue };
            let status = term.agent.status().map(|status| status.name().to_string());
            let screen = if status.as_deref() == Some("waiting") { screen_lines(term) } else { Vec::new() };
            let _ = kind;
            live.panes.push(LivePane { key, status, template, screen });
        }
        live.pulls = self.recap_pulls();
        live
    }

    fn recap_pulls(&self) -> Vec<PullLive> {
        self.projects
            .workspaces()
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

fn screen_lines(term: &crate::term::Term) -> Vec<String> {
    let text = term.emulator.last_lines(SCREEN_LINES * 2).unwrap_or_default();
    let lines: Vec<String> = text.lines().filter(|line| !line.trim().is_empty()).map(str::to_string).collect();
    let skip = lines.len().saturating_sub(SCREEN_LINES);
    lines[skip..].to_vec()
}
