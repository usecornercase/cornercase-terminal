use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::Saving;
use crate::editor::Editor;
use crate::flows::dialog::{CANCEL, Room, RowText, SAVE};
use crate::flows::persona::Persona;
use crate::flows::persona::draft::{self, Draft};
use crate::text::one_line;

const ROWS: [FormRow; 4] = [FormRow::Name, FormRow::Description, FormRow::Emoji, FormRow::Instructions];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FormRow {
    Name,
    Description,
    Emoji,
    Instructions,
}

impl FormRow {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Description => "description",
            Self::Emoji => "emoji",
            Self::Instructions => "instructions",
        }
    }
}

pub(crate) enum Pressed {
    Save(Saving),
    Leave,
    Stay,
}

#[derive(Debug)]
pub(crate) struct Form {
    pub(crate) id: Option<String>,
    base: Option<String>,
    pub(crate) draft: Draft,
    original: Draft,
    pub(crate) cursor: usize,
    pub(crate) field: Option<(FormRow, String)>,
    pub(crate) text: Option<Editor>,
    pub(crate) error: Option<String>,
}

impl Form {
    pub(crate) fn new(persona: Option<&Persona>) -> Self {
        let draft = persona.map_or_else(Draft::default, |persona| Draft {
            name: persona.name.clone(),
            description: persona.description.clone(),
            emoji: persona.emoji.clone(),
            body: draft::body(&persona.text).trim_end().to_string(),
        });
        Self {
            id: persona.map(|persona| persona.id.clone()),
            base: persona.map(|persona| persona.text.clone()),
            original: draft.clone(),
            draft,
            cursor: 0,
            field: None,
            text: None,
            error: None,
        }
    }

    pub(crate) fn title(&self) -> String {
        match (self.draft.name.trim(), &self.id) {
            ("", None) => "new".into(),
            ("", Some(id)) => id.clone(),
            (name, _) => name.to_string(),
        }
    }

    pub(crate) fn dirty(&self) -> bool {
        self.draft != self.original
            || self.field.is_some()
            || self.text.as_ref().is_some_and(|text| text.text() != self.draft.body)
    }

    pub(crate) fn rows(&self) -> Vec<RowText> {
        let id = match &self.id {
            Some(id) => format!("id {id}: flows name it"),
            None => "its id comes from the name".into(),
        };
        let row = |row: FormRow, value: String, note: String| RowText {
            label: row.label().into(),
            value,
            note,
            ..RowText::default()
        };
        vec![
            row(FormRow::Name, self.draft.name.clone(), id),
            row(FormRow::Description, self.draft.description.clone(), "one line on what it is for".into()),
            row(FormRow::Emoji, self.draft.emoji.clone(), "shown before its name".into()),
            row(FormRow::Instructions, one_line(&self.draft.body), "✎ how the agent works".into()),
        ]
    }

    pub(crate) fn field_label(&self) -> Option<(&'static str, String)> {
        self.field.as_ref().map(|(row, input)| (row.label(), input.clone()))
    }

    pub(crate) fn heading(&self) -> String {
        format!("{} › instructions: what the agent should do and how", self.title())
    }

    pub(crate) fn hint(&self) -> String {
        if self.text.is_some() {
            "enter starts a new line · save keeps the instructions · esc cancels"
        } else if self.field.is_some() {
            "enter keeps it · esc cancels"
        } else {
            "enter or a click changes a row · save writes the persona · cancel goes back"
        }
        .into()
    }

    pub(crate) fn select(&mut self, i: usize) {
        if i < ROWS.len() {
            self.cursor = i;
        }
    }

    pub(crate) fn activate(&mut self) {
        self.error = None;
        let row = ROWS[self.cursor.min(ROWS.len() - 1)];
        let value = match row {
            FormRow::Name => self.draft.name.clone(),
            FormRow::Description => self.draft.description.clone(),
            FormRow::Emoji => self.draft.emoji.clone(),
            FormRow::Instructions => {
                let mut editor = Editor::new(&self.draft.body);
                editor.home();
                self.text = Some(editor);
                return;
            }
        };
        self.field = Some((row, value));
    }

    fn keep(&mut self) {
        if let Some(editor) = self.text.take() {
            self.draft.body = editor.text().trim_end().to_string();
        }
        let Some((row, input)) = self.field.take() else { return };
        let value = input.trim().to_string();
        match row {
            FormRow::Name => self.draft.name = value,
            FormRow::Description => self.draft.description = value,
            FormRow::Emoji => self.draft.emoji = value,
            FormRow::Instructions => {}
        }
    }

    fn drop_open(&mut self) -> bool {
        self.text.take().is_some() | self.field.take().is_some()
    }

    fn save(&mut self) -> Pressed {
        if self.draft.name.trim().is_empty() {
            self.error = Some("a persona needs a name".into());
            return Pressed::Stay;
        }
        if self.draft.body.trim().is_empty() {
            self.error = Some("write its instructions: what the agent should do and how".into());
            return Pressed::Stay;
        }
        let text = draft::compose(&self.draft, self.base.as_deref());
        Pressed::Save(Saving { id: self.id.clone(), text, replace: self.id.is_some() })
    }

    pub(crate) fn press(&mut self, label: &str) -> Pressed {
        self.error = None;
        let open = self.field.is_some() || self.text.is_some();
        match label {
            SAVE if open => self.keep(),
            SAVE => return self.save(),
            CANCEL if self.drop_open() => {}
            CANCEL => return Pressed::Leave,
            _ => {}
        }
        Pressed::Stay
    }

    pub(crate) fn key(&mut self, key: KeyEvent, room: Room) -> Pressed {
        if let Some(editor) = &mut self.text {
            if key.code == KeyCode::Esc {
                self.text = None;
            } else {
                editor.key(key, room.width, room.height);
            }
            return Pressed::Stay;
        }
        if self.field.is_some() {
            self.field_key(key);
            return Pressed::Stay;
        }
        match key.code {
            KeyCode::Esc => return Pressed::Leave,
            KeyCode::Enter | KeyCode::Char(' ') => self.activate(),
            KeyCode::Up | KeyCode::BackTab => self.select(self.cursor.saturating_sub(1)),
            KeyCode::Down | KeyCode::Tab => self.select(self.cursor + 1),
            _ => {}
        }
        Pressed::Stay
    }

    fn field_key(&mut self, key: KeyEvent) {
        let typing = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let Some((_, input)) = &mut self.field else { return };
        match key.code {
            KeyCode::Esc => self.field = None,
            KeyCode::Enter => self.keep(),
            KeyCode::Backspace => {
                input.pop();
            }
            KeyCode::Char(c) if typing => input.push(c),
            _ => {}
        }
    }

    pub(crate) fn paste(&mut self, text: &str, room: Room) {
        if let Some(editor) = &mut self.text {
            editor.insert_lines(text);
            editor.reveal(room.width, room.height);
        } else if let Some((_, input)) = &mut self.field {
            input.extend(text.chars().map(|c| if c.is_control() { ' ' } else { c }));
        }
    }

    pub(crate) fn wheel(&mut self, delta: isize, room: Room) {
        match &mut self.text {
            Some(editor) => editor.scroll_by(delta, room.width, room.height),
            None => self.select(self.cursor.saturating_add_signed(delta).min(ROWS.len() - 1)),
        }
    }
}
