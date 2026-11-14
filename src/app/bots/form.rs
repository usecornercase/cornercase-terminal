use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use crate::accounts::AgentAccounts;
use crate::app::App;
use crate::app::input::wheel;
use crate::app::overlay::Overlay;
use crate::app::text_fields::FieldArea;
use crate::bots::folder;
use crate::bots::form::{Field, Form, Step};
use crate::control::bots::{EditBot, NewBot};
use crate::editor::Editor;
use crate::list::{COMBO_ROWS, Combo};
use crate::log;
use crate::ui::{self, bot_form};

enum Request {
    New(NewBot),
    Edit(EditBot),
}

impl App {
    pub(in crate::app) fn open_bot_form(&mut self, editing: Option<u64>) {
        let agents = self.bot_agent_infos();
        let preferred = self.config.agent.trim().to_string();
        let found = editing.and_then(|key| self.panels.bots.by_key(key));
        let (form, saved) = match found {
            Some(at) => {
                let live = &self.panels.bots.live[at];
                let (about, _) = folder::notes(&live.folder);
                (Form::editing(agents, &live.bot, &about), live.bot.account.clone())
            }
            None => (Form::new(agents, &preferred), None),
        };
        self.screen.nav = None;
        self.overlay = Some(Overlay::BotForm(Box::new(form)));
        self.bot_accounts(saved.as_deref(), found.is_some());
    }

    fn bot_accounts(&mut self, saved: Option<&str>, editing: bool) {
        let Some(Overlay::BotForm(form)) = &self.overlay else { return };
        let kind = form.info().map(|info| info.agent.clone()).unwrap_or_default();
        let account = self.account_choices().and_then(|choices| {
            let agent = choices.of(&kind)?;
            let active = if editing { "" } else { agent.active.as_str() };
            Some(AgentAccounts::new(kind.clone(), &agent.names, agent.labels.clone(), active).keeping(saved))
        });
        if let Some(Overlay::BotForm(form)) = &mut self.overlay {
            form.account = account;
        }
    }

    pub(in crate::app) fn bot_form_key(&mut self, key: KeyEvent, area: Rect) {
        let Some(Overlay::BotForm(form)) = &mut self.overlay else { return };
        let width = usize::from(bot_form::text(area, form, form.focus).width.max(1));
        let rows = bot_form::list(area, form).map_or(COMBO_ROWS, ui::combo::rows);
        match form.key(key, width, rows) {
            Step::Submit => self.submit_bot_form(),
            Step::Cancel => self.overlay = None,
            Step::Chose(field) => self.bot_chose(field),
            Step::None => {}
        }
    }

    fn bot_chose(&mut self, field: Field) {
        if field == Field::Agent {
            self.bot_accounts(None, false);
        }
    }

    pub(in crate::app) fn bot_form_paste(&mut self, text: &str, area: Rect) {
        let Some(Overlay::BotForm(form)) = &mut self.overlay else { return };
        let (focus, idle) = (form.focus, form.opened().is_none());
        let width = usize::from(bot_form::text(area, form, focus).width.max(1));
        let Some(editor) = form.editor(focus).filter(|_| idle) else { return };
        if focus == Field::About {
            editor.insert_lines(text);
            editor.reveal(width, 4);
        } else {
            editor.insert(&text.replace(['\n', '\r'], " "));
        }
        form.error = None;
    }

    pub(in crate::app) fn bot_form_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let Some(Overlay::BotForm(form)) = &mut self.overlay else { return };
        if let Some(delta) = wheel(ev.kind) {
            let rows = bot_form::list(area, form).map(ui::combo::rows);
            if let (Some(open), Some(rows)) = (form.opened(), rows) {
                form.combo(open).into_iter().for_each(|combo| combo.scroll_by(delta, rows));
            }
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        match bot_form::hit(area, form, pos) {
            Some(bot_form::Hit::Cancel) => self.overlay = None,
            Some(bot_form::Hit::Submit) => self.submit_bot_form(),
            Some(bot_form::Hit::Close) | None => form.close_lists(),
            Some(bot_form::Hit::Choose(i)) => {
                if let Some(open) = choose_in_list(form, i) {
                    self.bot_chose(open);
                }
            }
            Some(bot_form::Hit::Field(field, row, col)) => press_field(form, area, (field, row, col)),
        }
    }

    pub(in crate::app) fn bot_text(&mut self, area: Rect, field: Field) -> Option<(FieldArea, &mut Editor)> {
        let Some(Overlay::BotForm(form)) = &mut self.overlay else { return None };
        let text = bot_form::text(area, form, field);
        let editor = form.editor(field)?;
        let top = ui::first_line(editor, text);
        Some((FieldArea { text, width: usize::from(text.width.max(1)), top, scrolls: field == Field::About }, editor))
    }

    fn form_request(&self) -> Option<Result<Request, String>> {
        let Some(Overlay::BotForm(form)) = &self.overlay else { return None };
        Some(if form.editing.is_some() { form.edit_bot().map(Request::Edit) } else { form.new_bot().map(Request::New) })
    }

    fn bot_form_failed(&mut self, message: String) {
        if let Some(Overlay::BotForm(form)) = &mut self.overlay {
            form.error = Some(message);
        }
    }

    fn submit_bot_form(&mut self) {
        match self.form_request() {
            Some(Ok(Request::New(new))) => self.submit_new_bot(&new),
            Some(Ok(Request::Edit(edit))) => self.submit_edit_bot(&edit),
            Some(Err(message)) => self.bot_form_failed(message),
            None => {}
        }
    }

    fn submit_new_bot(&mut self, new: &NewBot) {
        log::info!("bots", "new from the window", name = new.name, about_bytes = new.about.len());
        match self.bot_new(new) {
            Ok(at) => {
                self.overlay = None;
                self.show_bot(at);
            }
            Err(refusal) => self.bot_form_failed(refusal.message),
        }
    }

    fn submit_edit_bot(&mut self, edit: &EditBot) {
        log::info!("bots", "edit from the window", bot = edit.bot);
        match self.bot_edit(edit) {
            Ok(_) => self.overlay = None,
            Err(refusal) => self.bot_form_failed(refusal.message),
        }
    }
}

fn choose_in_list(form: &mut Form, i: usize) -> Option<Field> {
    let open = form.opened()?;
    let changed = form.combo(open).is_some_and(|combo| combo.pick(i));
    if !changed {
        return None;
    }
    form.chose(open);
    Some(open)
}

fn press_field(form: &mut Form, area: Rect, (field, row, col): (Field, usize, usize)) {
    let r = bot_form::text(area, form, field);
    let opening = form.combo_of(field).is_some_and(|combo| !combo.is_open());
    let grey = field == Field::Agent && form.editing.is_some();
    form.focus_on(field);
    if let Some(editor) = form.editor(field) {
        let line = ui::first_line(editor, r) + row;
        editor.place(usize::from(r.width.max(1)), line, col);
    } else if opening && !grey {
        form.combo(field).into_iter().for_each(Combo::open);
    }
}
