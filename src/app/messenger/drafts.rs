use ratatui::layout::Rect;

use super::{Asked, Delivery, Route};
use crate::agents;
use crate::app::App;
use crate::control::{Command, Start};
use crate::git;
use crate::messenger::{Choice, Keyboard, Message, Order, Tap, Via};
use crate::project::{Hierarchy, Workspace};
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
    pub(super) fn new_draft(&mut self, via: Via, edit: Option<i64>, _area: Rect) {
        let messenger = self.messenger_mut(via);
        messenger.next_draft += 1;
        let id = messenger.next_draft;
        messenger.drafts.push_back(Draft { id, project: None, place: None, agent: None });
        while messenger.drafts.len() > DRAFTS {
            messenger.drafts.pop_front();
        }
        self.advance_draft(via, id, edit);
    }

    fn draft_mut(&mut self, via: Via, id: u32) -> Option<&mut Draft> {
        self.messenger_mut(via).drafts.iter_mut().find(|d| d.id == id)
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

    fn installed_agents(&self, via: Via) -> Vec<String> {
        let kinds = agents::kinds(&self.config).into_iter();
        let run = self.messenger(via).run.clone();
        kinds
            .filter(|kind| agents::installed(&agents::command(&self.config, kind)))
            .filter(|kind| Tap::Draft(0, Choice::Agent(kind.clone())).fits(&run, via))
            .collect()
    }

    fn next(&mut self, via: Via, id: u32) -> Next {
        let Some(draft) = self.messenger(via).drafts.iter().find(|d| d.id == id).cloned() else { return Next::Gone };
        let project = match self.draft_project(via, &draft) {
            Ok(project) => project,
            Err(next) => return next,
        };
        let place = match self.draft_place(via, &draft, project) {
            Ok(place) => place,
            Err(next) => return next,
        };
        let place_label = self.place_label(project, &place);
        let installed = self.installed_agents(via);
        let resolved = agents::resolve(&self.config, None, None);
        let Some(agent) = draft.agent.clone().or(resolved).or_else(|| match installed.as_slice() {
            [only] => Some(only.clone()),
            _ => None,
        }) else {
            let keyboard = installed
                .iter()
                .map(|kind| vec![self.messenger(via).tap(kind.clone(), &Tap::Draft(id, Choice::Agent(kind.clone())))])
                .collect();
            return Next::Agent(place_label, keyboard);
        };
        if let Some(draft) = self.draft_mut(via, id) {
            draft.project = Some(project);
            draft.place = Some(place);
            draft.agent = Some(agent.clone());
        }
        Next::Prompt(agent, place_label)
    }

    fn draft_project(&self, via: Via, draft: &Draft) -> Result<u64, Next> {
        let projects: Vec<u64> = crate::project::sidebar_order(&self.projects, &self.groups)
            .into_iter()
            .filter(|&p| !self.projects[p].closing)
            .map(|p| self.projects[p].id)
            .collect();
        let only = match projects.as_slice() {
            [only] => Some(*only),
            _ => None,
        };
        draft.project.filter(|p| projects.contains(p)).or(only).ok_or_else(|| {
            let keyboard = projects
                .iter()
                .filter_map(|&project| self.projects.project_index(project))
                .map(|p| {
                    let project = &self.projects[p];
                    let tap = Tap::Draft(draft.id, Choice::Project(project.id));
                    vec![self.messenger(via).tap(self.project_label(project), &tap)]
                })
                .collect();
            Next::Project(keyboard)
        })
    }

    fn draft_place(&self, via: Via, draft: &Draft, project: u64) -> Result<Place, Next> {
        let id = draft.id;
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
        draft.place.clone().filter(open).or(only).ok_or_else(|| {
            let mut keyboard: Keyboard = workspaces
                .iter()
                .map(|(ws, label)| {
                    vec![self.messenger(via).tap(label.clone(), &Tap::Draft(id, Choice::Workspace(*ws)))]
                })
                .collect();
            if root {
                keyboard.push(vec![self.messenger(via).tap("+ new worktree", &Tap::Draft(id, Choice::Worktree))]);
            }
            let name = self.projects.project_index(project).map(|p| self.project_label(&self.projects[p]));
            Next::Place(name.unwrap_or_default(), keyboard)
        })
    }

    fn show_step(&mut self, via: Via, edit: Option<i64>, text: String, keyboard: Keyboard) {
        match (edit, self.messenger(via).chat().map(|c| c.id)) {
            (Some(message), Some(chat)) => self.post(via, Order::Edit { chat, message, text, keyboard }),
            _ => self.say(via, text, Delivery { keyboard, ..Delivery::default() }),
        }
    }

    fn advance_draft(&mut self, via: Via, id: u32, edit: Option<i64>) {
        let m = via.markup();
        match self.next(via, id) {
            Next::Gone => self.say(via, EXPIRED.into(), Delivery::default()),
            Next::Project(keyboard) if keyboard.is_empty() => {
                self.show_step(via, edit, NO_PROJECT.into(), Vec::new());
            }
            Next::Project(keyboard) => {
                self.show_step(via, edit, "Start an agent in which project?".into(), keyboard);
            }
            Next::Place(project, keyboard) => {
                self.show_step(via, edit, format!("Where in {}?", m.bold(&m.escape(&project))), keyboard);
            }
            Next::Agent(place, keyboard) => {
                self.show_step(via, edit, format!("Which agent, in {}?", m.escape(&place)), keyboard);
            }
            Next::Prompt(agent, place) => {
                if edit.is_some() {
                    let text = format!("{} in {}", m.bold(&m.escape(&agent)), m.escape(&place));
                    self.show_step(via, edit, text, Vec::new());
                }
                let text = format!("What should {} do in {}?", m.bold(&m.escape(&agent)), m.escape(&place));
                self.ask(via, text, &format!("prompt for {agent}"), (Route::Prompt(id), None));
            }
        }
    }

    pub(super) fn draft_choice(&mut self, via: Via, id: u32, choice: Choice, message: i64) {
        let Some(draft) = self.draft_mut(via, id) else {
            self.show_step(via, Some(message), EXPIRED.into(), Vec::new());
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
                let m = via.markup();
                let name = m.escape(&project.map(|p| self.project_label(&self.projects[p])).unwrap_or_default());
                let name = m.bold(&name);
                self.show_step(via, Some(message), format!("A new worktree in {name}"), Vec::new());
                self.ask(via, format!("Name of the new branch in {name}:"), "branch", (Route::Branch(id), None));
                return;
            }
        }
        self.advance_draft(via, id, Some(message));
    }

    pub(super) fn draft_branch(&mut self, via: Via, id: u32, branch: &str, message: i64) {
        if branch.is_empty() {
            self.reply(via, message, NO_BRANCH);
            return;
        }
        let Some(draft) = self.draft_mut(via, id) else {
            self.reply(via, message, EXPIRED);
            return;
        };
        draft.place = Some(Place::Worktree(branch.to_string()));
        self.advance_draft(via, id, None);
    }

    pub(super) fn draft_prompt(&mut self, via: Via, id: u32, message: &Message, area: Rect) {
        let (prompt, message) = (message.text.as_str(), message.id);
        if prompt.trim().is_empty() {
            self.reply(via, message, NO_PROMPT);
            return;
        }
        let drafts = &mut self.messenger_mut(via).drafts;
        let found = drafts.iter().position(|d| d.id == id);
        let Some(draft) = found.and_then(|i| drafts.remove(i)) else {
            self.reply(via, message, EXPIRED);
            return;
        };
        let (Some(project), Some(place), Some(agent)) = (draft.project, draft.place, draft.agent) else {
            self.reply(via, message, EXPIRED);
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
        self.chat_request(via, Command::Start(start), asked, Some(area).filter(|a| a.area() > 0));
    }
}
