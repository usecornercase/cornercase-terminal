use crossterm::event::{KeyCode, KeyEvent};

use super::{DEFAULT_FOLDER, ICONS, Machine, destination_taken, known_icon, name_taken};
use super::{valid_destination, valid_folder, valid_name};
use crate::editor::Editor;
use crate::list::Combo;
use crate::text;

pub(crate) const BACK: &str = "back";
pub(crate) const ADD: &str = "add";
pub(crate) const SAVE: &str = "save";

const HINT: &str = "tab moves to the next field · enter saves · esc goes back to the machines";
const LIST_HINT: &str = "enter picks · type to filter · esc closes the list";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Field {
    Name,
    Icon,
    Destination,
    Folder,
}

impl Field {
    pub(crate) const ALL: [Self; 4] = [Self::Name, Self::Icon, Self::Destination, Self::Folder];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Icon => "icon",
            Self::Destination => "destination",
            Self::Folder => "folder",
        }
    }

    pub(crate) fn about(self) -> &'static str {
        match self {
            Self::Name => "letters, digits, `.`, `_` and `-`, at most 20 characters",
            Self::Icon => "goes before the machine's name",
            Self::Destination => "what you give ssh: user@host, or a Host of your ~/.ssh/config",
            Self::Folder => "where its clones go there: an absolute path or one starting with ~/",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    Field { field: Field, line: usize, col: usize },
    Choose(usize),
    Close,
    Button(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    None,
    Back,
    Submit,
}

#[derive(Debug, Clone)]
pub(crate) struct Form {
    pub(crate) was: Option<Machine>,
    pub(crate) name: Editor,
    pub(crate) icon: Combo,
    pub(crate) destination: Editor,
    pub(crate) folder: Editor,
    pub(crate) focus: Field,
    pub(crate) errors: Vec<(Field, String)>,
    pub(crate) notice: Option<String>,
}

fn icons(chosen: char) -> Combo {
    let options = ICONS.iter().map(|(glyph, name)| format!("{glyph} {name}")).collect();
    Combo::new(options, ICONS.iter().position(|(glyph, _)| *glyph == chosen))
}

impl Form {
    pub(crate) fn new() -> Self {
        Self {
            was: None,
            name: Editor::default(),
            icon: icons(ICONS[0].0),
            destination: Editor::default(),
            folder: Editor::new(DEFAULT_FOLDER),
            focus: Field::Name,
            errors: Vec::new(),
            notice: None,
        }
    }

    pub(crate) fn editing(machine: &Machine) -> Self {
        Self {
            was: Some(machine.clone()),
            name: Editor::new(&machine.name),
            icon: icons(known_icon(&machine.icon)),
            destination: Editor::new(&machine.destination),
            folder: Editor::new(&machine.folder),
            ..Self::new()
        }
    }

    pub(crate) fn title(&self) -> String {
        let name = self.was.as_ref().map_or("new machine", |was| was.name.as_str());
        format!("Settings › Machines › {name}")
    }

    pub(crate) fn submit(&self) -> &'static str {
        if self.was.is_some() { SAVE } else { ADD }
    }

    pub(crate) fn buttons(&self) -> Vec<&'static str> {
        vec![BACK, self.submit()]
    }

    pub(crate) fn hint(&self) -> &'static str {
        if self.icon.is_open() { LIST_HINT } else { HINT }
    }

    pub(crate) fn error(&self, field: Field) -> Option<&str> {
        self.errors.iter().find(|(f, _)| *f == field).map(|(_, why)| why.as_str())
    }

    pub(crate) fn editor_of(&self, field: Field) -> Option<&Editor> {
        match field {
            Field::Name => Some(&self.name),
            Field::Destination => Some(&self.destination),
            Field::Folder => Some(&self.folder),
            Field::Icon => None,
        }
    }

    pub(crate) fn editor(&mut self, field: Field) -> Option<&mut Editor> {
        match field {
            Field::Name => Some(&mut self.name),
            Field::Destination => Some(&mut self.destination),
            Field::Folder => Some(&mut self.folder),
            Field::Icon => None,
        }
    }

    fn touched(&mut self) {
        let focus = self.focus;
        self.errors.retain(|(field, _)| *field != focus);
        self.notice = None;
    }

    fn step_focus(&mut self, forward: bool) {
        self.icon.close();
        let at = Field::ALL.iter().position(|field| *field == self.focus).unwrap_or_default();
        let count = Field::ALL.len();
        let next = if forward { at + 1 } else { at + count - 1 };
        self.focus = Field::ALL[next % count];
    }

    pub(crate) fn key(&mut self, key: KeyEvent, width: usize, rows: usize) -> Step {
        self.touched();
        if self.icon.is_open() && !matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
            self.icon.key(key, rows);
            return Step::None;
        }
        match key.code {
            KeyCode::Esc => return Step::Back,
            KeyCode::Tab => self.step_focus(true),
            KeyCode::BackTab => self.step_focus(false),
            KeyCode::Enter if self.focus != Field::Icon => return Step::Submit,
            _ => {
                let focus = self.focus;
                match self.editor(focus) {
                    Some(editor) => {
                        editor.one_line_key(key, width);
                    }
                    None => {
                        self.icon.key(key, rows);
                    }
                }
            }
        }
        Step::None
    }

    pub(crate) fn paste(&mut self, text: &str, width: usize) {
        self.touched();
        let focus = self.focus;
        if let Some(editor) = self.editor(focus) {
            editor.insert_lines(&text::one_line(text));
            editor.reveal(width, 1);
        }
    }

    pub(crate) fn hit(&mut self, hit: Hit, width: usize) -> Step {
        match hit {
            Hit::Button(BACK) => return Step::Back,
            Hit::Button(_) => return Step::Submit,
            Hit::Choose(i) => self.icon.choose(i),
            Hit::Close => self.icon.close(),
            Hit::Field { field: Field::Icon, .. } => {
                let open = self.icon.is_open();
                self.focus = Field::Icon;
                if open { self.icon.close() } else { self.icon.open() }
            }
            Hit::Field { field, line, col } => {
                self.icon.close();
                self.focus = field;
                if let Some(editor) = self.editor(field) {
                    let line = line + editor.scroll();
                    editor.place(width, line, col);
                }
            }
        }
        Step::None
    }

    fn built(&self) -> Machine {
        let icon = self.icon.chosen().and_then(|i| ICONS.get(i)).map_or(ICONS[0].0, |(glyph, _)| *glyph);
        Machine {
            name: self.name.text().trim().into(),
            icon: icon.to_string(),
            destination: self.destination.text().trim().into(),
            folder: self.folder.text().trim().into(),
        }
    }

    pub(crate) fn checked(&mut self, machines: &[Machine]) -> Option<(Option<usize>, Machine)> {
        let at = self.was.as_ref().and_then(|was| machines.iter().position(|machine| machine == was));
        let others: Vec<Machine> =
            machines.iter().enumerate().filter(|(i, _)| Some(*i) != at).map(|(_, machine)| machine.clone()).collect();
        let machine = self.built();
        let name = valid_name(&machine.name).err().map(String::from).or_else(|| name_taken(&others, &machine.name));
        let destination = valid_destination(&machine.destination)
            .err()
            .map(String::from)
            .or_else(|| destination_taken(&others, &machine.destination));
        let folder = valid_folder(&machine.folder).err().map(String::from);
        self.errors = [(Field::Name, name), (Field::Destination, destination), (Field::Folder, folder)]
            .into_iter()
            .filter_map(|(field, why)| why.map(|why| (field, why)))
            .collect();
        if let Some((field, _)) = self.errors.first() {
            self.focus = *field;
            return None;
        }
        Some((at, machine))
    }
}

#[cfg(test)]
mod tests;
