use ratatui::layout::{Position, Rect};

use super::super::App;
use super::super::interaction::{Anchor, Drag};
use super::ready;
use crate::changes::selection::{self, Selection, Spot};
use crate::text::stretch::Stretch;
use crate::ui::changes::{self as panel, LinesAction};

impl App {
    pub(super) fn press_diff_line(&mut self, view: &panel::View, (i, spot): (usize, Spot), area: Rect) {
        self.press_diff(view, (i, spot, None), area);
    }

    pub(super) fn press_diff_text(&mut self, view: &panel::View, (i, spot): (usize, Spot), at: (Position, Rect, Rect)) {
        let (pos, panel_area, area) = at;
        self.press_diff(view, (i, spot, panel::stretch_near(panel_area, view, i, pos)), area);
    }

    fn press_diff(&mut self, view: &panel::View, (i, spot, text): (usize, Spot, Option<Stretch<Spot>>), area: Rect) {
        let Some(file) = ready(view).and_then(|d| d.files.get(i)) else { return };
        let Some(&workspace) = view.owners.get(i) else { return };
        let path = file.path.clone();
        let selection =
            Selection { workspace, path: path.clone(), digest: file.digest, anchor: spot, end: spot, stretch: None };
        self.panels.changes.select(selection);
        let anchor = Anchor::Diff { spot, text };
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

    pub(in crate::app) fn select_diff_text(&mut self, anchor: Stretch<Spot>, pos: Position, panel_area: Rect) {
        let Some(view) = self.panel_view() else { return };
        let Some(selected) = view.selection else { return };
        let Some(pointer) = panel::stretch_near(panel_area, &view, selected.file, pos) else { return };
        let mut stretch = selected.stretch;
        Stretch::dragged(&mut stretch, anchor, pointer);
        self.panels.changes.set_stretch(stretch);
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
            LinesAction::Copy => self.copy(&match selected.stretch {
                Some(stretch) => stretch.text(lines.iter().map(|line| line.text)),
                None => selection::text(&lines),
            }),
            LinesAction::Ask => {
                let reference = selection::reference(
                    file,
                    selected.from.hunk,
                    selection::covered(&lines, selected.stretch.as_ref()),
                );
                if let Some(&workspace) = view.owners.get(selected.file) {
                    self.ask_agent(workspace, &reference);
                }
            }
        }
    }
}
