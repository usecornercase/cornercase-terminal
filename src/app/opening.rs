use std::path::{Path, PathBuf};

use ratatui::layout::Position;

use super::App;
use super::menus::MenuAction;
use super::overlay::Overlay;
use super::toast::Toast;
use crate::host::open::{Failed, Kind, Open, Opener};
use crate::log;
use crate::project::Hierarchy;
use crate::text;
use crate::ui;

const NOT_UTF8: &str = "cannot open a path that is not UTF-8";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Subject {
    Project(u64),
    Workspace(u64),
    Pane(u64),
    File(u64),
    Row(u64),
}

#[derive(Debug, Default)]
pub(super) struct Opening {
    window: u64,
    openers: Vec<Opener>,
    row: Option<(u64, String)>,
}

struct Place {
    path: PathBuf,
    line: Option<u32>,
    kind: Kind,
}

impl App {
    pub(crate) fn set_window(&mut self, client: u64, openers: &[Opener]) {
        if self.opening.window != client || self.opening.openers != openers {
            self.opening.window = client;
            self.opening.openers = openers.to_vec();
        }
    }

    pub(crate) fn take_opens(&mut self) -> Vec<(u64, Open)> {
        self.outbox.take_opens()
    }

    pub(crate) fn open_failed(&mut self, client: u64, failed: &Failed) {
        let reason = text::one_line(&failed.reason);
        log::warning!("ui", "could not open", client = client, opener = failed.opener.id(), reason = reason);
        let message = format!("could not open in {}: {reason}", failed.opener.name());
        self.messages.toast = Some(Toast::new(message, ui::ToastIcon::Bug));
    }

    pub(super) fn open_entry(&self, subject: Subject) -> MenuAction {
        if self.opening.openers.is_empty() { MenuAction::CopyPath(subject) } else { MenuAction::OpenIn(subject) }
    }

    pub(super) fn open_in_menu(&mut self, subject: Subject, at: Position) {
        let Some(place) = self.place(subject) else { return };
        let actions: Vec<MenuAction> = self
            .opening
            .openers
            .iter()
            .filter(|opener| opener.applies(place.kind))
            .map(|&opener| MenuAction::OpenWith(subject, opener))
            .collect();
        if actions.is_empty() {
            self.copy_path(subject);
        } else {
            self.overlay = Some(Overlay::Menu { at, actions });
        }
    }

    pub(super) fn open_file_menu(&mut self, workspace: u64, at: Position) {
        if self.opening.openers.is_empty() {
            self.copy_path(Subject::File(workspace));
        } else {
            self.open_in_menu(Subject::File(workspace), at);
        }
    }

    pub(super) fn open_row_menu(&mut self, workspace: u64, row: &str, at: Position) {
        self.opening.row = Some((workspace, row.to_string()));
        let subject = Subject::Row(workspace);
        if self.opening.openers.is_empty() {
            self.overlay = Some(Overlay::Menu { at, actions: vec![MenuAction::CopyPath(subject)] });
        } else {
            self.open_in_menu(subject, at);
        }
    }

    pub(super) fn open_with(&mut self, subject: Subject, opener: Opener) {
        let Some(place) = self.place(subject) else { return };
        if !self.opening.openers.contains(&opener) || !opener.applies(place.kind) {
            self.copy_path(subject);
            return;
        }
        let Some(path) = place.path.to_str() else {
            self.messages.toast = Some(Toast::new(NOT_UTF8, ui::ToastIcon::Bug));
            return;
        };
        let client = self.opening.window;
        log::info!("ui", "open in", client = client, opener = opener.id(), folder = place.kind == Kind::Folder);
        let open = Open { opener, path: path.to_string(), line: place.line };
        self.outbox.open(client, open);
    }

    pub(super) fn copy_path(&mut self, subject: Subject) {
        let Some(place) = self.place(subject) else { return };
        let Some(path) = place.path.to_str() else {
            self.messages.toast = Some(Toast::new(NOT_UTF8, ui::ToastIcon::Bug));
            return;
        };
        self.copy(path);
        let what = if place.kind == Kind::Folder { "folder" } else { "file" };
        let message = format!("copied the {what}'s path: it is on the server's machine");
        self.messages.toast = Some(Toast::new(message, ui::ToastIcon::Check));
    }

    fn workspace_path(&self, workspace: u64) -> Option<&Path> {
        Some(&self.projects.workspace_by(workspace)?.path)
    }

    fn place(&self, subject: Subject) -> Option<Place> {
        let folder = |path: PathBuf| Place { path, line: None, kind: Kind::Folder };
        match subject {
            Subject::Project(id) => Some(folder(self.projects[self.projects.project_index(id)?].path.clone())),
            Subject::Workspace(id) => Some(folder(self.workspace_path(id)?.to_path_buf())),
            Subject::Pane(id) => {
                let cwd = self.projects.pane_by(id)?.cwd();
                if let Some(cwd) = cwd.filter(|cwd| cwd.is_dir()) {
                    return Some(folder(cwd));
                }
                let at = self.projects.tab_of_pane(id)?.workspace();
                Some(folder(self.projects.workspace(at).path.clone()))
            }
            Subject::File(workspace) => {
                let root = self.workspace_path(workspace)?;
                let viewer = self.panels.files.viewer(workspace)?;
                let path = root.join(&viewer.path);
                let image = viewer.content.as_ref().is_some_and(|content| content.is_image());
                let line = viewer.selected().filter(|_| !image).map(|(first, _)| first);
                Some(Place { kind: Kind::of(&path).unwrap_or(Kind::File), path, line })
            }
            Subject::Row(workspace) => {
                let (row_workspace, row) = self.opening.row.as_ref()?;
                if *row_workspace != workspace {
                    return None;
                }
                let path = self.workspace_path(workspace)?.join(row);
                Some(Place { kind: Kind::of(&path)?, path, line: None })
            }
        }
    }
}
