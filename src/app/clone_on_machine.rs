use std::path::{Path, PathBuf};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use serde_json::Value;

use super::App;
use super::event::AppEvent;
use super::input::wheel;
use super::machines::MachineEvent;
use super::machines::relay::{Sent, Then};
use super::overlay::Overlay;
use super::text_fields::FieldArea;
use crate::control::contract::Refusal;
use crate::control::{CloneFile, CloneRepository, Cloned, Unwritten};
use crate::editor::Editor;
use crate::error::Result;
use crate::git::ignored::{self, IgnoredFile, Listing};
use crate::git::remote;
use crate::log::{self, Job, Level};
use crate::machines::DEFAULT_FOLDER;
use crate::machines::clone_form::{CloneForm, Opening, Step};
use crate::machines::run_on::{self, Target};
use crate::machines::send::Send;
use crate::project::Hierarchy;
use crate::ui::clone_on_machine::{self as view, Hit};

#[derive(Debug)]
pub(super) struct Cloning {
    form: CloneForm,
    back: Overlay,
    key: u64,
}

impl Cloning {
    pub(super) fn form(&self) -> &CloneForm {
        &self.form
    }
}

impl App {
    pub(in crate::app) fn open_clone_on_machine(&mut self, target: &Target, project: u64) {
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
        let key = self.clients.requests.key();
        let cloning = Cloning { form: CloneForm::new(opening), back, key };
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
            move || ignored::list(&listed),
            move |result, _| {
                let result = result.map_err(|error| error.to_string());
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
        let Cloning { form, back, key } = *cloning;
        self.overlay = Some(back);
        if let Some(run_on) = self.dialog_run_on().filter(|_| form.cloning) {
            run_on.awaiting = Some(key);
        }
        self.follow_run_on();
    }

    fn cloning_with(&self, key: u64) -> bool {
        matches!(&self.overlay, Some(Overlay::CloneOnMachine(cloning)) if cloning.key == key)
    }

    fn awaits_clone(&mut self, key: u64) -> bool {
        let Some(run_on) = self.dialog_run_on().filter(|run_on| run_on.awaiting == Some(key)) else { return false };
        run_on.awaiting = None;
        true
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
        if files.is_empty() {
            self.send_clone(url, Vec::new());
            return;
        }
        let (root, key) = (cloning.form.root.clone(), cloning.key);
        let job = Job::new(Level::Debug, "machines", "read ignored files").with("files", files.len());
        self.spawn_job(
            job,
            move || packed(&root, &files),
            move |result, _| {
                let result = result.map_err(|error| error.to_string());
                Some(AppEvent::Machine(MachineEvent::Packed { key, url, result }))
            },
        );
    }

    pub(in crate::app) fn clone_packed(
        &mut self,
        key: u64,
        url: String,
        result: std::result::Result<Vec<CloneFile>, String>,
    ) {
        if !self.cloning_with(key) {
            log::info!("machines", "files read for a clone dialog that closed");
            return;
        }
        match result {
            Ok(files) => self.send_clone(url, files),
            Err(why) => self.clone_refused(why),
        }
    }

    fn send_clone(&mut self, url: String, files: Vec<CloneFile>) {
        let Some(Overlay::CloneOnMachine(cloning)) = &self.overlay else { return };
        let (form, key) = (&cloning.form, cloning.key);
        let machine = form.machine.clone();
        let clone = CloneRepository { url, parent: PathBuf::from(&form.folder), focus: false, files };
        if let Err(refusal) = self.send_to_machine(&machine, Send::Clone(clone), Then::Clone(key)) {
            self.clone_refused(refusal.message);
        }
    }

    fn clone_refused(&mut self, why: String) {
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
        let cloned = result.map(|value| serde_json::from_value::<Cloned>(value).unwrap_or_default());
        if cloned.is_ok() && self.cloning_with(key) {
            self.clone_back();
        }
        if !self.cloning_with(key) && !self.awaits_clone(key) {
            log::info!("machines", "clone answered after its dialog closed", name = sent.machine);
            return;
        }
        let cloned = match cloned {
            Ok(cloned) => cloned,
            Err(refusal) => return self.clone_refused(refusal.message),
        };
        self.check_cloned(&sent.machine, cloned.ids.project);
        let note = cloned_note(&sent.machine, &cloned.unwritten);
        match &mut self.overlay {
            Some(Overlay::Issues(b)) => b.notice = Some(note),
            Some(Overlay::StartFlow { start, .. }) if !cloned.unwritten.is_empty() => start.notice = Some(note),
            Some(Overlay::NewWorkspace { error, .. }) if !cloned.unwritten.is_empty() => *error = Some(note),
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
    if unwritten.is_empty() {
        return format!("cloned on {name}");
    }
    let left = unwritten.iter().map(|file| format!("{} ({})", file.path.display(), file.why));
    format!("cloned on {name} · not copied: {}", run_on::capped(left))
}
