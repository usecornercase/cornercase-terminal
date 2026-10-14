use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::layout::Side;
use super::overlay::Overlay;
use super::toast::Toast;
use crate::agents;
use crate::control::QueueAdd;
use crate::git;
use crate::issues::{self, Issue, browser::Place};
use crate::project::Hierarchy;
use crate::queue;
use crate::queue::form::{Choice, Field, Form};
use crate::todo::editor::Editor;
use crate::ui;

impl App {
    pub(super) fn new_queue_form(&mut self, id: Option<u64>) {
        let entry = id.and_then(|id| self.queued.queue.entry(id)).cloned();
        let places = self.queue_places();
        let matching: Vec<usize> = entry
            .as_ref()
            .map(|entry| {
                places
                    .iter()
                    .enumerate()
                    .filter(|(_, choice)| {
                        if entry.task.place.server == crate::control::server_token()
                            && entry.task.place.workspace_id.is_some()
                        {
                            choice.place.workspace_id == entry.task.place.workspace_id
                        } else {
                            choice.place.project == entry.task.place.project
                                && choice.place.workspace == entry.task.place.workspace
                                && choice.place.workspace_name == entry.task.place.workspace_name
                        }
                    })
                    .map(|(i, _)| i)
                    .collect()
            })
            .unwrap_or_default();
        let place = matching
            .first()
            .copied()
            .or_else(|| {
                self.project().and_then(|project| {
                    places.iter().position(|choice| {
                        choice.place.project == project.path
                            && choice.place.workspace_id == project.workspace().map(|workspace| workspace.id)
                    })
                })
            })
            .unwrap_or_default();
        let place_confirmed = if entry.is_some() { matching.len() == 1 } else { !places.is_empty() };
        let kinds = self.queue_kinds(entry.as_ref());
        let agent = entry
            .as_ref()
            .and_then(|entry| kinds.iter().position(|kind| *kind == entry.task.agent))
            .unwrap_or_default();
        let zone = entry.as_ref().map_or_else(
            || if self.queued.zone.is_empty() { queue::clock::local_zone() } else { self.queued.zone.clone() },
            |entry| entry.task.zone.clone(),
        );
        let form = Form {
            id,
            label: entry
                .as_ref()
                .filter(|entry| entry.task.label != entry.task.prompt.lines().next().unwrap_or_default())
                .map(|entry| entry.task.label.clone()),
            prompt: Editor::new(entry.as_ref().map_or("", |entry| &entry.task.prompt)),
            at: Editor::new(
                &entry.as_ref().map_or_else(|| "in 2h".into(), |entry| queue::clock::display(entry.task.at, &zone)),
            ),
            branch: Editor::new(
                entry.as_ref().and_then(|entry| entry.task.place.branch.as_deref()).unwrap_or_default(),
            ),
            field: Field::Prompt,
            zone,
            places,
            place,
            place_confirmed,
            kinds,
            agent,
            modes: vec!["default".into()],
            mode: 0,
            worktree: entry.as_ref().is_some_and(|entry| entry.task.place.branch.is_some()),
            busy: false,
            error: None,
        };
        self.overlay = Some(Overlay::Queue(Box::new(form)));
        self.queue_modes();
        if let Some(Overlay::Queue(form)) = &mut self.overlay
            && let Some(mode) = entry.as_ref().and_then(|entry| entry.task.mode.as_ref())
        {
            form.mode = form.modes.iter().position(|name| name == mode).unwrap_or_else(|| {
                form.modes.push(mode.clone());
                form.modes.len() - 1
            });
        }
    }

    fn queue_kinds(&self, entry: Option<&queue::Entry>) -> Vec<String> {
        let mut kinds: Vec<_> = std::iter::once(agents::AUTO.to_string()).chain(agents::kinds(&self.config)).collect();
        if let Some(entry) = entry
            && !kinds.contains(&entry.task.agent)
        {
            kinds.push(entry.task.agent.clone());
        }
        kinds
    }

    fn queue_places(&self) -> Vec<Choice> {
        let mut places = Vec::new();
        for project in self.projects.iter().filter(|project| !project.closing) {
            places.push(Choice {
                label: format!("{} · root", self.project_label(project)),
                place: queue::Place {
                    project: project.path.clone(),
                    workspace: None,
                    workspace_name: None,
                    workspace_id: None,
                    server: String::new(),
                    branch: None,
                },
                worktree: git::is_repo_root(&project.path),
            });
            for workspace in project.workspaces.iter().filter(|workspace| workspace.open()) {
                places.push(Choice {
                    label: if project
                        .workspaces
                        .iter()
                        .filter(|other| other.open() && other.label() == workspace.label())
                        .count()
                        > 1
                    {
                        format!("{} › {} (workspace {})", self.project_label(project), workspace.label(), workspace.id)
                    } else {
                        format!("{} › {}", self.project_label(project), workspace.label())
                    },
                    place: queue::Place {
                        project: project.path.clone(),
                        workspace: Some(workspace.path.clone()),
                        workspace_name: workspace.name.clone(),
                        workspace_id: Some(workspace.id),
                        server: crate::control::server_token().into(),
                        branch: None,
                    },
                    worktree: false,
                });
            }
        }
        places
    }

    pub(super) fn schedule_issue(&mut self, issue: &Issue, agent: &str, place: Option<Place>) {
        let project = place.as_ref().map(|place| place.project).or(match &self.overlay {
            Some(Overlay::Issues(browser)) => Some(browser.project),
            _ => None,
        });
        let branch = issues::branch(issue);
        let prompt = issues::prompt(&self.config.prompt, issue, &branch);
        self.new_queue_form(None);
        if let Some(Overlay::Queue(form)) = &mut self.overlay {
            form.label = Some(format!("{} {}", issue.key, issue.title));
            form.prompt = Editor::new(&prompt);
            form.branch = Editor::new(&branch);
            form.field = Field::At;
            form.agent = form.kinds.iter().position(|kind| kind == agent).unwrap_or_default();
            let path = project.and_then(|id| self.projects.project_index(id)).map(|p| &self.projects[p].path);
            form.place_confirmed = false;
            let workspace = place
                .as_ref()
                .and_then(|place| place.workspace)
                .and_then(|id| self.projects.workspace_index(id))
                .map(|(p, w)| &self.projects[p].workspaces[w]);
            if let Some(index) = form.places.iter().position(|choice| {
                Some(&choice.place.project) == path
                    && choice.place.workspace_id == workspace.map(|workspace| workspace.id)
            }) {
                form.place = index;
                form.place_confirmed = true;
            }
            form.worktree =
                place.map_or_else(|| path.is_some_and(|path| git::is_repo_root(path)), |place| place.worktree);
        }
        self.queue_modes();
    }

    fn queue_modes(&mut self) {
        let Some(Overlay::Queue(form)) = &mut self.overlay else { return };
        let selected = form.kinds.get(form.agent).cloned();
        let kind = self.agent_kind(None, selected.filter(|kind| kind != agents::AUTO)).unwrap_or_default();
        let Some(Overlay::Queue(form)) = &mut self.overlay else { return };
        form.modes = std::iter::once("default".into())
            .chain(agents::modes(&self.config, &kind).into_iter().map(|(name, _)| name))
            .collect();
        form.mode = 0;
    }

    pub(super) fn queue_key(&mut self, key: KeyEvent) {
        let Some(Overlay::Queue(form)) = &mut self.overlay else { return };
        if form.busy {
            return;
        }
        match key.code {
            KeyCode::Esc => self.overlay = None,
            KeyCode::Enter => self.submit_queue(),
            KeyCode::Tab | KeyCode::BackTab => {
                form.field = match form.field {
                    Field::Prompt => Field::At,
                    Field::At => Field::Branch,
                    Field::Branch => Field::Prompt,
                }
            }
            _ => {
                let area = self.queued.area.unwrap_or(Rect::new(0, 0, 72, 22));
                let width = usize::from(ui::queue_form::field(area, form.field).width.max(1));
                form.key(key, width);
            }
        }
    }

    pub(super) fn queue_form_mouse(&mut self, event: MouseEvent, pos: Position, area: Rect) {
        if event.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let Some(Overlay::Queue(form)) = &mut self.overlay else { return };
        if form.busy {
            return;
        }
        match ui::queue_form::hit(area, pos) {
            Some(ui::queue_form::Hit::Cancel) => self.overlay = None,
            Some(ui::queue_form::Hit::Submit) => self.submit_queue(),
            Some(ui::queue_form::Hit::Field(field, line, col)) => {
                form.field = field;
                form.editor().place(usize::from(ui::queue_form::field(area, field).width.max(1)), line, col);
            }
            Some(ui::queue_form::Hit::Place) => {
                if form.place_confirmed {
                    form.place = (form.place + 1) % form.places.len().max(1);
                }
                form.place_confirmed = !form.places.is_empty();
                if !form.places.get(form.place).is_some_and(|place| place.worktree) {
                    form.worktree = false;
                }
            }
            Some(ui::queue_form::Hit::Agent) => {
                form.agent = (form.agent + 1) % form.kinds.len().max(1);
                self.queue_modes();
            }
            Some(ui::queue_form::Hit::Mode) => form.mode = (form.mode + 1) % form.modes.len().max(1),
            Some(ui::queue_form::Hit::Worktree) if form.places.get(form.place).is_some_and(|place| place.worktree) => {
                form.worktree = !form.worktree;
            }
            Some(ui::queue_form::Hit::ClaudeReset) => {
                form.at = Editor::new("claude reset");
                form.field = Field::At;
            }
            Some(ui::queue_form::Hit::CodexReset) => {
                form.at = Editor::new("codex reset");
                form.field = Field::At;
            }
            _ => {}
        }
    }

    fn submit_queue(&mut self) {
        let Some(Overlay::Queue(form)) = &self.overlay else { return };
        if form.busy {
            return;
        }
        if !form.place_confirmed {
            self.queue_form_done(Err("choose an open place before scheduling".into()));
            return;
        }
        let Some(place) = form.places.get(form.place) else {
            self.queue_form_done(Err("open a project before scheduling an agent".into()));
            return;
        };
        let Some(project) =
            self.projects.iter().find(|project| !project.closing && project.path == place.place.project)
        else {
            self.queue_form_done(Err("the selected project closed; choose an open place".into()));
            return;
        };
        let workspace = place.place.workspace.as_ref().and_then(|path| {
            project.workspaces.iter().find(|workspace| {
                workspace.open() && workspace.path == *path && Some(workspace.id) == place.place.workspace_id
            })
        });
        if place.place.workspace.is_some() && workspace.is_none() {
            self.queue_form_done(Err("the selected workspace closed; choose an open place".into()));
            return;
        }
        let agent = form.kinds.get(form.agent).cloned();
        let captured_default = form.modes[form.mode] == "default"
            && form
                .id
                .and_then(|id| self.queued.queue.entry(id))
                .is_some_and(|entry| agent.as_ref() == Some(&entry.task.agent) && entry.task.mode.is_none());
        let add = QueueAdd {
            at: form.at.text(),
            zone: form.zone.clone(),
            agent,
            mode: (!captured_default).then(|| form.modes[form.mode].clone()),
            project: Some(project.id),
            workspace: workspace.map(|workspace| workspace.id),
            worktree: form.worktree.then(|| form.branch.text()),
            prompt: Some(form.prompt.text()),
            label: form.label.clone(),
            submit: form.id.is_none().then_some(true),
            ..QueueAdd::default()
        };
        let id = form.id;
        match self.add_queue_request(None, None, id, &add) {
            Ok(Some(value)) => {
                self.queue_form_done(value["id"].as_u64().ok_or_else(|| "the queue returned no id".into()));
            }
            Ok(None) => {
                if let Some(Overlay::Queue(form)) = &mut self.overlay {
                    form.busy = true;
                }
            }
            Err(reason) => self.queue_form_done(Err(reason)),
        }
    }

    pub(super) fn queue_form_done(&mut self, reply: Result<u64, String>) {
        match reply {
            Ok(id) => {
                self.overlay = None;
                self.show_side(Some(Side::Queue));
                self.toast = Some(Toast::new(format!("scheduled queue entry {id}"), ui::ToastIcon::Check));
            }
            Err(reason) => {
                if let Some(Overlay::Queue(form)) = &mut self.overlay {
                    form.busy = false;
                    form.error = Some(reason);
                }
            }
        }
    }

    pub(super) fn queue_form_view(form: &Form) -> ui::queue_form::Form {
        ui::queue_form::Form {
            prompt: form.prompt.clone(),
            at: form.at.clone(),
            branch: form.branch.clone(),
            field: form.field,
            zone: form.zone.clone(),
            place: if form.place_confirmed {
                form.places.get(form.place).map_or_else(|| "no open project".into(), |place| place.label.clone())
            } else {
                "choose an open place…".into()
            },
            agent: form.kinds.get(form.agent).cloned().unwrap_or_default(),
            mode: form.modes.get(form.mode).cloned().unwrap_or_default(),
            worktree: form.worktree,
            busy: form.busy,
            note: if form.busy { Some("reading the plan reset…".into()) } else { form.error.clone() },
        }
    }
}
