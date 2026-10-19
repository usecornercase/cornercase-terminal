use std::collections::BTreeMap;

use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::overlay::Overlay;
use super::toast::Toast;
use crate::accounts::AgentAccounts;
use crate::agents;
use crate::control::QueueAdd;
use crate::git;
use crate::issues::browser::{Place, Screen, Tab};
use crate::issues::{self, Issue};
use crate::list::{COMBO_ROWS, Combo};
use crate::project::Hierarchy;
use crate::queue;
use crate::queue::form::{Choice, Control, Form, Step, When};
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
        let confirmed = if entry.is_some() { matching.len() == 1 } else { !places.is_empty() };
        let kinds = self.queue_kinds(entry.as_ref());
        let agent = entry
            .as_ref()
            .and_then(|entry| kinds.iter().position(|kind| *kind == entry.task.agent))
            .unwrap_or_default();
        let zone = entry.as_ref().map_or_else(
            || if self.queued.zone.is_empty() { queue::clock::local_zone() } else { self.queued.zone.clone() },
            |entry| entry.task.zone.clone(),
        );
        let at = entry.as_ref().map_or_else(crate::time::now, |entry| entry.task.at);
        let shown = queue::clock::display(at, &zone);
        let (date, clock) = shown.split_once(' ').unwrap_or((&shown, "09:00"));
        let mut form = Form::new(places, confirmed.then_some(place), kinds, agent, zone.clone(), date);
        if let Some(entry) = &entry {
            form.id = id;
            form.label = (entry.task.label != entry.task.prompt.lines().next().unwrap_or_default())
                .then(|| entry.task.label.clone());
            form.prompt = Editor::new(&entry.task.prompt);
            form.when = When::On.combo();
            form.clock = Editor::new(clock);
            form.open_in.choose(usize::from(entry.task.place.branch.is_some()));
            form.branch = Editor::new(entry.task.place.branch.as_deref().unwrap_or_default());
        }
        self.queued.back = match self.overlay.take() {
            Some(Overlay::Issues(browser)) => Some(browser),
            other => {
                self.overlay = other;
                None
            }
        };
        self.overlay = Some(Overlay::Queue(Box::new(form)));
        self.queue_modes();
        self.queue_accounts(entry.as_ref().map(|entry| &entry.task.accounts));
        if let Some(Overlay::Queue(form)) = &mut self.overlay
            && let Some(mode) = entry.as_ref().and_then(|entry| entry.task.mode.as_ref())
        {
            let mut modes = form.mode.options().to_vec();
            let chosen = modes.iter().position(|name| name == mode).unwrap_or_else(|| {
                modes.push(mode.clone());
                modes.len() - 1
            });
            form.mode = Combo::new(modes, Some(chosen));
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
        let chosen = self.chosen_accounts();
        self.new_queue_form(None);
        if let Some(Overlay::Queue(form)) = &mut self.overlay {
            form.label = Some(format!("{} {}", issue.key, issue.title));
            form.prompt = Editor::new(&prompt);
            form.branch = Editor::new(&branch);
            form.focus = Control::When;
            if let Some(i) = form.agent.options().iter().position(|kind| kind == agent) {
                form.agent.choose(i);
            }
            let path = project.and_then(|id| self.projects.project_index(id)).map(|p| &self.projects[p].path);
            let workspace = place
                .as_ref()
                .and_then(|place| place.workspace)
                .and_then(|id| self.projects.workspace_index(id))
                .map(|(p, w)| &self.projects[p].workspaces[w]);
            let found = form.places.iter().position(|choice| {
                Some(&choice.place.project) == path
                    && choice.place.workspace_id == workspace.map(|workspace| workspace.id)
            });
            form.place = Combo::new(form.place.options().to_vec(), found);
            let worktree =
                place.map_or_else(|| path.is_some_and(|path| git::is_repo_root(path)), |place| place.worktree);
            form.open_in.choose(usize::from(worktree));
        }
        self.queue_modes();
        self.queue_accounts(Some(&chosen));
    }

    fn queue_kind(&self) -> String {
        let Some(Overlay::Queue(form)) = &self.overlay else { return String::new() };
        let selected = form.agent.label().filter(|kind| *kind != agents::AUTO).map(str::to_string);
        self.agent_kind(None, selected).unwrap_or_default()
    }

    fn queue_accounts(&mut self, saved: Option<&BTreeMap<String, String>>) {
        let kind = self.queue_kind();
        let account = self.account_choices().and_then(|choices| {
            let agent = choices.of(&kind)?;
            let saved = saved.and_then(|saved| saved.get(&kind)).map(String::as_str);
            Some(AgentAccounts::new(kind.clone(), &agent.names, agent.labels.clone(), &agent.active).keeping(saved))
        });
        if let Some(Overlay::Queue(form)) = &mut self.overlay {
            if let Some(gone) = account.as_ref().and_then(AgentAccounts::missing) {
                form.error = Some(format!("the {kind} account `{gone}` is gone: pick another account"));
            }
            form.account = account;
        }
    }

    fn queue_modes(&mut self) {
        let kind = self.queue_kind();
        let Some(Overlay::Queue(form)) = &mut self.overlay else { return };
        let modes = std::iter::once("default".into())
            .chain(agents::modes(&self.config, &kind).into_iter().map(|(name, _)| name))
            .collect();
        form.mode = Combo::new(modes, Some(0));
    }

    pub(super) fn queue_key(&mut self, key: KeyEvent, area: Rect) {
        let Some(Overlay::Queue(form)) = &mut self.overlay else { return };
        if form.busy {
            return;
        }
        let width = usize::from(ui::queue_form::text(area, form, form.focus).width.max(1));
        let rows = ui::queue_form::list(area, form).map_or(COMBO_ROWS, ui::combo::rows);
        match form.key(key, width, rows) {
            Step::Submit => self.submit_queue(),
            Step::Cancel => self.close_queue_form(false),
            Step::Chose(control) => self.queue_chose(control),
            Step::None => {}
        }
    }

    fn queue_chose(&mut self, control: Control) {
        if control == Control::Agent {
            self.queue_modes();
            self.queue_accounts(None);
        }
    }

    pub(super) fn queue_form_mouse(&mut self, event: MouseEvent, pos: Position, area: Rect) {
        let Some(Overlay::Queue(form)) = &mut self.overlay else { return };
        if form.busy {
            return;
        }
        if let Some(delta) = super::input::wheel(event.kind) {
            let rows = ui::queue_form::list(area, form).map(ui::combo::rows);
            if let (Some(open), Some(rows)) = (form.opened(), rows) {
                form.combo(open).into_iter().for_each(|combo| combo.scroll_by(delta, rows));
            }
            return;
        }
        if event.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        match ui::queue_form::hit(area, form, pos) {
            Some(ui::queue_form::Hit::Cancel) => self.close_queue_form(false),
            Some(ui::queue_form::Hit::Submit) => self.submit_queue(),
            Some(ui::queue_form::Hit::Close) | None => form.close_lists(),
            Some(ui::queue_form::Hit::Choose(i)) => {
                let Some(open) = form.opened() else { return };
                let changed = form.combo(open).is_some_and(|combo| {
                    let changed = combo.chosen() != Some(i);
                    combo.choose(i);
                    changed
                });
                if changed {
                    form.chose(open);
                    self.queue_chose(open);
                }
            }
            Some(ui::queue_form::Hit::Control(control, row, col)) => {
                let r = ui::queue_form::text(area, form, control);
                let opening = form.combo_of(control).is_some_and(|combo| !combo.is_open());
                form.focus_on(control);
                if let Some(editor) = form.editor(control) {
                    let line = ui::queue_form::first_line(editor, r) + row;
                    editor.place(usize::from(r.width.max(1)), line, col);
                } else if opening {
                    form.combo(control).into_iter().for_each(Combo::open);
                }
            }
        }
    }

    fn submit_queue(&mut self) {
        let Some(Overlay::Queue(form)) = &self.overlay else { return };
        if form.busy {
            return;
        }
        let Some(place) = form.choice() else {
            let reason = if form.places.is_empty() {
                "open a project before scheduling an agent"
            } else {
                "choose an open place before scheduling"
            };
            self.queue_form_done(Err(reason.into()));
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
        let agent = form.agent.label().map(str::to_string);
        let mode = form.mode.label().unwrap_or("default").to_string();
        let captured_default = mode == "default"
            && form
                .id
                .and_then(|id| self.queued.queue.entry(id))
                .is_some_and(|entry| agent.as_ref() == Some(&entry.task.agent) && entry.task.mode.is_none());
        let accounts = self.queue_chosen_accounts(form);
        let add = QueueAdd {
            at: form.at(),
            zone: form.zone.clone(),
            agent,
            mode: (!captured_default).then_some(mode),
            project: Some(project.id),
            workspace: workspace.map(|workspace| workspace.id),
            worktree: form.worktree().then(|| form.branch.text()),
            prompt: Some(form.prompt.text()),
            label: form.label.clone(),
            submit: form.id.is_none().then_some(true),
            accounts,
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
            Err(reason) => self.queue_form_done(Err(reason.into())),
        }
    }

    fn queue_chosen_accounts(&self, form: &Form) -> BTreeMap<String, String> {
        let Some((kind, name)) = form.chosen_account() else { return BTreeMap::new() };
        let entry = form.id.and_then(|id| self.queued.queue.entry(id));
        if entry.is_some_and(|entry| !entry.task.accounts.contains_key(kind)) {
            let active = self.account_choices().and_then(|choices| Some(choices.of(kind)?.active.clone()));
            if active.as_deref() == Some(name) {
                return BTreeMap::new();
            }
        }
        BTreeMap::from([(kind.to_string(), name.to_string())])
    }

    pub(super) fn queue_form_done(&mut self, reply: Result<u64, String>) {
        let form = match &mut self.overlay {
            Some(Overlay::Queue(form)) => Some(form),
            _ => None,
        };
        match (reply, form) {
            (Ok(id), form) => {
                if form.is_some() {
                    self.close_queue_form(true);
                }
                self.toast = Some(Toast::new(format!("scheduled queue entry {id}"), ui::ToastIcon::Check));
            }
            (Err(reason), Some(form)) => {
                form.busy = false;
                form.error = Some(reason);
            }
            (Err(reason), None) => {
                self.toast = Some(Toast::new(format!("could not schedule: {reason}"), ui::ToastIcon::Bug));
            }
        }
    }

    pub(super) fn close_queue_form(&mut self, scheduled: bool) {
        self.overlay = self.queued.back.take().map(|mut browser| {
            browser.scheduling = false;
            if scheduled {
                browser.screen = Screen::List;
                browser.tab_to(Tab::Queue);
                browser.search.reset();
            }
            Overlay::Issues(browser)
        });
        self.sync_queued();
    }

    pub(super) fn queue_form_view(form: &Form) -> Box<Form> {
        Box::new(form.clone())
    }
}
