use std::path::{Path, PathBuf};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use serde_json::Value;

use super::App;
use super::event::AppEvent;
use super::input::wheel;
use super::machines::MachineEvent;
use super::machines::relay::Then;
use super::overlay::Overlay;
use super::text_fields::FieldArea;
use super::toast::Toast;
use crate::control::contract::{Code, Refusal};
use crate::control::{CloneFile, CloneRepository, Cloned};
use crate::editor::Editor;
use crate::error::{Error, Result};
use crate::git::ignored::{self, IgnoredFile, Listing};
use crate::git::remote;
use crate::log::{Job, Level};
use crate::machines::DEFAULT_FOLDER;
use crate::machines::act::UNREADABLE;
use crate::machines::clone_form::{CloneForm, Opening, Step, cloned_note};
use crate::machines::run_on::{self, Target};
use crate::machines::send::Send;
use crate::project::Hierarchy;
use crate::ui::ToastIcon;
use crate::ui::clone_on_machine::{self as view, Hit};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) struct Pending {
    key: u64,
    link: u64,
    project: u64,
    machine: String,
    label: String,
}

#[derive(Debug)]
pub(super) struct Cloning {
    form: CloneForm,
    back: Overlay,
    pending: Pending,
}

impl Cloning {
    pub(super) fn form(&self) -> &CloneForm {
        &self.form
    }
}

#[derive(Debug)]
pub(crate) struct Packed {
    key: u64,
    clone: CloneRepository,
    result: std::result::Result<Vec<CloneFile>, String>,
}

impl App {
    pub(in crate::app) fn open_clone_on_machine(&mut self, target: &Target, project: u64) {
        if self.still_cloning(target.link, project) {
            return;
        }
        let Some(at) = self.projects.project_index(project) else { return };
        let shown = &self.projects[at];
        let Some(url) = remote::identity_url(&shown.repo.remotes).map(str::to_string) else { return };
        let opening = Opening {
            project: shown.label(self.home.as_deref()),
            machine: target.name.clone(),
            folder: self.machine_folder(&target.name),
            root: shown.path.clone(),
            url,
        };
        let (root, label) = (opening.root.clone(), opening.project.clone());
        let Some(back) = self.overlay.take() else { return };
        let key = self.clients.requests.key();
        let pending = Pending { key, link: target.link, project, machine: target.name.clone(), label };
        let cloning = Cloning { form: CloneForm::new(opening), back, pending };
        self.overlay = Some(Overlay::CloneOnMachine(Box::new(cloning)));
        self.list_ignored(key, root);
    }

    pub(in crate::app) fn still_cloning(&self, link: u64, project: u64) -> bool {
        self.machines.clones.iter().any(|pending| pending.link == link && pending.project == project)
    }

    fn machine_folder(&self, name: &str) -> String {
        let machine = self.config.machines.iter().find(|machine| machine.name.eq_ignore_ascii_case(name));
        machine.map_or(DEFAULT_FOLDER, |machine| machine.folder.as_str()).to_string()
    }

    fn list_ignored(&self, key: u64, root: PathBuf) {
        let job = Job::new(Level::Debug, "machines", "list ignored files");
        self.spawn_job(
            job,
            move || Ok(ignored::list(&root)),
            move |result, job| {
                let result = quietly(result.and_then(std::convert::identity), job);
                Some(AppEvent::Machine(MachineEvent::Ignored { key, result }))
            },
        );
    }

    pub(in crate::app) fn ignored_listed(&mut self, key: u64, result: std::result::Result<Listing, String>) {
        if let Some(Overlay::CloneOnMachine(cloning)) = &mut self.overlay
            && cloning.pending.key == key
        {
            cloning.form.listed(result);
        }
    }

    fn clone_form(&mut self) -> Option<&mut CloneForm> {
        match &mut self.overlay {
            Some(Overlay::CloneOnMachine(cloning)) => Some(&mut cloning.form),
            _ => None,
        }
    }

    pub(super) fn clone_on_machine_key(&mut self, key: KeyEvent, area: Rect) {
        let width = url_width(area);
        let Some(form) = self.clone_form() else { return };
        let step = form.key(key, width);
        self.clone_step(step);
    }

    pub(super) fn clone_on_machine_paste(&mut self, text: &str, area: Rect) {
        let width = url_width(area);
        if let Some(form) = self.clone_form() {
            form.paste(text, width);
        }
    }

    pub(super) fn clone_on_machine_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let width = url_width(area);
        let rows = usize::from(view::list(area).height);
        let Some(form) = self.clone_form() else { return };
        if let Some(delta) = wheel(ev.kind) {
            form.scroll_by(delta, rows);
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let step = match view::hit(area, form, pos) {
            Some(Hit::Button(label)) => form.press(label),
            Some(Hit::Url { col }) => {
                form.place(width, col);
                Step::None
            }
            Some(Hit::File(at)) => {
                form.toggle(at);
                Step::None
            }
            None => Step::None,
        };
        self.clone_step(step);
    }

    pub(super) fn clone_url_text(&mut self, area: Rect) -> Option<(FieldArea, &mut Editor)> {
        let text = view::url_text(area);
        let form = self.clone_form()?;
        let field = FieldArea { text, width: usize::from(text.width.max(1)), top: form.url.scroll(), scrolls: true };
        Some((field, &mut form.url))
    }

    pub(super) fn clone_back(&mut self) {
        self.leave_clone();
        self.follow_run_on();
    }

    fn leave_clone(&mut self) {
        if let Some(Overlay::CloneOnMachine(cloning)) = self.overlay.take() {
            self.overlay = Some(cloning.back);
        }
    }

    fn cloning_with(&self, key: u64) -> bool {
        matches!(&self.overlay, Some(Overlay::CloneOnMachine(cloning)) if cloning.pending.key == key)
    }

    fn take_clone(&mut self, key: u64) -> Option<Pending> {
        let at = self.machines.clones.iter().position(|pending| pending.key == key)?;
        Some(self.machines.clones.swap_remove(at))
    }

    fn follow_run_on(&mut self) {
        self.follow_issues_run_on();
        self.follow_start_flow_run_on();
        self.follow_new_workspace_run_on();
    }

    fn clone_step(&mut self, step: Step) {
        match step {
            Step::None => {}
            Step::Back => self.clone_back(),
            Step::Clone { url, files } => self.start_cloning(url, files),
        }
    }

    fn start_cloning(&mut self, url: String, files: Vec<IgnoredFile>) {
        let Some(Overlay::CloneOnMachine(cloning)) = &mut self.overlay else { return };
        cloning.form.cloning = true;
        let (form, pending) = (&cloning.form, cloning.pending.clone());
        let clone = CloneRepository { url, parent: PathBuf::from(&form.folder), focus: false, files: Vec::new() };
        let root = form.root.clone();
        self.machines.clones.push(pending.clone());
        if files.is_empty() {
            self.send_clone(clone, &pending);
            return;
        }
        let key = pending.key;
        let job = Job::new(Level::Debug, "machines", "read ignored files").with("files", files.len());
        self.spawn_job(
            job,
            move || Ok(packed(&root, &files)),
            move |result, job| {
                let result = quietly(result.and_then(std::convert::identity), job);
                Some(AppEvent::Machine(MachineEvent::Packed(Box::new(Packed { key, clone, result }))))
            },
        );
    }

    pub(in crate::app) fn clone_packed(&mut self, packed: Packed) {
        let Packed { key, mut clone, result } = packed;
        let Some(pending) = self.machines.clones.iter().find(|pending| pending.key == key).cloned() else { return };
        match result {
            Ok(files) => {
                clone.files = files;
                self.send_clone(clone, &pending);
            }
            Err(why) => self.clone_failed(&pending, &why),
        }
    }

    fn send_clone(&mut self, clone: CloneRepository, pending: &Pending) {
        let sent = match self.held_name(pending.link) {
            Some(name) => self.send_to_machine(&name, Send::Clone(clone), Then::Clone(pending.key)).map(|_| ()),
            None => Err(Refusal::new(Code::Unavailable, format!("{} is no longer listed", pending.machine))),
        };
        if let Err(refusal) = sent {
            self.clone_failed(pending, &refusal.message);
        }
    }

    pub(in crate::app) fn clone_relayed(&mut self, key: u64, result: std::result::Result<Value, Refusal>) {
        let Some(pending) = self.machines.clones.iter().find(|pending| pending.key == key).cloned() else { return };
        let cloned = match result.map(serde_json::from_value::<Cloned>) {
            Ok(Ok(cloned)) => cloned,
            Ok(Err(_)) => return self.clone_failed(&pending, UNREADABLE),
            Err(refusal) => {
                let why = run_on::reason(&self.clone_machine(&pending), &refusal.message);
                return self.clone_failed(&pending, &why);
            }
        };
        self.take_clone(key);
        if self.cloning_with(key) {
            self.leave_clone();
        }
        if let Some(name) = self.held_name(pending.link)
            && self.run_on_dialog().is_some_and(|dialog| dialog.shows(pending.link, pending.project))
        {
            self.check_cloned(&name, cloned.ids.project);
        }
        let note = cloned_note(&self.clone_project(&pending), &self.clone_machine(&pending), &cloned.unwritten);
        self.messages.toast = Some(Toast::new(note, ToastIcon::Check));
    }

    fn clone_failed(&mut self, pending: &Pending, why: &str) {
        self.take_clone(pending.key);
        let said = format!("the clone on {} failed: {why}", self.clone_machine(pending));
        self.messages.toast = Some(Toast::new(said, ToastIcon::Bug));
        if let Some(Overlay::CloneOnMachine(cloning)) = &mut self.overlay
            && cloning.pending.key == pending.key
        {
            cloning.form.cloning = false;
        }
    }

    fn held_name(&self, link: u64) -> Option<String> {
        self.held_by(link).map(|held| held.name.clone())
    }

    fn clone_machine(&self, pending: &Pending) -> String {
        self.held_name(pending.link).unwrap_or_else(|| pending.machine.clone())
    }

    fn clone_project(&self, pending: &Pending) -> String {
        let at = self.projects.project_index(pending.project);
        at.map_or_else(|| pending.label.clone(), |at| self.projects[at].label(self.home.as_deref()))
    }
}

fn url_width(area: Rect) -> usize {
    usize::from(view::url_text(area).width.max(1))
}

fn packed(root: &Path, files: &[IgnoredFile]) -> Result<Vec<CloneFile>> {
    let top = ignored::toplevel(root)?;
    let file = |file: &IgnoredFile| {
        let data = STANDARD.encode(ignored::read(&top, file)?);
        Ok(CloneFile { path: file.path.clone(), data, mode: file.mode })
    };
    files.iter().map(file).collect()
}

fn quietly<T>(result: Result<T>, job: &mut Job) -> std::result::Result<T, String> {
    let (result, logged) = told(result);
    if let Some(logged) = logged {
        job.fail(logged);
    }
    result
}

pub(in crate::app) fn told<T>(result: Result<T>) -> (std::result::Result<T, String>, Option<String>) {
    match result {
        Ok(value) => (Ok(value), None),
        Err(error) => (Err(run_on::clipped_or(&error.to_string(), "failed")), Some(failure(&error))),
    }
}

fn failure(error: &Error) -> String {
    match error {
        Error::Io(error) => format!("{:?}", error.kind()),
        Error::RunGit(_) | Error::Git(_) => "git".into(),
        _ => "other".into(),
    }
}
