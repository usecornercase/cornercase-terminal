use std::collections::BTreeMap;
use std::path::Path;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::persist;

mod steps;

pub(crate) use steps::TOURS;

pub(crate) const CORE: &str = "core";

pub(crate) const DOCS: &str = "https://usecornercase.dev/docs/";

pub(crate) const VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Target {
    None,
    NewProject,
    Projects,
    Workspaces,
    Tabs,
    Pane,
    Agents,
    Issues,
    Changes,
    Files,
    Todo,
    Search,
    Settings,
    Usage,
    Remote,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Panel {
    Changes,
    Files,
    Todo,
    Flow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Dialog {
    Issues,
    Search,
    Settings,
    Usage,
    Remote,
    Keys,
    Flows,
    Queue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Page {
    Agents,
    Accounts,
    Flows,
    Ui,
    Remote,
    Plugins,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(crate) enum Show {
    Panel { panel: Panel },
    Issues,
    Queue,
    Search,
    Keys,
    Settings { page: Page },
    HelpMenu,
    RemoteMenu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(crate) enum Done {
    Next,
    ProjectOpened,
    WorkspaceMade,
    TabOpened,
    AgentStarted,
    PromptConfirmed,
    Panel { panel: Panel },
    Opened { dialog: Dialog },
    TodoAdded,
    AgentsSection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Offer {
    Prompt,
    AgentsSection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Step {
    pub(crate) id: &'static str,
    pub(crate) title: &'static str,
    pub(crate) text: &'static str,
    pub(crate) target: Target,
    pub(crate) done: Done,
    pub(crate) offer: Option<Offer>,
    pub(crate) show: Option<Show>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tour {
    pub(crate) id: &'static str,
    pub(crate) title: &'static str,
    pub(crate) docs: &'static str,
    pub(crate) steps: &'static [Step],
}

impl Tour {
    pub(crate) fn docs_url(&self) -> String {
        format!("{DOCS}{}", self.docs)
    }
}

pub(crate) fn find(id: &str) -> Option<(usize, &'static Tour)> {
    TOURS.iter().enumerate().find(|(_, t)| t.id == id)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Facts {
    pub(crate) projects: usize,
    pub(crate) workspaces: usize,
    pub(crate) tabs: usize,
    pub(crate) panes: usize,
    pub(crate) agents: usize,
    pub(crate) prompted: Option<SystemTime>,
    pub(crate) panel: Option<Panel>,
    pub(crate) dialog: Option<Dialog>,
    pub(crate) todos: usize,
    pub(crate) agents_section: bool,
    pub(crate) repository: bool,
    pub(crate) compact: bool,
}

impl Done {
    pub(crate) fn met(self, start: &Facts, now: &Facts) -> bool {
        match self {
            Self::Next => false,
            Self::ProjectOpened => now.projects > start.projects,
            Self::WorkspaceMade => now.workspaces > start.workspaces,
            Self::TabOpened => now.tabs > start.tabs || now.panes > start.panes,
            Self::AgentStarted => now.agents > start.agents,
            Self::PromptConfirmed => now.prompted.is_some_and(|at| start.prompted.is_none_or(|before| at > before)),
            Self::Panel { panel } => now.panel == Some(panel),
            Self::Opened { dialog } => now.dialog == Some(dialog),
            Self::TodoAdded => now.todos > start.todos,
            Self::AgentsSection => now.agents_section,
        }
    }

    pub(crate) fn asks(self, facts: &Facts) -> bool {
        match self {
            Self::Next => false,
            Self::ProjectOpened => facts.projects == 0,
            Self::WorkspaceMade => facts.repository,
            Self::AgentsSection => !facts.agents_section,
            _ => true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Progress {
    pub(crate) done: Vec<String>,
    pub(crate) finished: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Saved {
    pub(crate) version: u32,
    pub(crate) offered: bool,
    pub(crate) announced: bool,
    pub(crate) tours: BTreeMap<String, Progress>,
}

impl Default for Saved {
    fn default() -> Self {
        Self { version: VERSION, offered: false, announced: false, tours: BTreeMap::new() }
    }
}

impl Saved {
    pub(crate) fn progress(&self, tour: &str) -> Progress {
        self.tours.get(tour).cloned().unwrap_or_default()
    }

    pub(crate) fn complete(&mut self, tour: &Tour, step: &str) -> bool {
        if !tour.steps.iter().any(|s| s.id == step) {
            return false;
        }
        let progress = self.tours.entry(tour.id.to_string()).or_default();
        if progress.done.iter().any(|d| d == step) {
            return false;
        }
        progress.done.push(step.to_string());
        true
    }

    pub(crate) fn finish(&mut self, tour: &Tour) {
        self.tours.entry(tour.id.to_string()).or_default().finished = true;
    }

    pub(crate) fn finished(&self, tour: &str) -> bool {
        self.tours.get(tour).is_some_and(|p| p.finished)
    }

    pub(crate) fn resume_at(&self, tour: &Tour) -> usize {
        if self.finished(tour.id) {
            return 0;
        }
        let progress = self.progress(tour.id);
        tour.steps.iter().position(|s| !progress.done.iter().any(|d| d == s.id)).unwrap_or(0)
    }

    pub(crate) fn reset(&mut self, tour: Option<&str>) {
        match tour {
            Some(id) => {
                self.tours.remove(id);
            }
            None => self.tours.clear(),
        }
    }
}

pub(crate) fn first_run(session: &Path, stats: &Path, star: &crate::star::Saved) -> bool {
    !session.exists() && !stats.exists() && *star == crate::star::Saved::default()
}

pub(crate) fn load(path: &Path) -> Option<Saved> {
    let text = std::fs::read_to_string(path).ok()?;
    Some(serde_json::from_str::<Saved>(&text).ok().filter(|saved| saved.version == VERSION).unwrap_or_default())
}

pub(crate) fn save(path: &Path, saved: &Saved) -> std::io::Result<()> {
    persist::save(path, saved)
}

#[cfg(test)]
mod tests;
