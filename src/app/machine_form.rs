use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::overlay::Overlay;
use crate::editor::Editor;
use crate::list::COMBO_ROWS;
use crate::log;
use crate::machines::Machine;
use crate::machines::form::{Field, Form, Hit, Step};
use crate::settings::{self, Row};
use crate::ui;

pub(super) const CHANGED: &str = "this machine changed outside the form: go back and open it again";

fn width_of(area: Rect, field: Field) -> usize {
    usize::from(ui::machine_form::text(area, field).width.max(1))
}

impl App {
    pub(super) fn machine_action(&mut self, action: &settings::Action) {
        match *action {
            settings::Action::AddMachine => self.open_machine_form(None),
            settings::Action::EditMachine(at) => self.open_machine_form(Some(at)),
            settings::Action::RemoveMachine(at) => self.remove_machine_row(at),
            settings::Action::UpdateMachine(at) => {
                self.follow_machines();
                if let Some(name) = self.config.machines.get(at).map(|machine| machine.name.clone()) {
                    self.update_machine(&name);
                }
            }
            _ => {}
        }
    }

    fn open_machine_form(&mut self, at: Option<usize>) {
        let form = at.and_then(|at| self.config.machines.get(at)).map_or_else(Form::new, Form::editing);
        self.overlay = Some(Overlay::Machine(Box::new(form)));
    }

    fn remove_machine_row(&mut self, at: usize) {
        let Some(gone) = self.config.machines.get(at).cloned() else { return };
        let mut found = false;
        let removed = self.change_config(|config| {
            if let Some(at) = config.machines.iter().position(|machine| *machine == gone) {
                config.machines.remove(at);
                found = true;
            }
        });
        let notice = match removed {
            Err(why) => why,
            Ok(()) if !found => format!("{} changed outside the settings: nothing was removed", gone.name),
            Ok(()) => {
                log::info!("machines", "machine removed", name = gone.name);
                format!("{} removed: its server keeps running there", gone.name)
            }
        };
        self.settings_message(notice);
    }

    fn machine_form(&mut self) -> Option<&mut Form> {
        match &mut self.overlay {
            Some(Overlay::Machine(form)) => Some(form),
            _ => None,
        }
    }

    pub(super) fn machine_form_key(&mut self, key: KeyEvent, area: Rect) {
        let Some(form) = self.machine_form() else { return };
        let width = width_of(area, form.focus);
        let rows = ui::machine_form::list(area, form).map_or(COMBO_ROWS, ui::combo::rows);
        let step = form.key(key, width, rows);
        self.machine_step(step);
    }

    pub(super) fn machine_form_paste(&mut self, text: &str, area: Rect) {
        if let Some(form) = self.machine_form() {
            let width = width_of(area, form.focus);
            form.paste(text, width);
        }
    }

    pub(super) fn machine_form_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let Some(form) = self.machine_form() else { return };
        if let Some(delta) = super::input::wheel(ev.kind) {
            if let Some(rows) = ui::machine_form::list(area, form).map(ui::combo::rows) {
                form.icon.scroll_by(delta, rows);
            }
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let Some(hit) = ui::machine_form::hit(area, form, pos) else { return };
        let width = match hit {
            Hit::Field { field, .. } => width_of(area, field),
            _ => 1,
        };
        let step = form.hit(hit, width);
        self.machine_step(step);
    }

    pub(super) fn machine_text(
        &mut self,
        area: Rect,
        field: Field,
    ) -> Option<(super::text_fields::FieldArea, &mut Editor)> {
        let text = ui::machine_form::text(area, field);
        let editor = self.machine_form()?.editor(field)?;
        let width = usize::from(text.width.max(1));
        Some((super::text_fields::FieldArea { text, width, top: editor.scroll(), scrolls: true }, editor))
    }

    fn machine_step(&mut self, step: Step) {
        match step {
            Step::None => {}
            Step::Back => {
                let was = self.machine_form().and_then(|form| form.was.clone());
                let row = self.machine_row_of(was.as_ref());
                self.back_to_machines(&row, None);
            }
            Step::Submit => self.save_machine(),
        }
    }

    fn machine_row_of(&self, machine: Option<&Machine>) -> Row {
        let at = machine.and_then(|machine| self.config.machines.iter().position(|listed| listed == machine));
        at.map_or(Row::AddMachine, Row::Machine)
    }

    fn back_to_machines(&mut self, row: &Row, notice: Option<String>) {
        self.open_settings_on(row);
        if let (Some(notice), Some(Overlay::Settings(settings))) = (notice, &mut self.overlay) {
            settings.notice = Some(notice);
        }
    }

    fn save_machine(&mut self) {
        let Some(mut form) = self.machine_form().cloned() else { return };
        let mut checked = None;
        let written = self.change_config(|config| {
            if form.was.as_ref().is_some_and(|was| !config.machines.contains(was)) {
                form.notice = Some(CHANGED.into());
                return;
            }
            let Some((at, machine)) = form.checked(&config.machines) else { return };
            checked = Some((machine.clone(), at.is_none()));
            match at {
                Some(at) => config.machines[at] = machine,
                None => config.machines.push(machine),
            }
        });
        if let Err(why) = written {
            form.notice = Some(why);
            checked = None;
        }
        let Some((saved, added)) = checked else {
            if let Some(shown) = self.machine_form() {
                *shown = form;
            }
            return;
        };
        log::info!("machines", "machine set", name = saved.name, new = added);
        let notice = format!("{} {}", saved.name, if added { "added" } else { "saved" });
        let row = self.machine_row_of(Some(&saved));
        self.back_to_machines(&row, Some(notice));
    }
}
