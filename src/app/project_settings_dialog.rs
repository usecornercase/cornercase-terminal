use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::input::wheel;
use super::overlay::Overlay;
use crate::config;
use crate::project::Hierarchy;
use crate::project::settings::{Action, Field, Form, Hit, Room};
use crate::ui::{self, ProjectSettingsHit};

impl App {
    pub(super) fn open_project_settings(&mut self, project: u64) {
        let Some(p) = self.projects.project_index(project) else { return };
        let saved = self.config.project(&self.projects[p].path, self.home.as_deref());
        let form = Form::new(project, self.project_label(&self.projects[p]), &saved);
        self.nav = None;
        self.overlay = Some(Overlay::ProjectSettings(Box::new(form)));
    }

    fn settings_form(&mut self) -> Option<&mut Form> {
        match &mut self.overlay {
            Some(Overlay::ProjectSettings(form)) => Some(form),
            _ => None,
        }
    }

    pub(super) fn project_settings_key(&mut self, key: KeyEvent, area: Rect) {
        let room = room(area);
        let Some(form) = self.settings_form() else { return };
        let action = form.key(key, room);
        self.project_settings_act(action);
    }

    pub(super) fn project_settings_paste(&mut self, text: &str, area: Rect) {
        let room = room(area);
        if let Some(form) = self.settings_form() {
            form.paste(text, room);
        }
    }

    pub(super) fn project_settings_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let room = room(area);
        let Some(form) = self.settings_form() else { return };
        if let Some(delta) = wheel(ev.kind) {
            for field in [Field::Setup, Field::Teardown] {
                if ui::project_settings_field(area, field).contains(pos) {
                    form.scroll(field, delta, room);
                }
            }
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let view = project_settings_view(form);
        let hit = match ui::project_settings_hit(area, &view, pos) {
            Some(ProjectSettingsHit::Field { field, line, col }) => Hit::Field { field, line, col },
            Some(ProjectSettingsHit::Button(i)) => Hit::Button(view.buttons[i]),
            None => return,
        };
        let action = form.hit(hit, room);
        self.project_settings_act(action);
    }

    fn project_settings_act(&mut self, action: Action) {
        match action {
            Action::None => {}
            Action::Close => self.overlay = None,
            Action::Save(settings) => {
                let Some(project) = self.settings_form().map(|form| form.project) else { return };
                let Some(p) = self.projects.project_index(project) else {
                    self.overlay = None;
                    return;
                };
                let mut next = self.config.clone();
                next.set_project(&self.projects[p].path, self.home.as_deref(), settings);
                match config::save(&self.config_path, &next) {
                    Ok(()) => {
                        self.set_config(next);
                        self.overlay = None;
                    }
                    Err(e) => {
                        if let Some(form) = self.settings_form() {
                            form.notice = Some(format!("failed to save the settings: {e}"));
                        }
                    }
                }
            }
        }
    }
}

fn room(area: Rect) -> Room {
    let setup = ui::project_settings_field(area, Field::Setup);
    let teardown = ui::project_settings_field(area, Field::Teardown);
    Room {
        setup_width: usize::from(setup.width),
        setup_rows: usize::from(setup.height),
        teardown_width: usize::from(teardown.width),
        teardown_rows: usize::from(teardown.height),
    }
}

fn project_settings_view(form: &Form) -> ui::ProjectSettings {
    ui::ProjectSettings {
        title: form.title(),
        setup: form.setup.clone(),
        teardown: form.teardown.clone(),
        focus: form.focus,
        note: form.notice.clone().map(ui::Note::Error),
        hint: form.hint().to_string(),
        buttons: Form::buttons(),
    }
}

pub(super) fn project_settings_overlay(form: &Form) -> ui::Overlay {
    ui::Overlay::ProjectSettings(Box::new(project_settings_view(form)))
}
