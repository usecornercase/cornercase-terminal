use std::io;
use std::path::PathBuf;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use crossterm::event::{KeyCode, KeyEvent};

use super::run_on;
use crate::control::{CloneFile, Unwritten};
use crate::editor::Editor;
use crate::error;
use crate::git::clone::files::{MAX_FILES, MAX_TOTAL_BYTES};
use crate::git::clone::folder_name;
use crate::git::ignored::{self, IgnoredFile, Listing, Root};
use crate::text;

pub(crate) const BACK: &str = "back";
pub(crate) const SKIP_FILES: &str = "skip files";
pub(crate) const CLONE: &str = "clone";
pub(crate) const BUTTONS: [&str; 3] = [BACK, SKIP_FILES, CLONE];

const HINT: &str = "a click on a file ticks it · only ticked files are copied · enter clones · esc goes back";
const NO_URL: &str = "the URL is required";
const READING: &str = "reading the git-ignored files here…";
pub(crate) const NONE: &str = "No git-ignored files here.";
const TOO_BIG: &str = "too big to copy";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Files {
    Reading,
    Read(Vec<(IgnoredFile, bool)>),
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tone {
    Plain,
    Muted,
    Red,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Line {
    pub(crate) text: String,
    pub(crate) tone: Tone,
    pub(crate) file: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Step {
    None,
    Back,
    Clone { url: String, files: Vec<IgnoredFile> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Opening {
    pub(crate) project: String,
    pub(crate) machine: String,
    pub(crate) folder: String,
    pub(crate) root: PathBuf,
    pub(crate) url: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CloneForm {
    pub(crate) project: String,
    pub(crate) machine: String,
    pub(crate) folder: String,
    pub(crate) root: Root,
    pub(crate) url: Editor,
    pub(crate) files: Files,
    pub(crate) left_out: usize,
    pub(crate) scroll: usize,
    pub(crate) error: Option<String>,
    pub(crate) cloning: bool,
}

impl CloneForm {
    pub(crate) fn new(opening: Opening) -> Self {
        let Opening { project, machine, folder, url, .. } = opening;
        let mut url = Editor::new(&url);
        url.home();
        Self {
            project,
            machine,
            folder,
            root: Root::default(),
            url,
            files: Files::Reading,
            left_out: 0,
            scroll: 0,
            error: None,
            cloning: false,
        }
    }

    pub(crate) fn listed(&mut self, listed: Result<Listing, String>) {
        self.files = match listed {
            Ok(listing) => {
                self.left_out = listing.left_out;
                self.root = listing.root;
                Files::Read(listing.files.into_iter().map(|file| (file, false)).collect())
            }
            Err(why) => Files::Failed(why),
        };
    }

    pub(crate) fn title(&self) -> String {
        format!("Clone {} on {}", self.project, self.machine)
    }

    pub(crate) fn intro(&self) -> String {
        self.intro_on(ignored::copied_here())
    }

    fn intro_on(&self, read_here: bool) -> String {
        if !read_here {
            return format!(
                "{} clones it from the URL, without git-ignored files (such as .env): this computer runs Windows, \
                 where reading them to copy is not supported yet.",
                self.machine
            );
        }
        format!(
            "{} clones it from the URL, then copies the git-ignored files you tick (such as .env) into the clone. \
             Ignored folders, such as node_modules, are never offered.",
            self.machine
        )
    }

    pub(crate) fn destination(&self) -> String {
        let name = folder_name(self.url.text().trim()).unwrap_or_else(|_| "…".into());
        format!("{}/{name}", self.folder.trim_end_matches('/'))
    }

    pub(crate) fn hint(&self) -> String {
        if self.cloning { format!("cloning on {}…", self.machine) } else { HINT.into() }
    }

    pub(crate) fn dimmed(&self) -> Option<&'static str> {
        self.cloning.then_some(CLONE)
    }

    pub(crate) fn lines(&self) -> Vec<Line> {
        let plain = |text: &str, tone| Line { text: text.into(), tone, file: None };
        match &self.files {
            Files::Reading => vec![plain(READING, Tone::Muted)],
            Files::Failed(why) => vec![plain(why, Tone::Red)],
            Files::Read(files) if files.is_empty() => vec![plain(NONE, Tone::Muted)],
            Files::Read(files) => {
                let mut lines: Vec<Line> =
                    files.iter().enumerate().map(|(at, (file, ticked))| line(at, file, *ticked)).collect();
                if self.left_out > 0 {
                    lines.push(plain(&not_offered(self.left_out), Tone::Muted));
                }
                lines
            }
        }
    }

    pub(crate) fn toggle(&mut self, at: usize) {
        if self.cloning {
            return;
        }
        let Files::Read(files) = &mut self.files else { return };
        let total: u64 = files.iter().filter(|(_, ticked)| *ticked).map(|(file, _)| file.size).sum();
        let Some((file, ticked)) = files.get_mut(at).filter(|(file, _)| !file.too_big) else { return };
        if !*ticked && total + file.size > MAX_TOTAL_BYTES {
            self.error = Some(over_the_total());
            return;
        }
        *ticked = !*ticked;
        self.error = None;
    }

    pub(crate) fn scroll_by(&mut self, delta: isize, rows: usize) {
        let most = self.lines().len().saturating_sub(rows);
        self.scroll = self.scroll.saturating_add_signed(delta).min(most);
    }

    pub(crate) fn ticked(&self) -> Vec<IgnoredFile> {
        let Files::Read(files) = &self.files else { return Vec::new() };
        files.iter().filter(|(file, ticked)| *ticked && !file.too_big).map(|(file, _)| file.clone()).collect()
    }

    pub(crate) fn press(&mut self, label: &str) -> Step {
        match label {
            BACK => Step::Back,
            CLONE => self.clone_with(self.ticked()),
            SKIP_FILES => self.clone_with(Vec::new()),
            _ => Step::None,
        }
    }

    fn clone_with(&mut self, files: Vec<IgnoredFile>) -> Step {
        if self.cloning {
            return Step::None;
        }
        let url = self.url.text().trim().to_string();
        if url.is_empty() {
            self.error = Some(NO_URL.into());
            return Step::None;
        }
        self.error = None;
        Step::Clone { url, files }
    }

    pub(crate) fn key(&mut self, key: KeyEvent, width: usize) -> Step {
        match key.code {
            KeyCode::Esc => Step::Back,
            KeyCode::Enter => self.press(CLONE),
            _ if self.cloning => Step::None,
            _ => {
                self.error = None;
                self.url.one_line_key(key, width);
                self.url.reveal(width, 1);
                Step::None
            }
        }
    }

    pub(crate) fn paste(&mut self, text: &str, width: usize) {
        if self.cloning {
            return;
        }
        self.error = None;
        self.url.insert_lines(&text::one_line(text));
        self.url.reveal(width, 1);
    }

    pub(crate) fn place(&mut self, width: usize, col: usize) {
        let line = self.url.scroll();
        self.url.place(width, line, col);
    }
}

pub(crate) fn over_the_total() -> String {
    format!("over the {} MiB a clone takes", MAX_TOTAL_BYTES >> 20)
}

pub(crate) fn not_offered(left_out: usize) -> String {
    format!("… and {left_out} more not offered ({MAX_FILES} at most)")
}

pub(crate) fn packed(root: &Root, files: &[IgnoredFile]) -> error::Result<Vec<CloneFile>> {
    let mut total = 0;
    let mut file = |file: &IgnoredFile| {
        let (bytes, mode) = ignored::read(root, file)?;
        total += bytes.len() as u64;
        if total > MAX_TOTAL_BYTES {
            return Err(io::Error::other(over_the_total()).into());
        }
        Ok(CloneFile { path: file.path.clone(), data: STANDARD.encode(bytes), mode })
    };
    files.iter().map(&mut file).collect()
}

pub(crate) fn cloned_note(project: &str, machine: &str, unwritten: &[Unwritten]) -> String {
    let named = |file: &&Unwritten| run_on::has_text(&file.path.to_string_lossy());
    let left = unwritten.iter().filter(named).map(|file| {
        let path = run_on::clipped(&file.path.to_string_lossy());
        format!("{path} ({})", run_on::reason(machine, &file.why))
    });
    match run_on::capped(left) {
        Some(left) => format!("cloned {project} on {machine} · not copied: {left}"),
        None => format!("cloned {project} on {machine}"),
    }
}

pub(crate) fn described(file: &IgnoredFile) -> String {
    let path = file.path.display();
    let size = if file.size < 1024 { format!("{} B", file.size) } else { text::size(file.size) };
    if file.too_big { format!("{path}  {size} · {TOO_BIG}") } else { format!("{path}  {size}") }
}

fn line(at: usize, file: &IgnoredFile, ticked: bool) -> Line {
    let text = described(file);
    if file.too_big {
        return Line { text: format!("    {text}"), tone: Tone::Muted, file: None };
    }
    let mark = if ticked { "[x]" } else { "[ ]" };
    Line { text: format!("{mark} {text}"), tone: Tone::Plain, file: Some(at) }
}

#[cfg(test)]
mod tests;
