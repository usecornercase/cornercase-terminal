use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::super::App;
use super::super::dialogs::confirm_view;
use super::super::event::AppEvent;
use super::super::input::wheel;
use super::super::text_fields::FieldArea;
use super::{FORM_LABEL, LIST_LABEL, settings_row};
use crate::control::persona::{PersonaImport, PersonaSave, PersonaText};
use crate::editor::Editor;
use crate::flows::dialog::personas::{self, Fetched, Offered, Saving};
use crate::flows::dialog::{Dialog, Page, PageAction, Room};
use crate::flows::persona::source::{self, Gathered, Hosts};
use crate::flows::persona::{self as library, imported};
use crate::flows::share::visible;
use crate::flows::store::same_name;
use crate::log::{self, Job, Level};
use crate::ui::{self, FlowsHit, SettingsHit};

const HEADING: &str = "personas: how a role's agent works, picked in a role's persona row";

impl App {
    pub(super) fn open_personas(&mut self) {
        let listing = library::list(&self.personas_dir());
        let Some(dialog) = self.flows_dialog() else { return };
        let followers = personas::followers(&flow_list(dialog));
        dialog.personas = Some(Box::new(Page::new(listing, followers)));
    }

    pub(in crate::app) fn refresh_personas_page(&mut self) {
        let listing = library::list(&self.personas_dir());
        let Some(dialog) = self.flows_dialog() else { return };
        let followers = personas::followers(&flow_list(dialog));
        if let Some(page) = &mut dialog.personas {
            page.set_listing(listing, followers);
        }
    }

    pub(super) fn personas_key(&mut self, key: KeyEvent, area: Rect) {
        let Some(page) = self.persona_page() else { return };
        let action = page.key(key, page_room(page, area));
        self.personas_act(action);
    }

    pub(super) fn personas_paste(&mut self, text: &str, area: Rect) {
        let Some(page) = self.persona_page() else { return };
        let room = page_room(page, area);
        if let Some(form) = &mut page.form {
            form.paste(text, room);
        } else if let Some(catalogue) = &mut page.catalogue {
            catalogue.paste(text);
        }
    }

    pub(super) fn personas_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let Some(page) = self.persona_page() else { return };
        let room = page_room(page, area);
        if let Some(delta) = wheel(ev.kind) {
            page.wheel(delta, room);
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        if let Some((_, _, submit)) = page.ask_text() {
            let action = match ui::form_hit(area, submit, None, pos) {
                Some(ui::FormHit::Submit) => page.answered(true),
                Some(ui::FormHit::Cancel) => page.answered(false),
                _ => PageAction::None,
            };
            self.personas_act(action);
            return;
        }
        let ui::Overlay::Flows(view) = page_view(page) else { return };
        let action = match ui::flows_hit(area, &view, pos) {
            Some(FlowsHit::Button(i)) => page.press(view.buttons[i]),
            Some(FlowsHit::Row(SettingsHit::Row(i))) => page.activate(i),
            Some(FlowsHit::Text { line, col }) => {
                place(page, room, line, col);
                PageAction::None
            }
            _ => PageAction::None,
        };
        self.personas_act(action);
    }

    pub(super) fn personas_text_field(&mut self, area: Rect) -> Option<(FieldArea, &mut Editor)> {
        let page = self.persona_page()?;
        if page.ask.is_some() {
            return None;
        }
        let text = ui::flows_text(area, page.warnings().len(), false);
        let editor = page.form.as_mut()?.text.as_mut()?;
        Some((FieldArea { text, width: usize::from(text.width), top: editor.scroll(), scrolls: true }, editor))
    }

    fn persona_page(&mut self) -> Option<&mut Page> {
        self.flows_dialog()?.personas.as_deref_mut()
    }

    fn personas_act(&mut self, action: PageAction) {
        match action {
            PageAction::None => {}
            PageAction::Back => {
                if let Some(dialog) = self.flows_dialog() {
                    dialog.personas = None;
                }
            }
            PageAction::Save(saving) => self.persona_saved(saving),
            PageAction::Remove(id) => self.persona_removed(&id),
            PageAction::Fetch(source) => self.fetch_personas(source),
            PageAction::Import(offered) => self.personas_imported(offered),
        }
    }

    fn persona_saved(&mut self, saving: Saving) {
        let save = PersonaSave { id: saving.id.unwrap_or_default(), text: saving.text, replace: saving.replace };
        let saved = self.save_persona(&save);
        self.refresh_personas_page();
        let Some(page) = self.persona_page() else { return };
        match saved {
            Ok(info) => page.saved(&info.name),
            Err(refusal) => page.refused(refusal.message),
        }
    }

    fn persona_removed(&mut self, id: &str) {
        let removed = library::remove(&self.personas_dir(), id);
        if removed.is_ok() {
            log::info!("flows", "persona removed", persona = id);
        }
        self.refresh_flows();
        self.refresh_personas_page();
        let Some(page) = self.persona_page() else { return };
        page.notice = Some(match removed {
            Ok(_) => format!("removed {id}"),
            Err(e) => visible(&e.to_string()),
        });
    }

    fn fetch_personas(&self, typed: String) {
        let asked = typed.clone();
        let hosts = self.runs.persona_hosts.clone();
        self.spawn_job(
            Job::new(Level::Info, "flows", "personas fetch"),
            move || Ok(gathered(&asked, &hosts)),
            move |result, _| {
                let result = result.map_err(|e: crate::error::Error| e.to_string()).and_then(|gathered| gathered);
                Some(AppEvent::PersonasFetched(typed, result))
            },
        );
    }

    pub(in crate::app) fn personas_fetched(&mut self, typed: &str, result: Result<Gathered, String>) {
        let dir = self.personas_dir();
        let mine = library::list(&dir).personas;
        let label = source::source(typed, source::on_disk).ok();
        let fetched = result.map(|gathered| Fetched {
            label: label.as_ref().map_or_else(|| typed.to_string(), source::Source::label),
            page: label.as_ref().map_or_else(|| typed.to_string(), source::Source::page),
            web: label.as_ref().is_some_and(source::Source::is_web),
            divisions: gathered
                .divisions
                .into_iter()
                .map(|(key, label)| personas::Division { key, label: visible(&label) })
                .collect(),
            skipped: gathered.skipped,
            offered: gathered.found.into_iter().map(|found| offer(found, &dir, &mine)).collect(),
        });
        let Some(page) = self.persona_page() else { return };
        if let Some(catalogue) = &mut page.catalogue {
            catalogue.loaded(typed, fetched.map_err(|e| visible(&e)));
        }
    }

    fn personas_imported(&mut self, offered: Vec<Offered>) {
        let replace = offered.iter().any(|offered| offered.replaces);
        let texts = offered
            .into_iter()
            .map(|offered| PersonaText { file: offered.file, text: offered.text, source: offered.source })
            .collect();
        let import = PersonaImport { personas: texts, replace, check: false };
        let result = self.import_personas(&import);
        self.refresh_personas_page();
        let Some(page) = self.persona_page() else { return };
        match result {
            Ok(answer) => page.imported(answer.personas.len()),
            Err(refusal) => page.refused(visible(&refusal.message)),
        }
    }
}

fn flow_list(dialog: &Dialog) -> Vec<crate::flows::Flow> {
    dialog.flows.iter().map(|saved| saved.flow.clone()).collect()
}

fn gathered(typed: &str, hosts: &Hosts) -> Result<Gathered, String> {
    let found = source::source(typed, source::on_disk)?;
    if found == source::Source::Stdin {
        return Err("`-` is standard input, which the window has none of: type a folder, a file or a repository".into());
    }
    source::gather(&found, hosts)
}

fn offer(found: source::Found, dir: &std::path::Path, mine: &[library::Persona]) -> Offered {
    let read = imported(&found.file, &found.text, found.source.as_deref());
    let mut offered =
        Offered { file: found.file, source: found.source, division: found.division, ..Offered::default() };
    match read {
        Ok(persona) => {
            offered.replaces = library::taken(dir, &persona.id);
            offered.problem = mine
                .iter()
                .find(|other| other.id != persona.id && same_name(&other.name, &persona.name))
                .map(|other| format!("your persona {} has this name", other.id));
            offered.id = persona.id;
            offered.name = visible(&persona.name);
            offered.emoji = visible(&persona.emoji);
            offered.description = visible(&persona.description);
            offered.text = found.text;
        }
        Err(e) => {
            offered.name = visible(&offered.file);
            offered.problem = Some(visible(&e));
            offered.text = found.text;
        }
    }
    offered
}

fn place(page: &mut Page, room: Room, line: usize, col: usize) {
    if let Some(editor) = page.form.as_mut().and_then(|form| form.text.as_mut()) {
        let scroll = editor.scroll();
        editor.place(room.width, scroll + line, col);
    }
}

fn page_room(page: &Page, area: Rect) -> Room {
    let warnings = page.warnings().len();
    let text = ui::flows_text(area, warnings, false);
    Room {
        pick: ui::flows_pick_rows(area, warnings, false),
        width: usize::from(text.width),
        height: usize::from(text.height),
        wide: ui::flows_wide(area),
    }
}

fn heading(page: &Page) -> String {
    match (&page.form, &page.catalogue) {
        (Some(form), _) if form.text.is_some() => form.heading(),
        (Some(form), _) => format!("{}: a persona of yours, saved in personas/ next to config.json", form.title()),
        (None, Some(catalogue)) => catalogue.heading(),
        (None, None) => HEADING.into(),
    }
}

fn field(page: &Page) -> Option<ui::SettingsEdit> {
    if let Some((label, value)) = page.form.as_ref().and_then(|form| form.field_label()) {
        return Some(ui::SettingsEdit { label: label.into(), value });
    }
    let catalogue = page.catalogue.as_ref().filter(|catalogue| catalogue.editing_source())?;
    Some(ui::SettingsEdit { label: "source".into(), value: catalogue.source.clone() })
}

pub(in crate::app) fn page_view(page: &Page) -> ui::Overlay {
    if let Some((title, message, submit)) = page.ask_text() {
        return confirm_view(title, message, submit, None);
    }
    let form = page.form.as_ref();
    let text = form.and_then(|form| form.text.clone());
    let preview = if text.is_some() { None } else { page.note().map(ui::FlowPreview::Persona) };
    ui::Overlay::Flows(Box::new(ui::Flows {
        title: page.title(),
        heading: heading(page),
        search: false,
        tabs: Vec::new(),
        tab: 0,
        rows: page.rows().iter().map(settings_row).collect(),
        cursor: page.cursor(),
        label: if form.is_some() { FORM_LABEL } else { LIST_LABEL },
        edit: field(page),
        pick: None,
        text,
        warnings: page.warnings(),
        note: page.error().map(ui::Note::Error),
        hint: page.hint(),
        buttons: page.buttons(),
        submit: page.submit(),
        preview,
        preview_scroll: 0,
        personas: Vec::new(),
        focus: None,
        diagram: false,
        landing: None,
    }))
}
