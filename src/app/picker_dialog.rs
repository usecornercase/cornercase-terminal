use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::input::WHEEL_ROWS;
use super::overlay::Overlay;
use super::toast::Toast;
use crate::error::Result;
use crate::log::{self};
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
        if !(vscode::is_workspace(&path) && path.is_file()) {
            self.overlay = None;
            return self.open_into(path, group, None, area);
        }
        match vscode::read(&path) {
            Ok(workspace) => {
                self.overlay = None;
                self.import_workspace(workspace, group, area);
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

    fn open_into(&mut self, dir: PathBuf, group: Option<u64>, name: Option<String>, area: Rect) -> Result<()> {
        self.open_project(dir, area)?;
        let Some(project) = self.projects.get_mut(self.active) else { return Ok(()) };
        if project.name.is_none() {
            project.name = name;
        }
        if let Some(g) = group {
            project.group = Some(g);
            let closing = project.closing;
            self.cancel_pending_group_deletion(group, closing);
            if let Some(entry) = self.group_mut(g) {
                entry.collapsed = false;
            }
        }
        Ok(())
    }

    fn import_workspace(&mut self, workspace: vscode::Workspace, group: Option<u64>, area: Rect) {
        let existing = group.or_else(|| self.groups.iter().find(|g| g.entry.name == workspace.name).map(|g| g.id));
        let group = existing.unwrap_or_else(|| self.add_group(workspace.name.clone()));
        let (mut opened, mut missing, mut failed) = (Vec::new(), 0, 0);
        for folder in workspace.folders {
            if !folder.path.is_dir() {
                missing += 1;
                continue;
            }
            let folder_name = folder.path.file_name().map(|n| n.to_string_lossy().into_owned());
            let name = folder.name.filter(|n| Some(n) != folder_name.as_ref());
            if let Err(e) = self.open_into(folder.path.clone(), Some(group), name, area) {
                log::error!("app", "could not import a folder", folder = folder.path.display(), error = e);
                failed += 1;
                continue;
            }
            if let Some(id) = self.project().map(|p| p.id).filter(|id| !opened.contains(id)) {
                opened.push(id);
            }
        }
        if opened.is_empty() && existing.is_none() {
            self.delete_group(group);
        }
        if let Some(i) = opened.first().and_then(|&id| self.projects.project_index(id)) {
            self.active = i;
        }
        let name = self.group(group).map_or(workspace.name, |g| g.name.clone());
        let icon = if failed == 0 { ui::ToastIcon::Check } else { ui::ToastIcon::Bug };
        self.toast = Some(Toast::new(import_message(&name, opened.len(), missing, failed), icon));
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
                match ui::picker_hit(area, PICKER_SUBMIT, picker.items().len(), picker.scroll(), pos) {
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
            }
            _ => {}
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

fn import_message(group: &str, opened: usize, missing: usize, failed: usize) -> String {
    let imported = match opened {
        0 => format!("nothing imported from {group}"),
        1 => format!("1 project imported into {group}"),
        n => format!("{n} projects imported into {group}"),
    };
    let missing = (missing > 0).then(|| format!("{missing} not found"));
    let failed = (failed > 0).then(|| format!("{failed} could not open, see server.log"));
    [Some(imported), missing, failed].into_iter().flatten().collect::<Vec<_>>().join(", ")
}
