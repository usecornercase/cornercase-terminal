use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

use crate::app::App;
use crate::files::find::Find;
use crate::ui::files::{self as panel, Hit};

impl App {
    pub(super) fn find_mouse(&mut self, workspace: u64, hit: Hit, panel_area: Rect) {
        match hit {
            Hit::Find | Hit::FindField => self.focus_find(workspace),
            Hit::FindPrevious => self.step_find(workspace, -1, panel_area),
            Hit::FindNext => self.step_find(workspace, 1, panel_area),
            Hit::FindClose => self.close_find(workspace),
            _ => {}
        }
    }

    fn focus_find(&mut self, workspace: u64) {
        if let Some(viewer) = self.panels.files.viewer_mut(workspace) {
            viewer.find.get_or_insert_with(Find::default).focused = true;
        }
    }

    fn close_find(&mut self, workspace: u64) {
        if let Some(viewer) = self.panels.files.viewer_mut(workspace) {
            viewer.find = None;
        }
    }

    pub(super) fn find_key(&mut self, workspace: u64, key: KeyEvent, panel_area: Rect) {
        let empty =
            self.panels.files.viewer(workspace).and_then(|v| v.find.as_ref()).is_none_or(|f| f.text().is_empty());
        match key.code {
            KeyCode::Esc if empty => self.close_find(workspace),
            KeyCode::Esc => self.edit_find(workspace, panel_area, String::clear),
            KeyCode::Enter | KeyCode::Down => self.step_find(workspace, 1, panel_area),
            KeyCode::Up => self.step_find(workspace, -1, panel_area),
            KeyCode::Backspace => self.edit_find(workspace, panel_area, |q| _ = q.pop()),
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                self.edit_find(workspace, panel_area, |q| q.push(c));
            }
            _ => {}
        }
    }

    pub(super) fn find_paste(&mut self, workspace: u64, text: &str, panel_area: Rect) {
        self.edit_find(workspace, panel_area, |q| q.extend(text.chars().filter(|c| !c.is_control())));
    }

    fn edit_find(&mut self, workspace: u64, panel_area: Rect, edit: impl FnOnce(&mut String)) {
        let Some(view) = self.files_view() else { return };
        let top = panel::top_line(panel_area, &view);
        let Some(viewer) = self.panels.files.viewer_mut(workspace) else { return };
        let content = viewer.content.as_ref().map(Arc::clone);
        let lines = content.as_deref().map_or(&[][..], |c| c.lines());
        let find = viewer.find.get_or_insert_with(Find::default);
        find.edit(edit, lines, top);
        find.focused = true;
        self.reveal_find(workspace, panel_area);
    }

    fn step_find(&mut self, workspace: u64, delta: isize, panel_area: Rect) {
        if let Some(find) = self.panels.files.viewer_mut(workspace).and_then(|v| v.find.as_mut()) {
            find.step(delta);
        }
        self.reveal_find(workspace, panel_area);
    }

    fn reveal_find(&mut self, workspace: u64, panel_area: Rect) {
        let Some(view) = self.files_view() else { return };
        let panel::Screen::File(file) = &view.screen else { return };
        let Some((_, at)) = file.find.as_ref().and_then(Find::current) else { return };
        if let Some(scroll) = panel::reveal(panel_area, &view, at.line)
            && let Some(viewer) = self.panels.files.viewer_mut(workspace)
        {
            viewer.scroll = scroll;
        }
    }
}
