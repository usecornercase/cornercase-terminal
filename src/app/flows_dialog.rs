use std::path::PathBuf;

use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::dialogs::confirm_view;
use super::event::AppEvent;
use super::input::wheel;
use super::overlay::Overlay;
use crate::agents::Known;
use crate::flows::dialog::{Action, Agents, Dialog, Editing, Hit, Place, Room, RowText};
use crate::flows::store::{self, Origin};
use crate::flows::{Flow, builtin};
use crate::log::{self, Job, Level};
use crate::paths;
use crate::project::Hierarchy;
use crate::ui::{self, FlowsHit, SettingsHit};

const LIST_LABEL: usize = 26;
const FORM_LABEL: usize = 16;
const HEADING: &str = "teams of agents that work through a plan, started from an issue or a prompt";

impl App {
    pub(super) fn open_flows(&mut self, flow: Option<(&Origin, &str)>) {
        let project = self.project().map(|p| p.path.clone());
        let installed = builtin::installed(&self.config);
        let agents = Agents::new(installed, Known::of(&self.config.agent), project.is_some());
        let mut dialog = Dialog::new(self.flow_listing(project.as_deref()), agents);
        if let Some(i) = flow.and_then(|(origin, name)| dialog.find(origin, name)) {
            dialog.select(i);
            if origin_editable(flow) {
                dialog.open(i);
            }
        }
        let back = self.overlay.take().map(Box::new);
        self.overlay = Some(Overlay::Flows { dialog: Box::new(dialog), back });
    }

    pub(super) fn flows_note(&self) -> String {
        let project = self.project().map(|p| p.path.clone());
        let listing = self.flow_listing(project.as_deref());
        Dialog::new(listing, Agents::default()).summary()
    }

    fn flows_dialog(&mut self) -> Option<&mut Dialog> {
        match &mut self.overlay {
            Some(Overlay::Flows { dialog, .. }) => Some(dialog.as_mut()),
            _ => None,
        }
    }

    pub(super) fn flows_key(&mut self, key: KeyEvent, area: Rect) {
        let Some(dialog) = self.flows_dialog() else { return };
        let room = room(dialog, area);
        let action = dialog.key(key, room);
        self.flows_act(action);
    }

    pub(super) fn flows_paste(&mut self, text: &str, area: Rect) {
        if let Some(dialog) = self.flows_dialog() {
            let room = room(dialog, area);
            dialog.paste(text, room);
        }
    }

    pub(super) fn close_flows(&mut self) {
        if let Some(dialog) = self.flows_dialog()
            && dialog.close() == Action::Close
        {
            self.overlay = None;
        }
    }

    pub(super) fn flows_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let Some(dialog) = self.flows_dialog() else { return };
        let room = room(dialog, area);
        if let Some((_, _, submit)) = dialog.confirm_text() {
            if ev.kind == MouseEventKind::Down(MouseButton::Left) {
                match ui::form_hit(area, submit, None, pos) {
                    Some(ui::FormHit::Submit) => {
                        let action = dialog.confirmed();
                        self.flows_act(action);
                    }
                    Some(ui::FormHit::Cancel) => dialog.confirm = None,
                    _ => {}
                }
            }
            return;
        }
        if let Some(delta) = wheel(ev.kind) {
            dialog.wheel(delta, room);
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let ui::Overlay::Flows(view) = flows_view(dialog, area) else { return };
        let hit = match ui::flows_hit(area, &view, pos) {
            Some(FlowsHit::Tab(i)) => Hit::Tab(i),
            Some(FlowsHit::Row(SettingsHit::Remove(i))) => Hit::Remove(i),
            Some(FlowsHit::Row(SettingsHit::MoveUp(i))) => Hit::MoveUp(i),
            Some(FlowsHit::Row(SettingsHit::MoveDown(i))) => Hit::MoveDown(i),
            Some(FlowsHit::Row(SettingsHit::Row(i))) => Hit::Row(i),
            Some(FlowsHit::Pick(i)) => Hit::Pick(i),
            Some(FlowsHit::Button(i)) => Hit::Button(view.buttons[i]),
            Some(FlowsHit::Text { line, col }) => Hit::Text { line, col },
            Some(FlowsHit::Row(_)) | None => return,
        };
        let action = dialog.hit(hit, room);
        self.flows_act(action);
    }

    fn flows_act(&mut self, action: Action) {
        match action {
            Action::None => {}
            Action::Close => {
                let Some(Overlay::Flows { dialog, back }) = self.overlay.take() else { return };
                self.overlay = back.map(|back| *back);
                let note = self.flows_note();
                let starting = match &self.overlay {
                    Some(Overlay::StartFlow { start, .. }) => Some(start.project),
                    _ => None,
                };
                let listing = starting.map(|project| self.flow_listing(self.project_path(project).as_deref()));
                match (&mut self.overlay, listing) {
                    (Some(Overlay::Settings(settings)), _) => settings.flows = note,
                    (Some(Overlay::StartFlow { start, .. }), Some(listing)) => {
                        start.set_listing(listing, dialog.last.as_ref());
                    }
                    _ => {}
                }
            }
            Action::Save { flow, place, previous } => {
                let saved = self.flows_dir(place).and_then(|dir| save(&dir, &flow, previous.as_deref()));
                self.flows_saved(saved, Dialog::saved);
            }
            Action::Duplicate(flow) => {
                let saved = save(&paths::flows(&self.config_path), &flow, None);
                self.flows_saved(saved, |dialog, path| {
                    if let Some(i) = dialog.find(&Origin::User(path), "") {
                        dialog.open(i);
                    }
                });
            }
            Action::Delete(path) => {
                let deleted = store::delete(&path);
                if deleted.is_ok() {
                    log::info!("flows", "flow deleted", file = file_name(&path));
                }
                self.refresh_flows();
                if let (Err(e), Some(dialog)) = (deleted, self.flows_dialog()) {
                    dialog.notice = Some(format!("could not delete the flow: {e}"));
                }
            }
            Action::ReadModels(kind) => self.read_models(kind),
        }
    }

    fn project_path(&self, project: u64) -> Option<PathBuf> {
        self.projects.project_index(project).map(|p| self.projects[p].path.clone())
    }

    fn flows_dir(&self, place: Place) -> Result<PathBuf, String> {
        match place {
            Place::Yours => Ok(paths::flows(&self.config_path)),
            Place::Project => {
                self.project().map(|p| store::project_dir(&p.path)).ok_or_else(|| "no project is open".to_string())
            }
        }
    }

    fn flows_saved(&mut self, saved: Result<PathBuf, String>, then: impl FnOnce(&mut Dialog, PathBuf)) {
        if let Ok(path) = &saved {
            log::info!("flows", "flow saved", file = file_name(path));
        }
        self.refresh_flows();
        let Some(dialog) = self.flows_dialog() else { return };
        match saved {
            Ok(path) => then(dialog, path),
            Err(e) => dialog.notice = Some(format!("could not save the flow: {e}")),
        }
    }

    fn refresh_flows(&mut self) {
        let project = self.project().map(|p| p.path.clone());
        let listing = self.flow_listing(project.as_deref());
        if let Some(dialog) = self.flows_dialog() {
            dialog.set_listing(listing);
        }
    }

    fn read_models(&self, kind: Known) {
        let Some(read) = kind.catalogue() else { return };
        self.spawn_job(
            Job::new(Level::Debug, "flows", "models").with("agent", kind.name()),
            move || Ok(read()),
            move |result, _| Some(AppEvent::FlowModels(kind, result.unwrap_or_default())),
        );
    }

    pub(super) fn flow_models(&mut self, kind: Known, ids: Vec<String>) {
        if let Some(dialog) = self.flows_dialog() {
            dialog.models_read(kind, ids);
        }
    }
}

fn origin_editable(flow: Option<(&Origin, &str)>) -> bool {
    flow.is_some_and(|(origin, _)| origin.editable())
}

fn save(dir: &std::path::Path, flow: &Flow, previous: Option<&std::path::Path>) -> Result<PathBuf, String> {
    store::save(dir, flow, previous).map_err(|e| e.to_string())
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn room(dialog: &Dialog, area: Rect) -> Room {
    let warnings = dialog.warnings().len();
    let preview = dialog.edit.as_ref().is_some_and(|edit| edit.text.is_none());
    let text = ui::flows_text(area, warnings, preview);
    Room {
        pick: ui::flows_pick_rows(area, warnings, preview),
        width: usize::from(text.width),
        height: usize::from(text.height),
        wide: ui::flows_wide(area),
    }
}

pub(super) fn settings_row(text: &RowText) -> ui::SettingsRow {
    ui::SettingsRow {
        section: text.section,
        label: text.label.clone(),
        value: text.value.clone(),
        note: text.note.clone(),
        dangerous: text.red,
        removable: text.removable,
        movable: text.movable,
    }
}

pub(super) fn flows_view(dialog: &Dialog, area: Rect) -> ui::Overlay {
    if let Some((title, message, submit)) = dialog.confirm_text() {
        return confirm_view(title, message, submit, None);
    }
    let room = room(dialog, area);
    let edit = dialog.edit.as_deref();
    let texts = edit.map_or_else(|| dialog.list_rows(), |edit| edit.row_texts(&dialog.agents));
    let rows = texts.iter().map(settings_row).collect();
    let pick = edit.and_then(|edit| {
        let pick = edit.pick.as_ref()?;
        let choices = edit.pick_choices();
        let marks = edit.pick_marks();
        let items = choices
            .iter()
            .enumerate()
            .map(|(i, choice)| {
                let label = match marks.as_ref().and_then(|m| m.get(i)) {
                    Some(Some(true)) => format!("[x] {}", choice.label),
                    Some(Some(false)) => format!("[ ] {}", choice.label),
                    Some(None) => format!("    {}", choice.label),
                    None => choice.label.clone(),
                };
                (label, choice.note.clone(), false)
            })
            .collect();
        let selected = choices.len().checked_sub(1).map(|last| pick.search.selected().min(last));
        Some(ui::SettingsPick {
            title: edit.pick_title(),
            filter: pick.search.query().to_string(),
            items,
            selected,
            scroll: pick.search.scroll(),
        })
    });
    let field = edit
        .and_then(|e| e.field.as_ref().map(|f| ui::SettingsEdit { label: e.field_label(), value: f.input.clone() }));
    let heading = edit.filter(|e| e.text.is_some()).map_or_else(|| HEADING.to_string(), Editing::text_heading);
    ui::Overlay::Flows(Box::new(ui::Flows {
        title: dialog.title(),
        heading,
        tabs: edit.map(Editing::tabs).unwrap_or_default(),
        tab: edit.map_or(0, Editing::tab_index),
        rows,
        cursor: edit.map_or(dialog.cursor, |e| e.cursor),
        label: if edit.is_some() { FORM_LABEL } else { LIST_LABEL },
        edit: field,
        pick,
        text: edit.and_then(|e| e.text.as_ref().map(|t| t.editor.clone())),
        warnings: dialog.warnings(),
        note: dialog.error().map(ui::Note::Error),
        hint: dialog.hint(),
        buttons: dialog.buttons(room),
        submit: dialog.submit(),
        preview: edit.filter(|e| e.text.is_none()).map(|e| e.flow.clone()),
        diagram: edit.is_some_and(|e| e.diagram),
    }))
}
