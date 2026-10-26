use std::time::Instant;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::interaction::Drag;
use super::layout::Side;
use super::overlay::Overlay;
use crate::editor::Editor;
use crate::project::settings::Field as SettingsField;
use crate::queue::form::Control;
use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TextField {
    Todo,
    Tour,
    Queue(Control),
    Flows,
    StartPrompt,
    Setup(SettingsField),
    SharePaste,
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FieldArea {
    pub(super) text: Rect,
    pub(super) width: usize,
    pub(super) top: usize,
    pub(super) scrolls: bool,
}

impl FieldArea {
    pub(super) fn spot(&self, pos: Position) -> (usize, usize) {
        let col = usize::from(pos.x.saturating_sub(self.text.x));
        let line = if pos.y >= self.text.y {
            Some(self.top + usize::from(pos.y - self.text.y))
        } else {
            self.top.checked_sub(usize::from(self.text.y - pos.y))
        };
        line.map_or((0, 0), |line| (line, col))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FieldAction {
    Copy,
    Cut,
    SelectAll,
}

impl FieldAction {
    fn label(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Cut => "cut",
            Self::SelectAll => "select all",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FieldMenu {
    at: Position,
    field: TextField,
    actions: Vec<(FieldAction, bool)>,
}

impl FieldMenu {
    fn labels(&self) -> Vec<&'static str> {
        self.actions.iter().map(|(action, _)| action.label()).collect()
    }
}

impl App {
    fn text_field(&mut self, field: TextField, area: Rect) -> Option<(FieldArea, &mut Editor)> {
        match field {
            TextField::Todo => self.todo_text(area),
            TextField::Tour => self.tour_text(area),
            TextField::Queue(control) => self.queue_text(area, control),
            TextField::Flows => self.flows_text_field(area),
            TextField::StartPrompt => self.start_text(area),
            TextField::Setup(field) => self.project_text(area, field),
            TextField::SharePaste => self.share_text(area),
            TextField::Name => self.name_text(area),
        }
    }

    pub(super) fn focused_field(&self) -> Option<TextField> {
        match &self.overlay {
            Some(Overlay::Queue(form)) => {
                (!form.busy && form.opened().is_none() && form.editor_of(form.focus).is_some())
                    .then_some(TextField::Queue(form.focus))
            }
            Some(Overlay::Flows { dialog, .. }) => dialog.text().map(|_| TextField::Flows),
            Some(Overlay::StartFlow { start, .. }) => start.typing_prompt().then_some(TextField::StartPrompt),
            Some(Overlay::ProjectSettings(form)) => Some(TextField::Setup(form.focus)),
            Some(Overlay::ShareSettings(dialog)) => dialog.paste_editor().map(|_| TextField::SharePaste),
            Some(overlay) if overlay.takes_input() => Some(TextField::Name),
            None if self.tour_typing() => Some(TextField::Tour),
            None if self.todo_typing() => Some(TextField::Todo),
            Some(_) | None => None,
        }
    }

    fn field_candidates(&self) -> Vec<TextField> {
        match &self.overlay {
            Some(Overlay::Queue(form)) if !form.busy && form.opened().is_none() => form
                .controls()
                .into_iter()
                .filter(|&control| form.editor_of(control).is_some())
                .map(TextField::Queue)
                .collect(),
            Some(Overlay::Flows { .. }) => vec![TextField::Flows],
            Some(Overlay::StartFlow { start, .. }) if !start.waiting && !start.accounts_open() => {
                vec![TextField::StartPrompt]
            }
            Some(Overlay::ProjectSettings(_)) => {
                vec![TextField::Setup(SettingsField::Setup), TextField::Setup(SettingsField::Teardown)]
            }
            Some(Overlay::ShareSettings(_)) => vec![TextField::SharePaste],
            Some(overlay) if overlay.takes_input() => vec![TextField::Name],
            None if self.screen.side == Some(Side::Todo) => vec![TextField::Todo],
            Some(_) | None => Vec::new(),
        }
    }

    pub(super) fn field_at(&mut self, pos: Position, area: Rect) -> Option<TextField> {
        if self.popup().is_some() {
            return None;
        }
        if let Some(card) = self.tour_card(area) {
            let layout = ui::tour_layout(area, &card);
            if layout.area.contains(pos) {
                return (layout.field.contains(pos) && card.field.is_some()).then_some(TextField::Tour);
            }
        }
        self.field_candidates()
            .into_iter()
            .find(|&field| self.text_field(field, area).is_some_and(|(at, _)| at.text.contains(pos)))
    }

    pub(super) fn field_pressed(&mut self, field: TextField, pos: Position, area: Rect) {
        if self.focused_field() != Some(field) {
            return;
        }
        if self.screen.clicks.field_pressed(field, pos, Instant::now()) {
            if let Some((_, editor)) = self.text_field(field, area) {
                editor.select_word();
            }
        } else {
            self.screen.drag = Some(Drag::Text(field));
        }
    }

    pub(super) fn drag_field(&mut self, field: TextField, ev: MouseEvent, area: Rect) {
        let dragging = ev.kind == MouseEventKind::Drag(MouseButton::Left);
        let Some((at, editor)) = self.text_field(field, area) else {
            self.screen.drag = None;
            return;
        };
        if !dragging {
            editor.settle();
            self.screen.drag = None;
            return;
        }
        let (line, col) = at.spot(Position::new(ev.column, ev.row));
        editor.drag_to(at.width, line, col);
        if at.scrolls {
            editor.reveal(at.width, usize::from(at.text.height));
        }
    }

    pub(super) fn field_right_press(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> bool {
        if ev.kind != MouseEventKind::Down(MouseButton::Right) {
            return false;
        }
        let Some(field) = self.field_at(pos, area) else { return false };
        let inside = self.focused_field() == Some(field)
            && self.text_field(field, area).is_some_and(|(at, editor)| {
                let (line, col) = at.spot(pos);
                editor.selected_at(at.width, line, col)
            });
        if !inside {
            self.click_at(pos, area);
        }
        self.open_field_menu(field, pos, area);
        true
    }

    fn open_field_menu(&mut self, field: TextField, at: Position, area: Rect) {
        if self.focused_field() != Some(field) {
            return;
        }
        let Some((_, editor)) = self.text_field(field, area) else { return };
        let (selected, filled) = (editor.selection().is_some(), !editor.is_empty());
        let actions =
            vec![(FieldAction::Copy, selected), (FieldAction::Cut, selected), (FieldAction::SelectAll, filled)];
        self.screen.field_menu = Some(FieldMenu { at, field, actions });
    }

    pub(super) fn field_menu_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> bool {
        let Some(menu) = self.screen.field_menu.take() else { return false };
        if !matches!(ev.kind, MouseEventKind::Down(_)) {
            self.screen.field_menu = Some(menu);
            return true;
        }
        let shown = ui::menu_area(area, menu.at, &menu.labels());
        let picked = ui::menu_hit(shown, menu.actions.len(), pos).map(|i| menu.actions[i]);
        if let (MouseEventKind::Down(MouseButton::Left), Some((action, true))) = (ev.kind, picked) {
            self.run_field_action(menu.field, action, area);
        }
        true
    }

    fn run_field_action(&mut self, field: TextField, action: FieldAction, area: Rect) {
        if self.focused_field() != Some(field) {
            return;
        }
        let Some((at, editor)) = self.text_field(field, area) else { return };
        let copied = match action {
            FieldAction::Copy => editor.selected_text(),
            FieldAction::Cut => {
                let cut = editor.cut();
                if at.scrolls {
                    editor.reveal(at.width, usize::from(at.text.height));
                }
                cut
            }
            FieldAction::SelectAll => {
                editor.select_all();
                None
            }
        };
        if let Some(text) = copied {
            self.copy(&text);
        }
        if action == FieldAction::Cut && field == TextField::StartPrompt {
            self.prompt_edited();
        }
    }

    pub(super) fn field_menu_view(&self) -> Option<ui::FieldMenu> {
        let menu = self.screen.field_menu.as_ref().filter(|menu| self.focused_field() == Some(menu.field))?;
        Some(ui::FieldMenu {
            at: menu.at,
            items: menu.labels().into_iter().map(String::from).collect(),
            disabled: menu.actions.iter().map(|&(_, enabled)| !enabled).collect(),
        })
    }

    pub(super) fn close_field_menu_on_key(&mut self, escape: bool) -> bool {
        self.screen.field_menu.take().is_some() && escape
    }
}
