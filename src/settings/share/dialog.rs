use crossterm::event::{KeyCode, KeyEvent};

use super::preview::change_text;
use super::{MAX_BYTES, NEVER_SHARED, Section, Shared, too_large};
use crate::control::config::{NoteKind, SectionPreview, SetupChange, SetupExported, SetupNote, SetupPreview};
use crate::editor::Editor;
use crate::text;

pub(crate) const COPY: &str = "copy my setup";
pub(crate) const PREVIEW: &str = "preview";
pub(crate) const BACK: &str = "back";
pub(crate) const IMPORT: &str = "import";
pub(crate) const INTRO: &str = "Copy your setup to send it to a colleague, or paste one they sent you. Tokens and \
                                personal settings are never shared.";
const IMPORT_INTRO: &str = "Tick the sections to import: their settings take the shared values, the rest stay as \
                            they are. Red notes are programs that will run, or values to check.";
const NOTHING: &str = "Nothing to import: this setup matches the settings in use.";
const EMPTY: &str = "paste a setup first";
const PASTE_HINT: &str = "paste here · enter starts a new line · preview shows what would change · esc goes back";
const IMPORT_HINT: &str = "a click on a section ticks or unticks it · enter imports the ticked ones · esc goes back";
const SHARE_TITLE: &str = "Settings › Share";
const IMPORT_TITLE: &str = "Settings › Import";
const INDENT: &str = "      ";
const SCRIPT_INDENT: &str = "          ";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Room {
    pub(crate) width: usize,
    pub(crate) height: usize,
}

#[derive(Debug)]
pub(crate) enum Page {
    Paste {
        editor: Editor,
        error: Option<String>,
    },
    Preview {
        shared: Box<Shared>,
        preview: SetupPreview,
        ticked: Vec<bool>,
        scroll: usize,
        pasted: Editor,
        error: Option<String>,
    },
}

#[derive(Debug)]
pub(crate) struct Dialog {
    pub(crate) page: Page,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Action {
    None,
    Back,
    Copy,
    Preview(String),
    Import(Box<Shared>, Vec<Section>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tone {
    Section,
    Plain,
    Muted,
    Red,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Line {
    pub(crate) text: String,
    pub(crate) tone: Tone,
    pub(crate) section: Option<usize>,
}

impl Line {
    fn new(text: String, tone: Tone) -> Self {
        Self { text, tone, section: None }
    }
}

impl Dialog {
    pub(crate) fn paste_editor(&self) -> Option<&Editor> {
        match &self.page {
            Page::Paste { editor, .. } => Some(editor),
            Page::Preview { .. } => None,
        }
    }

    pub(crate) fn paste_editor_mut(&mut self) -> Option<&mut Editor> {
        match &mut self.page {
            Page::Paste { editor, .. } => Some(editor),
            Page::Preview { .. } => None,
        }
    }

    pub(crate) fn new(text: &str) -> Self {
        let mut editor = Editor::new(text);
        editor.home();
        Self { page: Page::Paste { editor, error: None }, warnings: Vec::new() }
    }

    pub(crate) fn key(&mut self, key: KeyEvent, room: Room) -> Action {
        self.warnings.clear();
        match (&mut self.page, key.code) {
            (Page::Paste { .. }, KeyCode::Esc) => return Action::Back,
            (Page::Paste { editor, error }, _) => {
                *error = None;
                editor.key(key, room.width, room.height);
            }
            (Page::Preview { .. }, KeyCode::Esc) => self.back(),
            (Page::Preview { .. }, KeyCode::Enter) => return self.import(),
            (Page::Preview { .. }, KeyCode::Up) => self.scroll(-1, room),
            (Page::Preview { .. }, KeyCode::Down) => self.scroll(1, room),
            (Page::Preview { .. }, _) => {}
        }
        Action::None
    }

    pub(crate) fn paste(&mut self, text: &str, room: Room) {
        self.warnings.clear();
        let Page::Paste { editor, error } = &mut self.page else { return };
        if editor.text().len() + text.len() > MAX_BYTES {
            *error = Some(too_large());
            return;
        }
        *error = None;
        editor.insert_lines(text);
        editor.reveal(room.width, room.height);
    }

    pub(crate) fn press(&mut self, label: &str) -> Action {
        self.warnings.clear();
        match (&self.page, label) {
            (Page::Paste { .. }, BACK) => Action::Back,
            (Page::Paste { .. }, COPY) => Action::Copy,
            (Page::Paste { .. }, PREVIEW) => self.preview(),
            (Page::Preview { .. }, BACK) => {
                self.back();
                Action::None
            }
            (Page::Preview { .. }, IMPORT) => self.import(),
            _ => Action::None,
        }
    }

    fn preview(&mut self) -> Action {
        let Page::Paste { editor, error } = &mut self.page else { return Action::None };
        let text = editor.text();
        if text.trim().is_empty() {
            *error = Some(EMPTY.into());
            return Action::None;
        }
        Action::Preview(text)
    }

    fn back(&mut self) {
        if let Page::Preview { pasted, .. } = &mut self.page {
            self.page = Page::Paste { editor: std::mem::take(pasted), error: None };
        }
    }

    pub(crate) fn previewed(&mut self, result: Result<(Shared, SetupPreview), String>) {
        let Page::Paste { editor, error } = &mut self.page else { return };
        match result {
            Err(why) => *error = Some(why),
            Ok((shared, preview)) => {
                let ticked = preview.sections.iter().map(changes).collect();
                let pasted = std::mem::take(editor);
                self.page = Page::Preview { shared: Box::new(shared), preview, ticked, scroll: 0, pasted, error: None };
            }
        }
    }

    pub(crate) fn refused(&mut self, why: String) {
        let (Page::Paste { error, .. } | Page::Preview { error, .. }) = &mut self.page;
        *error = Some(why);
    }

    pub(crate) fn place(&mut self, line: usize, col: usize, room: Room) {
        if let Page::Paste { editor, .. } = &mut self.page {
            let line = line + editor.scroll();
            editor.place(room.width, line, col);
        }
    }

    pub(crate) fn toggle(&mut self, section: usize) {
        let Page::Preview { preview, ticked, error, .. } = &mut self.page else { return };
        *error = None;
        if let (Some(shown), Some(tick)) = (preview.sections.get(section), ticked.get_mut(section))
            && changes(shown)
        {
            *tick = !*tick;
        }
    }

    pub(crate) fn scroll(&mut self, delta: isize, room: Room) {
        let most = self.lines(room.width).len().saturating_sub(room.height);
        match &mut self.page {
            Page::Paste { editor, .. } => editor.scroll_by(delta, room.width, room.height),
            Page::Preview { scroll, .. } => *scroll = scroll.saturating_add_signed(delta).min(most),
        }
    }

    fn ticked_sections(&self) -> Vec<Section> {
        let Page::Preview { preview, ticked, .. } = &self.page else { return Vec::new() };
        preview
            .sections
            .iter()
            .zip(ticked)
            .filter(|(section, ticked)| **ticked && changes(section))
            .filter_map(|(section, _)| Section::of(&section.id))
            .collect()
    }

    fn import(&self) -> Action {
        let Page::Preview { shared, .. } = &self.page else { return Action::None };
        let sections = self.ticked_sections();
        if sections.is_empty() { Action::None } else { Action::Import(shared.clone(), sections) }
    }

    pub(crate) fn buttons(&self) -> Vec<&'static str> {
        match self.page {
            Page::Paste { .. } => vec![BACK, COPY, PREVIEW],
            Page::Preview { .. } => vec![BACK, IMPORT],
        }
    }

    pub(crate) fn submit(&self) -> &'static str {
        match self.page {
            Page::Paste { .. } => PREVIEW,
            Page::Preview { .. } => IMPORT,
        }
    }

    pub(crate) fn dimmed(&self) -> Option<&'static str> {
        let importable = !self.ticked_sections().is_empty();
        (matches!(self.page, Page::Preview { .. }) && !importable).then_some(IMPORT)
    }

    pub(crate) fn title(&self) -> &'static str {
        match self.page {
            Page::Paste { .. } => SHARE_TITLE,
            Page::Preview { .. } => IMPORT_TITLE,
        }
    }

    pub(crate) fn intro(&self) -> &'static str {
        match &self.page {
            Page::Paste { .. } => INTRO,
            Page::Preview { preview, .. } if !preview.sections.iter().any(changes) => NOTHING,
            Page::Preview { .. } => IMPORT_INTRO,
        }
    }

    pub(crate) fn hint(&self) -> String {
        match self.page {
            Page::Paste { .. } => PASTE_HINT.into(),
            Page::Preview { .. } => IMPORT_HINT.into(),
        }
    }

    pub(crate) fn note(&self) -> Option<String> {
        let (Page::Paste { error, .. } | Page::Preview { error, .. }) = &self.page;
        error.clone().or_else(|| (!self.warnings.is_empty()).then(|| self.warnings.join(" · ")))
    }

    pub(crate) fn lines(&self, width: usize) -> Vec<Line> {
        let Page::Preview { preview, ticked, .. } = &self.page else { return Vec::new() };
        let mut lines = Vec::new();
        for (i, (section, ticked)) in preview.sections.iter().zip(ticked).enumerate() {
            if i > 0 {
                lines.push(Line::new(String::new(), Tone::Plain));
            }
            lines.push(Line { section: Some(i), ..heading(section, *ticked) });
            lines.extend(section.changes.iter().flat_map(change_lines));
            lines.extend(section.notes.iter().map(|note| note_line(note, INDENT)));
        }
        if !preview.notes.is_empty() {
            lines.push(Line::new(String::new(), Tone::Plain));
            lines.extend(preview.notes.iter().map(|note| note_line(note, "")));
        }
        lines.into_iter().flat_map(|line| wrapped(line, width)).collect()
    }
}

pub(crate) fn copied(exported: &SetupExported) -> String {
    let (settings, repositories) = super::counts(exported.settings, exported.repositories);
    let mut copied = format!("setup copied: {settings}, {repositories} · left out: tokens, personal settings");
    let not_shared = not_shared(exported).len();
    if not_shared > 0 {
        copied = format!("{copied} · {} not shared", text::plural(not_shared, "repository", "repositories"));
    }
    match exported.warnings.first() {
        Some(warning) => format!("{copied} · check: {warning}"),
        None => copied,
    }
}

pub(crate) fn copy_notes(exported: &SetupExported) -> Vec<String> {
    not_shared(exported).into_iter().chain(&exported.warnings).cloned().collect()
}

fn not_shared(exported: &SetupExported) -> Vec<&String> {
    exported.left_out.iter().filter(|entry| !NEVER_SHARED.contains(&entry.as_str())).collect()
}

fn changes(section: &SectionPreview) -> bool {
    !section.changes.is_empty()
}

fn heading(section: &SectionPreview, ticked: bool) -> Line {
    let count = match section.changes.len() {
        0 => return Line::new(format!("    {} · no changes", section.id), Tone::Muted),
        1 => "1 change".to_string(),
        n => format!("{n} changes"),
    };
    let mark = if ticked { "[x]" } else { "[ ]" };
    Line::new(format!("{mark} {} · {count}", section.id), Tone::Section)
}

fn note_line(note: &SetupNote, indent: &str) -> Line {
    let tone = match note.kind {
        NoteKind::Risk | NoteKind::Secret => Tone::Red,
        NoteKind::Missing | NoteKind::Connect | NoteKind::Unknown | NoteKind::Repository | NoteKind::Other => {
            Tone::Muted
        }
    };
    Line::new(format!("{indent}{}", note.text), tone)
}

fn change_lines(change: &SetupChange) -> Vec<Line> {
    let text = change_text(change);
    let script = text.script.into_iter().map(|line| Line::new(format!("{SCRIPT_INDENT}{line}"), Tone::Red));
    std::iter::once(Line::new(format!("{INDENT}{}", text.head), Tone::Plain)).chain(script).collect()
}

fn wrapped(line: Line, width: usize) -> Vec<Line> {
    if line.section.is_some() || width == 0 || text::cells(&line.text) <= width {
        return vec![line];
    }
    let body = line.text.trim_start_matches(' ');
    let indent = &line.text[..line.text.len() - body.len()];
    let room = width.saturating_sub(indent.len()).max(1);
    text::wrap_cells(body, room)
        .into_iter()
        .map(|part| Line { text: format!("{indent}{part}"), ..line.clone() })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests;
