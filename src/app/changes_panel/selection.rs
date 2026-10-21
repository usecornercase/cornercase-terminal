use ratatui::layout::Rect;

use super::super::App;
use super::super::interaction::{Anchor, Drag};
use super::ready;
use crate::changes::selection::{self, Selection, Spot};
use crate::ui::changes::{self as panel, LinesAction};

impl App {
    pub(super) fn press_diff_line(&mut self, view: &panel::View, (i, spot): (usize, Spot), area: Rect) {
        let Some(file) = ready(view).and_then(|d| d.files.get(i)) else { return };
        let Some(&workspace) = view.owners.get(i) else { return };
        let path = file.path.clone();
        let selection = Selection { workspace, path: path.clone(), digest: file.digest, anchor: spot, end: spot };
        self.panels.changes.select(selection);
        let anchor = Anchor::Diff(spot);
        self.screen.drag = Some(Drag::Lines { workspace, path, anchor, area, moved: false, scrolled: None });
    }

    pub(in crate::app) fn shows_diff_lines(&self, workspace: u64, path: &str, spot: Spot) -> bool {
        self.changes_shown()
            && self
                .panels
                .changes
                .selection()
                .is_some_and(|s| s.workspace == workspace && s.path == path && s.anchor == spot)
            && self.panel_view().is_some_and(|view| view.selection.is_some())
    }

    pub(in crate::app) fn select_diff_lines(&mut self, row: u16, panel_area: Rect) {
        let Some(view) = self.panel_view() else { return };
        let Some(selected) = view.selection else { return };
        if let Some(end) = panel::line_near(panel_area, &view, selected.file, row) {
            self.panels.changes.select_to(end);
        }
    }

    pub(in crate::app) fn scroll_diff(&mut self, panel_area: Rect, delta: isize) {
        let Some(view) = self.panel_view() else { return };
        self.panels.changes.scroll_by(delta, panel::max_scroll(panel_area, &view));
    }

    pub(super) fn selected_lines_action(&mut self, view: &panel::View, action: LinesAction) {
        let Some(selected) = view.selection else { return };
        let Some(file) = ready(view).and_then(|d| d.files.get(selected.file)) else { return };
        let lines = panel::picked(view, &selected);
        match action {
            LinesAction::Copy => self.copy(&selection::text(&lines)),
            LinesAction::Ask => {
                let reference = selection::reference(file, selected.from.hunk, &lines);
                if let Some(&workspace) = view.owners.get(selected.file) {
                    self.ask_agent(workspace, &reference);
                }
            }
        }
    }
}
