use std::path::{Path, PathBuf};

use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

use super::App;
use super::dialogs::confirm_view;
use super::event::AppEvent;
use super::input::wheel;
use super::overlay::Overlay;
use super::text_fields::FieldArea;
use crate::agents::Known;
use crate::editor::Editor;
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

    pub(super) fn flows_text_field(&mut self, area: Rect) -> Option<(FieldArea, &mut Editor)> {
        let dialog = self.flows_dialog()?;
        let text = ui::flows_text(area, dialog.warnings().len(), dialog.preview().is_some());
        let editor = dialog.text_mut()?;
        Some((FieldArea { text, width: usize::from(text.width), top: editor.scroll(), scrolls: true }, editor))
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
        if dialog.confirm_text().is_some() {
            if ev.kind == MouseEventKind::Down(MouseButton::Left)
                && let Some(action) = confirm_click(dialog, area, pos)
            {
                self.flows_act(action);
            }
            return;
        }
        if dialog.drag.is_some() {
            let action = flows_drag(dialog, ev, pos, area);
            self.flows_act(action);
            return;
        }
        if let Some(delta) = wheel(ev.kind) {
            wheel_flows(dialog, area, pos, delta, room);
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let ui::Overlay::Flows(view) = flows_view(dialog, area) else { return };
        let hit = ui::flows_hit(area, &view, pos);
        if grab_row(dialog, hit, area) {
            return;
        }
        let Some(hit) = list_hit(&view, hit) else { return };
        let action = dialog.hit(hit, room);
        follow_preview(dialog, area);
        self.flows_act(action);
    }

    fn flows_act(&mut self, action: Action) {
        match action {
            Action::None => {}
            Action::Close => self.flows_closed(),
            Action::Save { flow, place, previous } => self.flows_save(&flow, place, previous.as_deref()),
            Action::Duplicate(flow) => self.flows_duplicate(&flow),
            Action::Delete(path) => {
                let project = self.flows_project();
                if let (Err(e), Some(dialog)) = (self.delete_flow_file(&path, project.as_deref()), self.flows_dialog())
                {
                    dialog.notice = Some(e);
                }
            }
            Action::Default(name) => self.flows_default(&name),
            Action::Order(names) => {
                let project = self.flows_project();
                if let Err(e) = self.order_flows(&names, project.as_deref()) {
                    self.flows_notice(e);
                }
            }
            Action::Hide { name, file } => self.flows_hide(&name, file.as_deref()),
            Action::Unhide(name) => self.flows_unhide(&name),
            Action::Restore { file, name } => {
                if let Err(e) = self.unhide_builtin(&name, Some(&file)) {
                    self.flows_notice(e);
                }
            }
            Action::Tab(i) => self.open_settings_page(Page::ALL[i]),
            Action::StepTab(delta) => self.open_settings_page(Page::Flows.step(delta)),
            Action::Restart => self.open_restart(),
            Action::Share => self.open_share_settings(),
            Action::Export(flow) => self.flow_exported(&flow),
            Action::ReadModels(kind) => self.read_models(kind),
            Action::Fetch(source) => self.fetch_flow(source),
        }
    }

    fn flows_closed(&mut self) {
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

    fn flows_save(&mut self, flow: &Flow, place: Place, previous: Option<&Path>) {
        let saved = self.flows_dir(place).and_then(|dir| self.save_flow_file(&dir, flow, previous));
        self.flows_saved(saved, Dialog::saved);
    }

    fn flows_duplicate(&mut self, flow: &Flow) {
        let saved = self.save_flow_file(&paths::flows(self.config_file.path()), flow, None);
        self.flows_saved(saved, |dialog, path| {
            if let Some(i) = dialog.find(&Origin::User(path), "") {
                dialog.open(i);
            }
        });
    }

    fn flows_default(&mut self, name: &str) {
        let notice = match self.set_default_flow(Some(name)) {
            Ok(_) => format!("{name} is the default: the start dialog opens on it"),
            Err(e) => e,
        };
        self.flows_notice(notice);
    }

    fn flows_hide(&mut self, name: &str, file: Option<&Path>) {
        match self.hide_builtin(name, file) {
            Ok(()) => {
                if let Some(dialog) = self.flows_dialog() {
                    dialog.notice =
                        Some(format!("deleted {name}: deleted built-ins, at the end of the list, brings it back"));
                }
            }
            Err(e) => self.flows_notice(e),
        }
    }

    fn flows_unhide(&mut self, name: &str) {
        match self.unhide_builtin(name, None) {
            Ok(()) => self.flows_restored(name),
            Err(e) => self.flows_notice(e),
        }
    }

    fn flows_restored(&mut self, name: &str) {
        let Some(dialog) = self.flows_dialog() else { return };
        if let Some(i) = dialog.find(&Origin::BuiltIn, name) {
            dialog.select(i);
        }
        dialog.notice = Some(format!("restored {name}"));
    }

    fn flows_notice(&mut self, notice: String) {
        if let Some(dialog) = self.flows_dialog() {
            dialog.notice = Some(notice);
        }
    }

    pub(in crate::app) fn save_flow_file(
        &mut self,
        dir: &Path,
        flow: &Flow,
        previous: Option<&Path>,
    ) -> Result<PathBuf, String> {
        let old = previous.and_then(|path| store::read(path).ok()).map(|flow| flow.name);
        let was_default = previous.is_some_and(|path| self.is_default_file(path));
        let saved = save(dir, flow, previous);
        if let Ok(path) = &saved {
            log::info!("flows", "flow saved", file = file_name(path));
            if let Some(old) = old.filter(|old| !store::same_name(old, &flow.name)) {
                self.flow_renamed(&old, &flow.name, was_default);
            }
        }
        self.refresh_flows();
        saved
    }

    fn is_default_file(&self, path: &Path) -> bool {
        let projects = self.projects.iter().map(|project| Some(project.path.clone()));
        std::iter::once(None).chain(projects).any(|project| {
            let listing = self.flow_listing(project.as_deref());
            listing.default_flow().is_some_and(|saved| saved.origin.path() == Some(path))
        })
    }

    fn flow_renamed(&mut self, old: &str, new: &str, was_default: bool) {
        let moved = self.change_config(|config| {
            if was_default && store::same_name(&config.default_flow, old) {
                new.trim().clone_into(&mut config.default_flow);
            }
            store::rename(&mut config.flow_order, old, new);
        });
        if let Err(e) = moved {
            log::warning!("flows", "the default flow and the order did not follow a rename", why = e);
        }
    }

    pub(in crate::app) fn delete_flow_file(&mut self, path: &Path, project: Option<&Path>) -> Result<(), String> {
        let was_default =
            self.flow_listing(project).default_flow().is_some_and(|saved| saved.origin.path() == Some(path));
        let deleted = store::delete(path);
        if deleted.is_ok() {
            log::info!("flows", "flow deleted", file = file_name(path));
            if was_default {
                self.clear_default_flow();
            }
        }
        self.refresh_flows();
        deleted.map_err(|e| format!("could not delete the flow: {e}"))
    }

    fn clear_default_flow(&mut self) {
        if let Err(e) = self.change_config(|config| config.default_flow.clear()) {
            log::warning!("flows", "the default flow could not be cleared", why = e);
        }
    }

    pub(in crate::app) fn set_default_flow(&mut self, name: Option<&str>) -> Result<bool, String> {
        let name = name.map(str::trim).unwrap_or_default();
        if self.config.default_flow.trim() == name {
            return Ok(false);
        }
        self.change_config(|config| name.clone_into(&mut config.default_flow))?;
        if name.is_empty() {
            log::info!("flows", "default flow cleared");
        } else {
            log::info!("flows", "default flow set", name = name);
        }
        self.refresh_flows();
        Ok(true)
    }

    pub(in crate::app) fn order_flows(&mut self, wanted: &[String], project: Option<&Path>) -> Result<bool, String> {
        let listed: Vec<String> = self.flow_listing(project).flows.into_iter().map(|saved| saved.flow.name).collect();
        let known = self.known_flow_names();
        let next = store::reorder(&self.config.flow_order, wanted, &listed, |name| {
            known.iter().any(|other| store::same_name(other, name))
        });
        let changed = next != self.config.flow_order;
        if changed {
            let count = next.len();
            self.change_config(|config| config.flow_order = next)?;
            log::info!("flows", "flows ordered", count = count);
        }
        self.refresh_flows();
        Ok(changed)
    }

    fn known_flow_names(&self) -> Vec<String> {
        let all = self.flow_listing(None);
        let mut names = all.builtins;
        names.extend(all.flows.into_iter().map(|saved| saved.flow.name));
        for project in &self.projects {
            let listing = self.flow_listing(Some(&project.path));
            names.extend(
                listing
                    .flows
                    .into_iter()
                    .filter(|saved| matches!(saved.origin, Origin::Project(_)))
                    .map(|saved| saved.flow.name),
            );
        }
        names
    }

    fn save_then_delete(&mut self, next: &crate::config::Config, file: Option<&Path>) -> Result<(), String> {
        let previous = self.config.clone();
        let changed = *next != previous;
        if changed {
            self.write_config(next)?;
        }
        let Some(file) = file else { return Ok(()) };
        match store::delete(file) {
            Ok(()) => {
                log::info!("flows", "flow deleted", file = file_name(file));
                Ok(())
            }
            Err(e) => {
                let rolled_back = !changed || self.write_config(&previous).is_ok();
                let kept = if rolled_back { "" } else { " (the deleted built-in list could not be put back)" };
                Err(format!("could not delete the flow: {e}{kept}"))
            }
        }
    }

    pub(in crate::app) fn hide_builtin(&mut self, name: &str, file: Option<&Path>) -> Result<(), String> {
        let mut next = self.config.clone();
        store::hide(&mut next.hidden_flows, name);
        if store::same_name(&next.default_flow, name) {
            next.default_flow.clear();
        }
        let saved = self.save_then_delete(&next, file);
        self.refresh_flows();
        if saved.is_ok() {
            log::info!("flows", "built-in flow hidden", name = name);
        }
        saved
    }

    pub(in crate::app) fn unhide_builtin(&mut self, name: &str, file: Option<&Path>) -> Result<(), String> {
        let mut next = self.config.clone();
        store::unhide(&mut next.hidden_flows, name);
        let saved = match file {
            Some(file) => self.save_then_delete(&next, Some(file)),
            None => self.write_config(&next),
        };
        self.refresh_flows();
        if saved.is_ok() {
            log::info!("flows", "built-in flow restored", name = name);
        }
        saved
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
            Place::Yours => Ok(paths::flows(self.config_file.path())),
            Place::Project => self
                .flows_project()
                .map(|path| store::project_dir(&path))
                .ok_or_else(|| "the project is not open".to_string()),
        }
    }

    fn flows_saved(&mut self, saved: Result<PathBuf, String>, then: impl FnOnce(&mut Dialog, PathBuf)) {
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

fn save(dir: &Path, flow: &Flow, previous: Option<&Path>) -> Result<PathBuf, String> {
    store::save(dir, flow, previous).map_err(|e| e.to_string())
}

fn file_name(path: &Path) -> String {
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

fn confirm_click(dialog: &mut Dialog, area: Rect, pos: Position) -> Option<Action> {
    let (_, _, submit) = dialog.confirm_text()?;
    match ui::form_hit(area, submit, None, pos) {
        Some(ui::FormHit::Submit) => Some(dialog.confirmed()),
        Some(ui::FormHit::Cancel) => {
            dialog.confirm = None;
            None
        }
        _ => None,
    }
}

fn grab_row(dialog: &mut Dialog, hit: Option<FlowsHit>, area: Rect) -> bool {
    let Some(FlowsHit::Row(SettingsHit::Row(i))) = hit else { return false };
    if !dialog.grabbable(i) {
        return false;
    }
    dialog.grab(i);
    follow_preview(dialog, area);
    true
}

fn flows_drag(dialog: &mut Dialog, ev: MouseEvent, pos: Position, area: Rect) -> Action {
    match ev.kind {
        MouseEventKind::Drag(MouseButton::Left) => {
            if let ui::Overlay::Flows(view) = flows_view(dialog, area) {
                dialog.drag_over(ui::flows_row_at(area, &view, pos));
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            let action = dialog.release();
            follow_preview(dialog, area);
            return action;
        }
        MouseEventKind::Down(_) | MouseEventKind::Up(_) | MouseEventKind::Drag(_) => {
            dialog.cancel_drag();
        }
        _ => {}
    }
    Action::None
}

fn wheel_flows(dialog: &mut Dialog, area: Rect, pos: Position, delta: isize, room: Room) {
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
}

fn list_hit(view: &ui::Flows, hit: Option<FlowsHit>) -> Option<Hit> {
    Some(match hit? {
        FlowsHit::Tab(i) => Hit::Tab(i),
        FlowsHit::Row(SettingsHit::Remove(i)) => Hit::Remove(i),
        FlowsHit::Row(SettingsHit::MoveUp(i)) => Hit::MoveUp(i),
        FlowsHit::Row(SettingsHit::MoveDown(i)) => Hit::MoveDown(i),
        FlowsHit::Row(SettingsHit::Row(i)) => Hit::Row(i),
        FlowsHit::Pick(i) => Hit::Pick(i),
        FlowsHit::Button(i) => Hit::Button(view.buttons[i]),
        FlowsHit::Text { line, col } => Hit::Text { line, col },
        FlowsHit::Row(_) => return None,
    })
}

fn pick_label(marks: Option<&Vec<Option<bool>>>, i: usize, label: &str) -> String {
    match marks.and_then(|m| m.get(i)) {
        Some(Some(true)) => format!("[x] {label}"),
        Some(Some(false)) => format!("[ ] {label}"),
        Some(None) => format!("    {label}"),
        None => label.to_string(),
    }
}

fn pick_view(edit: &Editing) -> Option<ui::SettingsPick> {
    let pick = edit.pick.as_ref()?;
    let choices = edit.pick_choices();
    let marks = edit.pick_marks();
    let items = choices
        .iter()
        .enumerate()
        .map(|(i, choice)| (pick_label(marks.as_ref(), i, &choice.label), choice.note.clone(), false))
        .collect();
    let selected = choices.len().checked_sub(1).map(|last| pick.search.selected().min(last));
    Some(ui::SettingsPick {
        title: edit.pick_title(),
        filter: pick.search.query().to_string(),
        items,
        selected,
        scroll: pick.search.scroll(),
    })
}

fn flows_tabs(dialog: &Dialog, edit: Option<&Editing>) -> (Vec<String>, usize) {
    if dialog.settings_tabs() {
        (Page::ALL.iter().map(|page| page.name().to_string()).collect(), Page::Flows.index())
    } else {
        (edit.map(Editing::tabs).unwrap_or_default(), edit.map_or(0, Editing::tab_index))
    }
}

fn flows_heading(dialog: &Dialog, edit: Option<&Editing>) -> String {
    match (&dialog.importing, edit.filter(|e| e.text.is_some())) {
        (Some(_), _) => IMPORT_HEADING.to_string(),
        (None, edit) => edit.map_or_else(|| HEADING.to_string(), Editing::text_heading),
    }
}

fn flows_text(dialog: &Dialog, edit: Option<&Editing>) -> Option<Editor> {
    match &dialog.importing {
        Some(importing) => Some(importing.editor.clone()),
        None => edit.and_then(|e| e.text.as_ref().map(|t| t.editor.clone())),
    }
}

pub(super) fn flows_view(dialog: &Dialog, area: Rect) -> ui::Overlay {
    if let Some((title, message, submit)) = dialog.confirm_text() {
        return confirm_view(title, message, submit, None);
    }
    let room = room(dialog, area);
    let edit = dialog.edit.as_deref();
    let texts = edit.map_or_else(|| dialog.list_rows(), |edit| edit.row_texts(&dialog.agents));
    let field = edit
        .and_then(|e| e.field.as_ref().map(|f| ui::SettingsEdit { label: e.field_label(), value: f.input.clone() }));
    let (tabs, tab) = flows_tabs(dialog, edit);
    ui::Overlay::Flows(Box::new(ui::Flows {
        title: dialog.title(),
        heading: flows_heading(dialog, edit),
        tabs,
        tab,
        rows: texts.iter().map(settings_row).collect(),
        cursor: edit.map_or(dialog.cursor, |e| e.cursor),
        label: if edit.is_some() { FORM_LABEL } else { LIST_LABEL },
        edit: field,
        pick: edit.and_then(pick_view),
        text: flows_text(dialog, edit),
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
        landing: dialog.drag.filter(|drag| drag.moved).and_then(|drag| drag.landing),
    }))
}
