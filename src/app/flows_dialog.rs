use std::path::PathBuf;

use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

use super::App;
use super::dialogs::confirm_view;
use super::event::AppEvent;
use super::input::wheel;
use super::overlay::Overlay;
use crate::agents::Known;
use crate::error::Error;
use crate::flows::dialog::{Action, Agents, Dialog, Editing, Hit, Place, Preview, Room, RowText};
use crate::flows::registry::{self, Source};
use crate::flows::store::{self, Origin};
use crate::flows::{Flow, builtin, share};
use crate::log::{self, Job, Level};
use crate::paths;
use crate::project::Hierarchy;
use crate::settings::Page;
use crate::ui::{self, FlowsHit, SettingsHit};

const LIST_LABEL: usize = 26;
const FORM_LABEL: usize = 16;
const IMPORT_HEADING: &str = "paste a flow, or type owner/repo or an https address";
const HEADING: &str = "teams of agents that work through a plan, started from an issue or a prompt";

impl App {
    fn flows_dialog_for(&self, project: Option<u64>) -> Dialog {
        let project = project.or_else(|| self.project().map(|p| p.id));
        let path = project.and_then(|id| self.project_path(id));
        let installed = builtin::installed(&self.config);
        let agents = Agents::new(installed, Known::of(&self.config.agent), path.is_some());
        let mut dialog = Dialog::new(self.flow_listing(path.as_deref()), agents);
        dialog.project = project;
        dialog
    }

    pub(super) fn open_flows_tab(&mut self) {
        self.settings_page = Page::Flows;
        let mut dialog = self.flows_dialog_for(None);
        dialog.in_settings = true;
        self.overlay = Some(Overlay::Flows { dialog: Box::new(dialog), back: None });
    }

    fn open_settings_page(&mut self, page: Page) {
        if page != Page::Flows {
            self.settings_page = page;
            self.open_settings();
        }
    }

    pub(super) fn open_flows(&mut self, project: Option<u64>, flow: Option<(&Origin, &str)>) {
        let mut dialog = self.flows_dialog_for(project);
        if let Some(i) = flow.and_then(|(origin, name)| dialog.find(origin, name)) {
            dialog.select(i);
            dialog.open(i);
        }
        let back = self.overlay.take().map(Box::new);
        self.overlay = Some(Overlay::Flows { dialog: Box::new(dialog), back });
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
        follow_preview(dialog, area);
        self.flows_act(action);
    }

    pub(super) fn flows_paste(&mut self, text: &str, area: Rect) {
        if let Some(dialog) = self.flows_dialog() {
            let room = room(dialog, area);
            dialog.paste(text, room);
            follow_preview(dialog, area);
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
            let body = preview_body(dialog, area).filter(|body| body.contains(pos));
            let most = body.and_then(|body| match dialog.preview() {
                Some(Preview::Flow(flow)) => Some(ui::flow::max_scroll(&plan(flow), body)),
                _ => None,
            });
            match (body, most) {
                (Some(_), Some(most)) => dialog.scroll_preview(delta, most),
                (Some(_), None) => {}
                _ => dialog.wheel(delta, room),
            }
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
        follow_preview(dialog, area);
        self.flows_act(action);
    }

    fn flows_act(&mut self, action: Action) {
        match action {
            Action::None => {}
            Action::Close => {
                let Some(Overlay::Flows { dialog, back }) = self.overlay.take() else { return };
                self.overlay = back.map(|back| *back);
                let starting = match &self.overlay {
                    Some(Overlay::StartFlow { start, .. }) => Some(start.project),
                    _ => None,
                };
                let listing = starting.map(|project| self.flow_listing(self.project_path(project).as_deref()));
                if let (Some(Overlay::StartFlow { start, .. }), Some(listing)) = (&mut self.overlay, listing) {
                    start.set_listing(listing, dialog.last.as_ref());
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
            Action::Hide { name, file } => self.hide_flow(&name, file.as_deref()),
            Action::Unhide(name) => self.unhide_flow(&name),
            Action::Restore { file, name } => {
                let mut next = self.config.clone();
                store::unhide(&mut next.hidden_flows, &name);
                self.save_then_delete(&next, Some(&file));
                self.refresh_flows();
            }
            Action::Tab(i) => self.open_settings_page(Page::ALL[i]),
            Action::StepTab(delta) => self.open_settings_page(Page::Flows.step(delta)),
            Action::Restart => self.open_restart(),
            Action::Export(flow) => self.flow_exported(&flow),
            Action::ReadModels(kind) => self.read_models(kind),
            Action::Fetch(source) => self.fetch_flow(source),
        }
    }

    fn save_hidden(&mut self, config: &crate::config::Config) -> bool {
        if let Err(e) = self.write_config(config) {
            if let Some(dialog) = self.flows_dialog() {
                dialog.notice = Some(e);
            }
            return false;
        }
        true
    }

    fn save_then_delete(&mut self, next: &crate::config::Config, file: Option<&std::path::Path>) -> bool {
        let previous = self.config.clone();
        let changed = next.hidden_flows != previous.hidden_flows;
        if changed && !self.save_hidden(next) {
            return false;
        }
        let Some(file) = file else { return true };
        match store::delete(file) {
            Ok(()) => {
                log::info!("flows", "flow deleted", file = file_name(file));
                true
            }
            Err(e) => {
                let rolled_back = !changed || self.save_hidden(&previous);
                if let Some(dialog) = self.flows_dialog() {
                    let kept = if rolled_back { "" } else { " (the deleted built-in list could not be put back)" };
                    dialog.notice = Some(format!("could not delete the flow: {e}{kept}"));
                }
                false
            }
        }
    }

    fn hide_flow(&mut self, name: &str, file: Option<&std::path::Path>) {
        let mut next = self.config.clone();
        store::hide(&mut next.hidden_flows, name);
        let saved = self.save_then_delete(&next, file);
        self.refresh_flows();
        if !saved {
            return;
        }
        log::info!("flows", "built-in flow hidden", name = name);
        if let Some(dialog) = self.flows_dialog() {
            dialog.notice = Some(format!("deleted {name}: deleted built-ins, at the end of the list, brings it back"));
        }
    }

    fn unhide_flow(&mut self, name: &str) {
        let mut next = self.config.clone();
        store::unhide(&mut next.hidden_flows, name);
        let saved = self.save_hidden(&next);
        self.refresh_flows();
        if !saved {
            return;
        }
        log::info!("flows", "built-in flow restored", name = name);
        if let Some(dialog) = self.flows_dialog() {
            if let Some(i) = dialog.find(&Origin::BuiltIn, name) {
                dialog.select(i);
            }
            dialog.notice = Some(format!("restored {name}"));
        }
    }

    fn flow_exported(&mut self, flow: &Flow) {
        let notice = match share::to_share(flow) {
            Ok(text) => {
                self.copy(&text);
                log::info!("flows", "flow exported", bytes = text.len());
                let warning = share::env_warning(flow).map(|w| format!(" · {w}")).unwrap_or_default();
                format!(
                    "copied {}: a teammate pastes it in Flows › import, or saves it for `cornercase flow import`{warning}",
                    flow.name.trim()
                )
            }
            Err(e) => e,
        };
        if let Some(dialog) = self.flows_dialog() {
            dialog.notice = Some(notice);
        }
    }

    fn project_path(&self, project: u64) -> Option<PathBuf> {
        self.projects.project_index(project).map(|p| self.projects[p].path.clone())
    }

    fn flows_dir(&self, place: Place) -> Result<PathBuf, String> {
        match place {
            Place::Yours => Ok(paths::flows(&self.config_path)),
            Place::Project => self
                .flows_project()
                .map(|path| store::project_dir(&path))
                .ok_or_else(|| "the project is not open".to_string()),
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

    fn flows_project(&self) -> Option<PathBuf> {
        match &self.overlay {
            Some(Overlay::Flows { dialog, .. }) => dialog.project.and_then(|id| self.project_path(id)),
            _ => None,
        }
    }

    pub(in crate::app) fn refresh_flows(&mut self) {
        let listing = self.flow_listing(self.flows_project().as_deref());
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

    fn fetch_flow(&self, source: Source) {
        let asked = source.clone();
        self.spawn_job(
            Job::new(Level::Info, "flows", "fetch"),
            move || registry::fetch(&asked).map_err(Error::Api),
            move |result, _| Some(AppEvent::FlowFetched(source, result.map_err(|e| e.to_string()))),
        );
    }

    pub(super) fn flow_fetched(&mut self, source: &Source, result: Result<String, String>) {
        if let Some(dialog) = self.flows_dialog() {
            dialog.fetched(source, result);
        }
    }

    pub(super) fn flow_models(&mut self, kind: Known, ids: Vec<String>) {
        if let Some(dialog) = self.flows_dialog() {
            dialog.models_read(kind, ids);
        }
    }
}

fn save(dir: &std::path::Path, flow: &Flow, previous: Option<&std::path::Path>) -> Result<PathBuf, String> {
    store::save(dir, flow, previous).map_err(|e| e.to_string())
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

pub(super) fn plan(flow: &Flow) -> ui::flow::Diagram {
    ui::flow::Diagram::plan(flow, Color::Reset, false)
}

fn preview_body(dialog: &Dialog, area: Rect) -> Option<Rect> {
    dialog.preview()?;
    ui::flows_preview(area, dialog.warnings().len(), true, dialog.diagram()).map(ui::preview_body)
}

fn follow_preview(dialog: &mut Dialog, area: Rect) {
    let Some(body) = preview_body(dialog, area) else { return };
    let Some(edit) = dialog.edit.as_deref_mut() else { return };
    if let Some(stage) = edit.newly_focused() {
        edit.preview = ui::flow::scroll_to(&plan(&edit.flow), body, stage, edit.preview);
    }
}

fn room(dialog: &Dialog, area: Rect) -> Room {
    let warnings = dialog.warnings().len();
    let preview = dialog.preview().is_some();
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
        muted: text.muted,
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
    let heading = match (&dialog.importing, edit.filter(|e| e.text.is_some())) {
        (Some(_), _) => IMPORT_HEADING.to_string(),
        (None, edit) => edit.map_or_else(|| HEADING.to_string(), Editing::text_heading),
    };
    let (tabs, tab) = if dialog.settings_tabs() {
        (Page::ALL.iter().map(|page| page.name().to_string()).collect(), Page::Flows.index())
    } else {
        (edit.map(Editing::tabs).unwrap_or_default(), edit.map_or(0, Editing::tab_index))
    };
    ui::Overlay::Flows(Box::new(ui::Flows {
        title: dialog.title(),
        heading,
        tabs,
        tab,
        rows,
        cursor: edit.map_or(dialog.cursor, |e| e.cursor),
        label: if edit.is_some() { FORM_LABEL } else { LIST_LABEL },
        edit: field,
        pick,
        text: match &dialog.importing {
            Some(importing) => Some(importing.editor.clone()),
            None => edit.and_then(|e| e.text.as_ref().map(|t| t.editor.clone())),
        },
        warnings: dialog.warnings(),
        note: dialog.error().map(ui::Note::Error),
        hint: dialog.hint(),
        buttons: dialog.buttons(room),
        submit: dialog.submit(),
        preview: dialog.preview().map(|preview| match preview {
            Preview::Flow(flow) => ui::FlowPreview::Plan(Box::new(flow.clone())),
            Preview::Note(note) => ui::FlowPreview::Note(note),
        }),
        preview_scroll: dialog.preview_scroll(),
        focus: edit.and_then(Editing::preview_stage),
        diagram: dialog.diagram(),
    }))
}
