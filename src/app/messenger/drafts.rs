use ratatui::layout::Rect;

use super::{Asked, Route};
use crate::agents;
use crate::app::App;
use crate::control::{Command, Start};
use crate::git;
use crate::project::{Hierarchy, Workspace};
use crate::telegram::{Choice, Keyboard, Order, Tap, html};
use crate::text::CRUMB_SEPARATOR;

const DRAFTS: usize = 8;
const NO_PROJECT: &str = "No project is open in cornercase.";
const EXPIRED: &str = "That choice expired: /new starts again.";
const NO_BRANCH: &str = "The branch needs a name: reply again with one.";
const NO_PROMPT: &str = "The prompt is empty: reply again with what the agent should do.";

#[derive(Debug, Clone, PartialEq, Eq)]
enum Place {
    Workspace(u64),
    Worktree(String),
}

#[derive(Debug, Clone)]
pub(super) struct Draft {
    id: u32,
    project: Option<u64>,
    place: Option<Place>,
    agent: Option<String>,
}

enum Next {
    Project(Keyboard),
    Place(String, Keyboard),
    Agent(String, Keyboard),
    Prompt(String, String),
    Gone,
}

impl App {
    pub(super) fn new_draft(&mut self, edit: Option<i64>, _area: Rect) {
        self.telegram.next_draft += 1;
        let id = self.telegram.next_draft;
        self.telegram.drafts.push_back(Draft { id, project: None, place: None, agent: None });
        while self.telegram.drafts.len() > DRAFTS {
            self.telegram.drafts.pop_front();
        }
        self.advance_draft(id, edit);
    }

    fn draft_mut(&mut self, id: u32) -> Option<&mut Draft> {
        self.telegram.drafts.iter_mut().find(|d| d.id == id)
    }

    fn open_workspaces(&self, project: u64) -> Vec<&Workspace> {
        let Some(p) = self.projects.project_index(project) else { return Vec::new() };
        self.projects[p].workspaces.iter().filter(|w| w.open()).collect()
    }

    fn place_label(&self, project: u64, place: &Place) -> String {
        let name = self.projects.project_index(project).map(|p| self.project_label(&self.projects[p]));
        let name = name.unwrap_or_default();
        match place {
            Place::Workspace(id) => {
                let workspace = self.open_workspaces(project).into_iter().find(|w| w.id == *id).map(Workspace::label);
                format!("{name}{CRUMB_SEPARATOR}{}", workspace.unwrap_or_default())
            }
            Place::Worktree(branch) => format!("{name}{CRUMB_SEPARATOR}{branch} (a new worktree)"),
        }
    }

    fn installed_agents(&self) -> Vec<String> {
        let kinds = agents::kinds(&self.config).into_iter();
        let run = self.telegram.run.clone();
        kinds
            .filter(|kind| agents::installed(&agents::command(&self.config, kind)))
            .filter(|kind| Tap::Draft(0, Choice::Agent(kind.clone())).fits(&run))
            .collect()
    }

    fn next(&mut self, id: u32) -> Next {
        let projects: Vec<u64> = crate::project::sidebar_order(&self.projects, &self.groups)
            .into_iter()
            .filter(|&p| !self.projects[p].closing)
            .map(|p| self.projects[p].id)
            .collect();
        let Some(draft) = self.telegram.drafts.iter().find(|d| d.id == id).cloned() else { return Next::Gone };
        let only = match projects.as_slice() {
            [only] => Some(*only),
            _ => None,
        };
        let Some(project) = draft.project.filter(|p| projects.contains(p)).or(only) else {
            let keyboard = projects
                .iter()
                .filter_map(|&id| self.projects.project_index(id))
                .map(|p| {
                    let project = &self.projects[p];
                    vec![self.telegram.tap(self.project_label(project), &Tap::Draft(id, Choice::Project(project.id)))]
                })
                .collect();
            return Next::Project(keyboard);
        };
        let workspaces: Vec<(u64, String)> =
            self.open_workspaces(project).into_iter().map(|w| (w.id, w.label())).collect();
        let root = self.projects.project_index(project).is_some_and(|p| git::is_repo_root(&self.projects[p].path));
        let only = match workspaces.as_slice() {
            [(only, _)] if !root => Some(Place::Workspace(*only)),
            _ => None,
        };
        let open = |place: &Place| match place {
            Place::Workspace(ws) => workspaces.iter().any(|(id, _)| id == ws),
            Place::Worktree(_) => true,
        };
        let Some(place) = draft.place.clone().filter(open).or(only) else {
            let mut keyboard: Keyboard = workspaces
                .iter()
                .map(|(ws, label)| vec![self.telegram.tap(label.clone(), &Tap::Draft(id, Choice::Workspace(*ws)))])
                .collect();
            if root {
                keyboard.push(vec![self.telegram.tap("+ new worktree", &Tap::Draft(id, Choice::Worktree))]);
            }
            let name = self.projects.project_index(project).map(|p| self.project_label(&self.projects[p]));
            return Next::Place(name.unwrap_or_default(), keyboard);
        };
        let place_label = self.place_label(project, &place);
        let installed = self.installed_agents();
        let resolved = agents::resolve(&self.config, None, None);
        let Some(agent) = draft.agent.clone().or(resolved).or_else(|| match installed.as_slice() {
            [only] => Some(only.clone()),
            _ => None,
        }) else {
            let keyboard = installed
                .iter()
                .map(|kind| vec![self.telegram.tap(kind.clone(), &Tap::Draft(id, Choice::Agent(kind.clone())))])
                .collect();
            return Next::Agent(place_label, keyboard);
        };
        if let Some(draft) = self.draft_mut(id) {
            draft.project = Some(project);
            draft.place = Some(place);
            draft.agent = Some(agent.clone());
        }
        Next::Prompt(agent, place_label)
    }

    fn show_step(&mut self, edit: Option<i64>, html: String, keyboard: Keyboard) {
        match (edit, self.telegram.chat().map(|c| c.id)) {
            (Some(message), Some(chat)) => self.post(Order::Edit { chat, message, html, keyboard }),
            _ => self.say(html, keyboard, None, None),
        }
    }

    fn advance_draft(&mut self, id: u32, edit: Option<i64>) {
        match self.next(id) {
            Next::Gone => self.say(EXPIRED.into(), Vec::new(), None, None),
            Next::Project(keyboard) if keyboard.is_empty() => self.show_step(edit, NO_PROJECT.into(), Vec::new()),
            Next::Project(keyboard) => self.show_step(edit, "Start an agent in which project?".into(), keyboard),
            Next::Place(project, keyboard) => {
                self.show_step(edit, format!("Where in <b>{}</b>?", html::escape(&project)), keyboard);
            }
            Next::Agent(place, keyboard) => {
                self.show_step(edit, format!("Which agent, in {}?", html::escape(&place)), keyboard);
            }
            Next::Prompt(agent, place) => {
                if edit.is_some() {
                    let html = format!("<b>{}</b> in {}", html::escape(&agent), html::escape(&place));
                    self.show_step(edit, html, Vec::new());
                }
                let html = format!("What should <b>{}</b> do in {}?", html::escape(&agent), html::escape(&place));
                self.ask(html, &format!("prompt for {agent}"), Route::Prompt(id));
            }
        }
    }

    pub(super) fn draft_choice(&mut self, id: u32, choice: Choice, message: i64, _area: Rect) {
        let Some(draft) = self.draft_mut(id) else {
            self.show_step(Some(message), EXPIRED.into(), Vec::new());
            return;
        };
        match choice {
            Choice::Project(project) => {
                draft.project = Some(project);
                draft.place = None;
            }
            Choice::Workspace(workspace) => draft.place = Some(Place::Workspace(workspace)),
            Choice::Agent(kind) => draft.agent = Some(kind),
            Choice::Worktree => {
                let project = draft.project.and_then(|p| self.projects.project_index(p));
                let name = html::escape(&project.map(|p| self.project_label(&self.projects[p])).unwrap_or_default());
                self.show_step(Some(message), format!("A new worktree in <b>{name}</b>"), Vec::new());
                self.ask(format!("Name of the new branch in <b>{name}</b>:"), "branch", Route::Branch(id));
                return;
            }
        }
        self.advance_draft(id, Some(message));
    }

    pub(super) fn draft_branch(&mut self, id: u32, branch: &str, message: i64) {
        if branch.is_empty() {
            self.reply(message, NO_BRANCH);
            return;
        }
        let Some(draft) = self.draft_mut(id) else {
            self.reply(message, EXPIRED);
            return;
        };
        draft.place = Some(Place::Worktree(branch.to_string()));
        self.advance_draft(id, None);
    }

    pub(super) fn draft_prompt(&mut self, id: u32, prompt: &str, message: i64, area: Rect) {
        if prompt.trim().is_empty() {
            self.reply(message, NO_PROMPT);
            return;
        }
        let found = self.telegram.drafts.iter().position(|d| d.id == id);
        let Some(draft) = found.and_then(|i| self.telegram.drafts.remove(i)) else {
            self.reply(message, EXPIRED);
            return;
        };
        let (Some(project), Some(place), Some(agent)) = (draft.project, draft.place, draft.agent) else {
            self.reply(message, EXPIRED);
            return;
        };
        let label = self.place_label(project, &place);
        let (workspace, worktree, project) = match place {
            Place::Workspace(id) => (Some(id), None, None),
            Place::Worktree(branch) => (None, Some(branch), Some(project)),
        };
        let start = Start {
            agent: Some(agent.clone()),
            project,
            worktree,
            workspace,
            prompt: Some(prompt.to_string()),
            ..Start::default()
        };
        let asked = Asked::Start { message, place: label, agent };
        self.telegram_request(Command::Start(start), asked, Some(area).filter(|a| a.area() > 0));
    }
}
