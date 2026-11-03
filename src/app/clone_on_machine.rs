use std::path::{Path, PathBuf};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use serde_json::Value;

use super::App;
use super::event::AppEvent;
use super::input::wheel;
use super::issues_dialog::run_on::issue_project;
use super::machines::MachineEvent;
use super::machines::relay::{Sent, Then};
use super::overlay::Overlay;
use super::text_fields::FieldArea;
use super::toast::Toast;
use crate::control::contract::{Code, Refusal};
use crate::control::{CloneFile, CloneRepository, Cloned, Unwritten};
use crate::editor::Editor;
use crate::error::{Error, Result};
use crate::git::ignored::{self, IgnoredFile, Listing};
use crate::git::remote;
use crate::log::{self, Job, Level};
use crate::machines::DEFAULT_FOLDER;
use crate::machines::clone_form::{CloneForm, Opening, Step};
use crate::machines::run_on::{self, RunOn, Target};
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
        if self.machines.clones.iter().any(|pending| pending.link == target.link && pending.project == project) {
            return self.say_in_dialog(format!("still cloning on {}…", target.name));
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
        let root = opening.root.clone();
        let Some(back) = self.overlay.take() else { return };
        let pending =
            Pending { key: self.clients.requests.key(), link: target.link, project, machine: target.name.clone() };
        let cloning = Cloning { form: CloneForm::new(opening), back, pending };
        self.overlay = Some(Overlay::CloneOnMachine(Box::new(cloning)));
        self.list_ignored(root);
    }

    fn machine_folder(&self, name: &str) -> String {
        let machine = self.config.machines.iter().find(|machine| machine.name.eq_ignore_ascii_case(name));
        machine.map_or(DEFAULT_FOLDER, |machine| machine.folder.as_str()).to_string()
    }

    fn list_ignored(&self, root: PathBuf) {
        let job = Job::new(Level::Debug, "machines", "list ignored files");
        let listed = root.clone();
        self.spawn_job(
            job,
            move || Ok(ignored::list(&listed)),
            move |result, job| {
                let result = quietly(result.and_then(std::convert::identity), job);
                Some(AppEvent::Machine(MachineEvent::Ignored { root, result }))
            },
        );
    }

    pub(in crate::app) fn ignored_listed(&mut self, root: &Path, result: std::result::Result<Listing, String>) {
        if let Some(form) = self.clone_form().filter(|form| form.root == root) {
            form.listed(result);
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
        let Some(Overlay::CloneOnMachine(cloning)) = self.overlay.take() else { return };
        self.overlay = Some(cloning.back);
        self.follow_run_on();
    }

    fn cloning_with(&self, key: u64) -> bool {
        matches!(&self.overlay, Some(Overlay::CloneOnMachine(cloning)) if cloning.pending.key == key)
    }

    fn shows_clone(&self, pending: &Pending) -> bool {
        let chosen = |run_on: Option<&RunOn>| run_on.and_then(RunOn::machine).map(|target| target.link);
        let (link, project) = match &self.overlay {
            Some(Overlay::Issues(b)) => (b.machine().map(|target| target.link), issue_project(b)),
            Some(Overlay::StartFlow { start, .. }) => (chosen(start.run_on.as_ref()), start.project),
            Some(Overlay::NewWorkspace { project, run_on, .. }) => (chosen(run_on.as_ref()), *project),
            Some(Overlay::CloneOnMachine(cloning)) => (Some(cloning.pending.link), cloning.pending.project),
            _ => return false,
        };
        link == Some(pending.link) && project == pending.project
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
        if !self.shows_clone(&pending) {
            self.take_clone(key);
            log::info!("machines", "files read for a clone that was left", name = pending.machine);
            let left = format!("the clone on {} was not sent", pending.machine);
            self.messages.toast = Some(Toast::new(left, ToastIcon::Bug));
            return;
        }
        match result {
            Ok(files) => {
                clone.files = files;
                self.send_clone(clone, &pending);
            }
            Err(why) => {
                self.take_clone(key);
                self.say_in_dialog(why);
            }
        }
    }

    fn send_clone(&mut self, clone: CloneRepository, pending: &Pending) {
        let held = self.machines.held().iter().find(|held| held.link.id() == pending.link);
        let sent = match held.map(|held| held.name.clone()) {
            Some(name) => self.send_to_machine(&name, Send::Clone(clone), Then::Clone(pending.key)).map(|_| ()),
            None => Err(Refusal::new(Code::Unavailable, format!("{} is no longer listed", pending.machine))),
        };
        if let Err(refusal) = sent {
            self.take_clone(pending.key);
            self.say_in_dialog(refusal.message);
        }
    }

    fn say_in_dialog(&mut self, why: String) {
        match &mut self.overlay {
            Some(Overlay::CloneOnMachine(cloning)) => {
                cloning.form.cloning = false;
                cloning.form.error = Some(why);
            }
            Some(Overlay::Issues(b)) => b.error = Some(why),
            Some(Overlay::StartFlow { start, .. }) => start.notice = Some(why),
            Some(Overlay::NewWorkspace { error, .. }) => *error = Some(why),
            _ => {}
        }
    }

    pub(in crate::app) fn clone_relayed(&mut self, sent: &Sent, key: u64, result: std::result::Result<Value, Refusal>) {
        let Some(pending) = self.take_clone(key) else { return };
        let cloned = result.map(|value| serde_json::from_value::<Cloned>(value).unwrap_or_default());
        if cloned.is_ok() && self.cloning_with(key) {
            self.clone_back();
        }
        if let Ok(cloned) = &cloned
            && let Some(held) = self.machines.held().iter().find(|held| held.link.id() == pending.link)
        {
            let name = held.name.clone();
            self.check_cloned(&name, cloned.ids.project);
        }
        if !self.shows_clone(&pending) {
            log::info!("machines", "clone answered after its dialog closed", name = sent.machine);
            return;
        }
        match cloned {
            Ok(cloned) => self.cloned_in_dialog(&pending.machine, &cloned.unwritten),
            Err(refusal) => self.say_in_dialog(run_on::reason(&pending.machine, &refusal.message)),
        }
    }

    fn cloned_in_dialog(&mut self, machine: &str, unwritten: &[Unwritten]) {
        let note = cloned_note(machine, unwritten);
        match &mut self.overlay {
            Some(Overlay::Issues(b)) => b.notice = Some(note),
            Some(Overlay::StartFlow { start, .. }) if !unwritten.is_empty() => start.notice = Some(note),
            Some(Overlay::NewWorkspace { error, .. }) if !unwritten.is_empty() => *error = Some(note),
            _ => {}
        }
        self.follow_run_on();
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

fn cloned_note(name: &str, unwritten: &[Unwritten]) -> String {
    let left = unwritten.iter().map(|file| format!("{} ({})", file.path.display(), file.why));
    match run_on::capped(left) {
        Some(left) => format!("cloned on {name} · not copied: {left}"),
        None => format!("cloned on {name}"),
    }
}

fn quietly<T>(result: Result<T>, job: &mut Job) -> std::result::Result<T, String> {
    result.map_err(|error| {
        job.fail(failure(&error));
        run_on::clipped_or(&error.to_string(), "failed")
    })
}

pub(in crate::app) fn failure(error: &Error) -> String {
    match error {
        Error::Io(error) => format!("{:?}", error.kind()),
        Error::RunGit(_) | Error::Git(_) => "git".into(),
        _ => "other".into(),
    }
}
