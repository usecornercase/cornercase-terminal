use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use crate::app::App;
use crate::app::changes_panel::ChangesEvent;
use crate::app::event::AppEvent;
use crate::app::input::WHEEL_ROWS;
use crate::app::layout::Side;
use crate::app::overlay::Overlay;
use crate::changes::{self, BranchPicker, Checkout};
use crate::log::{Job, Level};
use crate::project::Hierarchy;
use crate::ui::{self, PickerHit};

const COMPARE_SUBMIT: &str = "compare";

impl App {
    pub(in crate::app) fn open_branches(&mut self, target: &Checkout) {
        let (workspace, dir) = (target.workspace, target.dir.clone());
        let job = Job::new(Level::Debug, "changes", "branches").with("workspace", workspace);
        self.spawn_job(
            job,
            move || Ok((changes::git::branches(&dir), changes::git::default_base(&dir))),
            move |result, _| {
                let (branches, default) = result.ok()?;
                Some(AppEvent::Changes(ChangesEvent::Branches { workspace, branches, default }))
            },
        );
    }

    pub(in crate::app) fn branches_listed(&mut self, workspace: u64, branches: Vec<String>, default: Option<String>) {
        let Some(target) = self.changes_target().filter(|t| t.workspace == workspace) else { return };
        if self.overlay.is_some()
            || self.screen.side != Some(Side::Changes)
            || self.panels.changes.scope() == changes::Scope::Every
            || self.panels.changes.mode() == changes::Mode::Uncommitted
        {
            return;
        }
        let current = self.panels.changes.label(workspace).or(target.base);
        self.overlay = Some(Overlay::Branches(BranchPicker::new(workspace, branches, default, current)));
    }

    fn branch_rows(area: Rect) -> usize {
        usize::from(ui::picker_list(ui::picker_area(area)).height)
    }

    pub(in crate::app) fn branches_key(&mut self, key: KeyEvent, area: Rect) {
        let rows = Self::branch_rows(area);
        let Some(Overlay::Branches(picker)) = &mut self.overlay else { return };
        match key.code {
            KeyCode::Esc => self.overlay = None,
            KeyCode::Enter => self.choose_base(None),
            KeyCode::Backspace => picker.pop(),
            KeyCode::Up => picker.move_selection(-1, rows),
            KeyCode::Down => picker.move_selection(1, rows),
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => picker.push(c),
            _ => {}
        }
    }

    pub(in crate::app) fn branches_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let rows = Self::branch_rows(area);
        let Some(Overlay::Branches(picker)) = &mut self.overlay else { return };
        match ev.kind {
            MouseEventKind::ScrollUp => picker.scroll_by(-WHEEL_ROWS, rows),
            MouseEventKind::ScrollDown => picker.scroll_by(WHEEL_ROWS, rows),
            MouseEventKind::Down(MouseButton::Left) => {
                match ui::picker_hit(area, COMPARE_SUBMIT, picker.items().len(), picker.scroll(), pos) {
                    Some(PickerHit::Item(i)) => self.choose_base(Some(i)),
                    Some(PickerHit::Submit) => self.choose_base(None),
                    Some(PickerHit::Cancel) => self.overlay = None,
                    None => {}
                }
            }
            _ => {}
        }
    }

    fn choose_base(&mut self, index: Option<usize>) {
        let Some(Overlay::Branches(picker)) = self.overlay.take() else { return };
        let Some(branch) = picker.chosen(index) else {
            self.overlay = Some(Overlay::Branches(picker));
            return;
        };
        self.set_changes_base(picker.workspace, Some(branch), picker.default());
    }

    pub(in crate::app) fn set_changes_base(&mut self, workspace: u64, branch: Option<String>, default: Option<&str>) {
        let base = branch.filter(|b| default != Some(b.as_str()));
        if let Some(ws) = self.projects.workspace_by_mut(workspace) {
            ws.base = base;
        }
        self.panels.changes.scroll_to_top();
    }

    pub(in crate::app) fn branches_view(picker: &BranchPicker) -> ui::Overlay {
        let items = picker.items();
        let hint = picker
            .selected()
            .and_then(|i| items.get(i))
            .map_or_else(|| "type to filter the branches".into(), |b| format!("enter compares with {b}"));
        ui::Overlay::Picker(ui::Picker {
            title: "Compare with",
            path: String::new(),
            filter: picker.filter().to_string(),
            items: items
                .iter()
                .map(|b| ui::Entry { name: (*b).to_string(), branch: picker.tag(b).map(str::to_string) })
                .collect(),
            selected: picker.selected(),
            scroll: picker.scroll(),
            hint,
            error: None,
            submit: COMPARE_SUBMIT,
            empty: "no branches",
        })
    }
}
