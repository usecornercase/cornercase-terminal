use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::input::WHEEL_ROWS;
use super::overlay::Overlay;
use super::toast::Toast;
use crate::control::ImportedWorkspace;
use crate::error::Result;
use crate::log::{self};
use crate::os::fs::canonical;
use crate::project::Hierarchy;
use crate::project::picker::Picker;
use crate::project::vscode;
use crate::ui::{self, PickerHit};

pub(super) const PICKER_SUBMIT: &str = "open";

const VSCODE_TAG: &str = "vs code";

impl App {
    pub(super) fn open_picker(&mut self, group: Option<u64>) {
        let home = self.home.as_deref();
        let in_group = group.and_then(|g| self.projects.iter().find(|p| p.group == Some(g)));
        let near = in_group.or_else(|| self.project()).and_then(|p| p.path.parent().map(Path::to_path_buf));
        let picker = near
            .and_then(|dir| Picker::open(&dir, home).ok())
            .or_else(|| home.and_then(|dir| Picker::open(dir, home).ok()))
            .or_else(|| Picker::open(Path::new("/"), home).ok());
        self.overlay = picker.map(|picker| Overlay::Picker { picker, group });
    }

    fn open_chosen(&mut self, path: PathBuf, group: Option<u64>, area: Rect) -> Result<()> {
        if !vscode::is_workspace(&path) {
            self.overlay = None;
            return self.open_into(path, group, area);
        }
        match vscode::read(&path) {
            Ok(workspace) => {
                self.overlay = None;
                let imported = self.import_workspace(workspace, group, area);
                self.show_imported(&imported);
                Ok(())
            }
            Err(e) => {
                if let Some(Overlay::Picker { picker, .. }) = &mut self.overlay {
                    picker.fail(e.to_string());
                }
                Ok(())
            }
        }
    }

    fn open_into(&mut self, dir: PathBuf, group: Option<u64>, area: Rect) -> Result<()> {
        self.open_project(dir, area)?;
        if let Some(g) = group {
            self.join_group(self.active, g);
            if let Some(entry) = self.group_mut(g) {
                entry.collapsed = false;
            }
        }
        Ok(())
    }

    pub(in crate::app) fn join_group(&mut self, p: usize, group: u64) {
        let project = &mut self.projects[p];
        project.group = Some(group);
        let closing = project.closing;
        self.cancel_pending_group_deletion(Some(group), closing);
    }

    pub(in crate::app) fn import_workspace(
        &mut self,
        workspace: vscode::Workspace,
        into: Option<u64>,
        area: Rect,
    ) -> ImportedWorkspace {
        let vscode::Workspace { name, folders } = workspace;
        let existing = into.or_else(|| self.groups.iter().find(|g| g.entry.name == name).map(|g| g.id));
        let group = existing.unwrap_or_else(|| self.add_group(name.clone()));
        let mut imported = ImportedWorkspace { group: Some(group), ..ImportedWorkspace::default() };
        for folder in folders {
            self.import_folder(folder, group, area, &mut imported);
        }
        if imported.projects.is_empty() && existing.is_none() {
            self.delete_group(group);
            imported.group = None;
        }
        imported.name = self.group(group).map_or(name, |g| g.name.clone());
        imported
    }

    fn import_folder(&mut self, folder: vscode::Folder, group: u64, area: Rect, imported: &mut ImportedWorkspace) {
        if !folder.path.is_dir() {
            imported.missing += 1;
            return;
        }
        let p = match self.open_folder(canonical(folder.path.clone()), Some(area)) {
            Ok(p) => p,
            Err(e) => {
                log::error!("app", "could not import a folder", folder = folder.path.display(), error = e);
                imported.failed += 1;
                return;
            }
        };
        let folder_name = folder.path.file_name().map(|n| n.to_string_lossy().into_owned());
        let project = &mut self.projects[p];
        if project.name.is_none() {
            project.name = folder.name.filter(|n| Some(n) != folder_name.as_ref());
        }
        let id = project.id;
        self.join_group(p, group);
        if !imported.projects.contains(&id) {
            imported.projects.push(id);
        }
    }

    fn show_imported(&mut self, imported: &ImportedWorkspace) {
        if let Some(i) = imported.projects.first().and_then(|&id| self.projects.project_index(id)) {
            self.leave_remote();
            self.active = i;
            if let Some(entry) = imported.group.and_then(|g| self.group_mut(g)) {
                entry.collapsed = false;
            }
        }
        let icon = if imported.failed == 0 { ui::ToastIcon::Check } else { ui::ToastIcon::Bug };
        self.messages.toast = Some(Toast::new(imported.message(), icon));
    }

    fn picker_rows(area: Rect) -> usize {
        usize::from(ui::picker_list(ui::picker_area(area)).height)
    }

    pub(super) fn picker_key(&mut self, key: KeyEvent, area: Rect) -> Result<()> {
        let Some(Overlay::Picker { picker, group }) = &mut self.overlay else { return Ok(()) };
        let (group, rows) = (*group, Self::picker_rows(area));
        match key.code {
            KeyCode::Esc => self.overlay = None,
            KeyCode::Enter => {
                if let Some(path) = picker.submit() {
                    return self.open_chosen(path, group, area);
                }
            }
            KeyCode::Backspace => picker.pop(),
            KeyCode::Left => picker.up(),
            KeyCode::Right | KeyCode::Tab => picker.enter_selected(),
            KeyCode::Up => picker.move_selection(-1, rows),
            KeyCode::Down => picker.move_selection(1, rows),
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => picker.push(c),
            _ => {}
        }
        Ok(())
    }

    pub(super) fn picker_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> Result<()> {
        let Some(Overlay::Picker { picker, group }) = &mut self.overlay else { return Ok(()) };
        let (group, rows) = (*group, Self::picker_rows(area));
        match ev.kind {
            MouseEventKind::ScrollUp => picker.scroll_by(-WHEEL_ROWS, rows),
            MouseEventKind::ScrollDown => picker.scroll_by(WHEEL_ROWS, rows),
            MouseEventKind::Down(MouseButton::Left) => {
                let hit = ui::picker_hit(area, PICKER_SUBMIT, picker.items().len(), picker.scroll(), pos);
                return self.picker_clicked(hit, group, area);
            }
            _ => {}
        }
        Ok(())
    }

    fn picker_clicked(&mut self, hit: Option<PickerHit>, group: Option<u64>, area: Rect) -> Result<()> {
        let Some(Overlay::Picker { picker, .. }) = &mut self.overlay else { return Ok(()) };
        match hit {
            Some(PickerHit::Item(i)) => {
                if let Some(path) = picker.choose(i) {
                    return self.open_chosen(path, group, area);
                }
            }
            Some(PickerHit::Submit) => {
                let dir = picker.dir().to_path_buf();
                return self.open_chosen(dir, group, area);
            }
            Some(PickerHit::Cancel) => self.overlay = None,
            None => {}
        }
        Ok(())
    }

    pub(super) fn picker_view(picker: &Picker, into_group: bool, home: Option<&Path>) -> ui::Overlay {
        let items = picker.items();
        let dir = crate::text::display_path(picker.dir(), home);
        let hint = match picker.selected().and_then(|i| items.get(i)) {
            Some(item) if item.name == ".." => "enter goes up".into(),
            Some(item) if item.workspace => format!("enter imports {}", item.name),
            Some(item) => format!("enter goes into {}", item.name),
            None if picker.filter().is_empty() => format!("enter opens {dir}"),
            None => String::new(),
        };
        ui::Overlay::Picker(ui::Picker {
            title: if into_group { "Add project" } else { "Open project" },
            path: if dir.ends_with('/') { dir } else { format!("{dir}/") },
            filter: picker.filter().to_string(),
            items: items
                .iter()
                .map(|item| ui::Entry {
                    name: item.name.clone(),
                    branch: if item.workspace { Some(VSCODE_TAG.into()) } else { item.branch.clone() },
                })
                .collect(),
            selected: picker.selected(),
            scroll: picker.scroll(),
            hint,
            error: picker.error().map(str::to_string),
            submit: PICKER_SUBMIT,
            empty: "no folders here",
        })
    }
}
