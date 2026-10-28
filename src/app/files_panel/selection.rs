use ratatui::layout::{Position, Rect};

use super::super::App;
use super::super::interaction::{Anchor, Drag};
use crate::text::stretch::Stretch;
use crate::ui::files::{self as panel};

impl App {
    pub(super) fn press_file_line(&mut self, workspace: u64, n: u32, area: Rect) {
        self.press_file(workspace, (n, None), area);
    }

    pub(super) fn press_file_text(&mut self, workspace: u64, n: u32, view: &panel::View, at: (Position, Rect, Rect)) {
        let (pos, panel_area, area) = at;
        self.press_file(workspace, (n, panel::stretch_near(panel_area, view, pos)), area);
    }

    fn press_file(&mut self, workspace: u64, (line, text): (u32, Option<Stretch<u32>>), area: Rect) {
        if let Some(viewer) = self.panels.files.viewer_mut(workspace) {
            viewer.selection = Some((line, line));
            viewer.stretch = None;
            let path = viewer.path.clone();
            let anchor = Anchor::File { line, text };
            self.screen.drag = Some(Drag::Lines { workspace, path, anchor, area, moved: false, scrolled: None });
        }
    }

    pub(in crate::app) fn shows_file(&self, workspace: u64, path: &str) -> bool {
        self.files_shown()
            && self.files_target().is_some_and(|(id, _)| id == workspace)
            && self.panels.files.viewer(workspace).is_some_and(|viewer| viewer.path == path)
    }

    pub(in crate::app) fn scroll_viewer(&mut self, workspace: u64, panel_area: Rect, delta: isize) {
        let Some(view) = self.files_view() else { return };
        if let Some(top) = panel::scrolled(panel_area, &view, delta)
            && let Some(viewer) = self.panels.files.viewer_mut(workspace)
        {
            viewer.scroll_to(top);
        }
    }

    pub(in crate::app) fn select_file_lines(&mut self, workspace: u64, anchor: u32, row: u16, panel_area: Rect) {
        let Some(view) = self.files_view() else { return };
        let Some(line) = panel::line_near(panel_area, &view, row) else { return };
        if let Some(viewer) = self.panels.files.viewer_mut(workspace) {
            viewer.selection = Some((anchor, line));
        }
    }

    pub(in crate::app) fn select_file_text(
        &mut self,
        workspace: u64,
        anchor: Stretch<u32>,
        pos: Position,
        panel_area: Rect,
    ) {
        let Some(view) = self.files_view() else { return };
        let Some(pointer) = panel::stretch_near(panel_area, &view, pos) else { return };
        if let Some(viewer) = self.panels.files.viewer_mut(workspace) {
            Stretch::dragged(&mut viewer.stretch, anchor, pointer);
        }
    }
}
