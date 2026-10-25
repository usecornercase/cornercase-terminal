use std::ops::ControlFlow;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::interaction::Drag;
use super::layout::{List, Side};
use super::overlay::Overlay;
use super::toast::NOT_READING;
use super::toast::Toast;
use crate::error::Result;
use crate::messenger::Via;
use crate::project::search::Finder;
use crate::ui::{self};

pub(super) const WHEEL_ROWS: isize = 3;

impl App {
    fn stop_dragging(&mut self) -> bool {
        match &self.screen.drag {
            Some(Drag::Border(_) | Drag::Divider { .. } | Drag::Row(_) | Drag::Pane(_) | Drag::Footer(_)) => {
                self.screen.drag = None;
                true
            }
            Some(Drag::Selection { .. } | Drag::Link(_) | Drag::Lines { .. } | Drag::Text(_)) | None => false,
        }
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent, area: Rect) -> Result<()> {
        if self.popup_key(key) || self.close_field_menu_on_key(key.code == KeyCode::Esc) {
            return Ok(());
        }
        if key.code == KeyCode::Esc && self.stop_dragging() {
            return Ok(());
        }
        match &self.overlay {
            Some(Overlay::Keys(group)) => return self.keys_key(*group, key, area),
            Some(Overlay::Menu { .. }) | None if self.is_prefix(key) => {
                self.stop_dragging();
                self.overlay = Some(Overlay::Keys(None));
            }
            Some(Overlay::Menu { .. }) if key.code == KeyCode::Esc => self.overlay = None,
            None if self.screen.nav.is_some() && key.code == KeyCode::Esc => self.screen.nav = None,
            Some(Overlay::Picker { .. }) => return self.picker_key(key, area),
            Some(Overlay::Branches(_)) => self.branches_key(key, area),
            Some(Overlay::Queue(_)) => self.queue_key(key, area),
            Some(Overlay::Issues(_)) => return self.issues_key(key, area),
            Some(Overlay::Settings(_)) => self.settings_key(key, area),
            Some(Overlay::ShareSettings(_)) => self.share_settings_key(key, area),
            Some(Overlay::Flows { .. }) => self.flows_key(key, area),
            Some(Overlay::StartFlow { .. }) => self.start_flow_key(key, area),
            Some(Overlay::ProjectSettings(_)) => self.project_settings_key(key, area),
            Some(Overlay::Worktrees(_) | Overlay::Ports(_)) => self.listing_key(key, area),
            Some(Overlay::CloseAgent(_)) => self.close_agent_key(key, area),
            Some(Overlay::CloseAgents(_)) => self.close_agents_key(key, area),
            Some(Overlay::Usage) => {
                self.usage_key(key);
            }
            Some(Overlay::Search(_)) => self.search_key(key, area),
            None if self.tour_typing() => self.tour_key(key, area),
            None if self.todo_typing() => self.todo_key(key, area),
            None if self.files_typing() => self.files_key(key, area),
            None if self.filtering() => self.filter_key(key),
            None if key.code == KeyCode::Esc && self.file_full_screen() => self.panels.files.set_full_screen(false),
            None if key.code == KeyCode::Esc && self.panel_shown() => self.close_panel(),
            Some(Overlay::Menu { .. }) | None => self.forward_key(key),
            Some(_) => return self.form_key(key, area),
        }
        Ok(())
    }

    pub(super) fn forward_key(&mut self, key: KeyEvent) {
        if self.holds_input() {
            return;
        }
        let Some(term) = self.term_mut() else { return };
        term.emulator.scroll_to_bottom();
        let bytes = term.emulator.encode_key(key);
        term.typed_key(key);
        if !bytes.is_empty() && !term.write(&bytes) {
            self.messages.toast = Some(Toast::new(NOT_READING, ui::ToastIcon::Bug));
        }
    }

    pub(super) fn handle_mouse(&mut self, ev: MouseEvent, area: Rect) -> Result<()> {
        let areas = self.layout(area).shown(self.screen.nav);
        let pos = Position::new(ev.column, ev.row);
        self.screen.hover = Some(pos);
        let field = if ev.kind == MouseEventKind::Down(MouseButton::Left) { self.field_at(pos, area) } else { None };
        let routed = self.route_mouse(ev, pos, &areas, area);
        if let Some(field) = field {
            self.field_pressed(field, pos, area);
        }
        match routed {
            ControlFlow::Break(done) => done,
            ControlFlow::Continue(()) => Ok(()),
        }
    }

    pub(super) fn click_at(&mut self, pos: Position, area: Rect) {
        let areas = self.layout(area).shown(self.screen.nav);
        for kind in [MouseEventKind::Down(MouseButton::Left), MouseEventKind::Up(MouseButton::Left)] {
            let ev = MouseEvent { kind, column: pos.x, row: pos.y, modifiers: KeyModifiers::NONE };
            _ = self.route_mouse(ev, pos, &areas, area);
        }
    }

    fn route_mouse(&mut self, ev: MouseEvent, pos: Position, areas: &ui::Areas, area: Rect) -> ControlFlow<Result<()>> {
        self.floating_mouse(ev, pos, areas, area)?;
        self.side_mouse(ev, pos, areas, area)?;
        self.chrome_mouse(ev, pos, areas, area)?;
        self.column_mouse(ev, pos, areas, area)?;
        ControlFlow::Break(self.main_mouse(ev, pos, areas, area))
    }

    fn floating_mouse(
        &mut self,
        ev: MouseEvent,
        pos: Position,
        areas: &ui::Areas,
        area: Rect,
    ) -> ControlFlow<Result<()>> {
        let modal = self.popup().is_some();
        if self.continue_drag(ev, areas, area)
            || self.field_menu_mouse(ev, pos, area)
            || (!modal
                && (self.star_mouse(ev, pos, area)
                    || self.field_right_press(ev, pos, area)
                    || self.tour_mouse(ev, pos, area)
                    || self.toast_mouse(ev, pos, area)))
        {
            return ControlFlow::Break(Ok(()));
        }
        if self.overlay.is_some() {
            return ControlFlow::Break(self.overlay_mouse(ev, pos, area));
        }
        ControlFlow::Continue(())
    }

    fn side_mouse(&mut self, ev: MouseEvent, pos: Position, areas: &ui::Areas, area: Rect) -> ControlFlow<Result<()>> {
        let in_panel = self.panel_shown() && areas.changes.contains(pos);
        if matches!(ev.kind, MouseEventKind::Down(_)) && !in_panel {
            self.panels.changes.unfocus();
            self.commit_todo();
            self.panels.files.unfocus();
        }
        if in_panel && self.screen.side.is_some() {
            return ControlFlow::Break(self.panel_mouse(ev, pos, areas.changes, area));
        }
        if let Some(delta) = wheel(ev.kind)
            && self.scroll_column(areas, pos, delta)
        {
            return ControlFlow::Break(Ok(()));
        }
        ControlFlow::Continue(())
    }

    fn chrome_mouse(
        &mut self,
        ev: MouseEvent,
        pos: Position,
        areas: &ui::Areas,
        area: Rect,
    ) -> ControlFlow<Result<()>> {
        let left = ev.kind == MouseEventKind::Down(MouseButton::Left);
        if let Some(border) = areas.border_hit(pos) {
            if left {
                self.press_border(border, Instant::now());
            }
            return ControlFlow::Break(Ok(()));
        }
        if self.button_mouse(areas, pos, left) {
            return ControlFlow::Break(Ok(()));
        }
        if areas.agents.contains(pos) {
            self.agents_mouse(areas, pos, ev.kind, area);
            return ControlFlow::Break(Ok(()));
        }
        ControlFlow::Continue(())
    }

    fn button_mouse(&mut self, areas: &ui::Areas, pos: Position, left: bool) -> bool {
        if areas.search_button.contains(pos) {
            if left {
                self.overlay = Some(Overlay::Search(Finder::default()));
            }
            return true;
        }
        if self.panel_buttons(areas, pos, left) {
            return true;
        }
        if areas.bar.contains(pos) {
            if left {
                self.toggle_nav();
            }
            return true;
        }
        let navs = [(areas.back, ui::Nav::Projects), (areas.agents_button, ui::Nav::Agents)];
        let Some(nav) = navs.into_iter().find_map(|(r, nav)| r.contains(pos).then_some(nav)) else { return false };
        if left {
            self.screen.nav = Some(nav);
        }
        true
    }

    fn column_mouse(
        &mut self,
        ev: MouseEvent,
        pos: Position,
        areas: &ui::Areas,
        area: Rect,
    ) -> ControlFlow<Result<()>> {
        let left = ev.kind == MouseEventKind::Down(MouseButton::Left);
        match self.activity_mouse(areas, pos, left, area) {
            Err(e) => return ControlFlow::Break(Err(e)),
            Ok(true) => return ControlFlow::Break(Ok(())),
            Ok(false) => {}
        }
        if self.plugin_row_mouse(areas.plugins, pos, left) {
            return ControlFlow::Break(Ok(()));
        }
        if let Some(item) = Self::footer_hit(areas, pos) {
            return ControlFlow::Break(if left { self.activity_click(item, pos, area) } else { Ok(()) });
        }
        if areas.list.contains(pos) {
            return ControlFlow::Break(self.list_mouse(areas, pos, ev.kind, area));
        }
        if areas.issues.contains(pos) {
            return ControlFlow::Break(self.issues_column_mouse(left, area));
        }
        if areas.workspaces.contains(pos) {
            return ControlFlow::Break(self.workspaces_mouse(ev, pos, areas, area));
        }
        if self.screen.nav.is_some() && areas.compact() {
            ControlFlow::Break(Ok(()))
        } else {
            ControlFlow::Continue(())
        }
    }

    fn issues_column_mouse(&mut self, left: bool, area: Rect) -> Result<()> {
        if left && self.issues_available() {
            self.screen.nav = None;
            return self.open_issues(area);
        }
        Ok(())
    }

    fn workspaces_mouse(&mut self, ev: MouseEvent, pos: Position, areas: &ui::Areas, area: Rect) -> Result<()> {
        let list = areas.workspaces_list;
        if !list.contains(pos) {
            return Ok(());
        }
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) => return self.click_workspaces(list, areas.pitch, pos, area),
            MouseEventKind::Down(MouseButton::Right) => self.open_workspace_menu(list, areas.pitch, pos),
            _ => {}
        }
        Ok(())
    }

    fn panel_mouse(&mut self, ev: MouseEvent, pos: Position, panel: Rect, area: Rect) -> Result<()> {
        let Some(side) = self.screen.side else { return Ok(()) };
        match side {
            Side::Todo => self.todo_mouse(ev, pos, panel, area),
            Side::Files => return self.files_mouse(ev, pos, panel, area),
            Side::Changes => return self.changes_mouse(ev, pos, panel, area),
            Side::Flow => self.flow_mouse(ev, pos, panel),
        }
        Ok(())
    }

    fn main_mouse(&mut self, ev: MouseEvent, pos: Position, areas: &ui::Areas, area: Rect) -> Result<()> {
        if areas.tab_bar.contains(pos) {
            return self.tab_bar_mouse(areas, pos, ev.kind, area);
        }
        if self.tab().is_none() {
            return self.empty_pane_mouse(ev.kind, pos, areas.pane, area);
        }
        self.pane_mouse(ev, pos, areas.pane);
        Ok(())
    }

    fn list_mouse(&mut self, areas: &ui::Areas, pos: Position, kind: MouseEventKind, area: Rect) -> Result<()> {
        match (kind, areas.tree) {
            (MouseEventKind::Down(MouseButton::Left), true) => return self.click_tree(areas.list, pos, area),
            (MouseEventKind::Down(MouseButton::Right), true) => self.open_tree_menu(areas.list, pos),
            (MouseEventKind::Down(MouseButton::Left), false) => self.click_projects(areas.list, areas.pitch, pos, area),
            (MouseEventKind::Down(MouseButton::Right), false) => self.open_project_menu(areas.list, areas.pitch, pos),
            _ => {}
        }
        Ok(())
    }

    fn panel_buttons(&mut self, areas: &ui::Areas, pos: Position, left: bool) -> bool {
        let changes = match self.changes_label() {
            Some(_) if areas.compact() => areas.changes_button,
            Some(label) => ui::changes_button(areas.issues, &label),
            None => Rect::default(),
        };
        let dot = self.config.todo_dot && self.panels.todos.pending() > 0;
        let todo = if areas.compact() || self.project().is_some() { areas.todo_with_dot(dot) } else { Rect::default() };
        let files = if self.project().is_some() { areas.files_button } else { Rect::default() };
        let toggle: fn(&mut Self) = if changes.contains(pos) {
            Self::toggle_changes
        } else if todo.contains(pos) {
            Self::toggle_todo
        } else if files.contains(pos) {
            Self::toggle_files
        } else {
            return false;
        };
        if left {
            toggle(self);
        }
        true
    }

    fn continue_drag(&mut self, ev: MouseEvent, areas: &ui::Areas, area: Rect) -> bool {
        match self.screen.drag.clone() {
            Some(Drag::Border(border)) => self.drag_border(border, ev, area),
            Some(Drag::Divider { tab, path }) => self.drag_divider(tab, &path, ev, areas.pane),
            Some(Drag::Pane(drag)) => self.drag_pane(drag, ev, areas.pane, area),
            Some(Drag::Lines { .. }) => self.drag_lines(ev, areas.changes),
            Some(Drag::Selection { term, link }) => {
                let pane = self.terminal_rect(areas.pane, term).unwrap_or(areas.pane);
                self.drag_selection(term, link, ev, pane);
            }
            Some(Drag::Footer(pane)) => self.drag_footer(pane, ev, areas.pane),
            Some(Drag::Row(drag)) => self.drag_row(drag, ev, areas, area),
            Some(Drag::Text(field)) => self.drag_field(field, ev, area),
            Some(Drag::Link(_)) | None => return false,
        }
        true
    }

    pub(super) fn scroll_column(&mut self, areas: &ui::Areas, pos: Position, delta: isize) -> bool {
        let items = if areas.pitch > 1 { delta.signum() } else { delta };
        if areas.tab_bar.contains(pos) {
            self.scroll.set(List::TabBar, self.tab_strip(areas).scrolled(delta.signum()));
            true
        } else if areas.agents.contains(pos) {
            self.scroll.set(List::Agents, self.agent_layout(areas).scrolled(items));
            true
        } else if areas.sidebar.contains(pos) {
            self.scroll.set(List::Projects, self.sidebar_layout(areas).scrolled(items));
            true
        } else if areas.workspaces.contains(pos) {
            let tabs = self.tab_lines();
            let layout =
                ui::workspace_layout(areas.workspaces_list, areas.pitch, &tabs, self.scroll.of(List::Workspaces));
            self.scroll.set(List::Workspaces, layout.scrolled(items));
            true
        } else {
            false
        }
    }

    fn toggle_nav(&mut self) {
        if self.screen.nav.is_none() {
            self.show_side(None);
        }
        self.screen.nav = match self.screen.nav {
            Some(_) => None,
            None if self.project().is_some() => Some(ui::Nav::Workspaces),
            None => Some(ui::Nav::Projects),
        };
        self.scroll.follow_again();
    }

    pub(super) fn activity_shown(&self) -> ui::ActivityShown {
        ui::ActivityShown {
            issues: self.issues_available(),
            changes: self.changes_label().is_some(),
            files: self.project().is_some(),
            flow: self.has_run(),
            plugins: u16::try_from(self.buttoned_plugins().count()).unwrap_or(u16::MAX),
        }
    }

    fn activity_mouse(&mut self, areas: &ui::Areas, pos: Position, left: bool, area: Rect) -> Result<bool> {
        let Some(item) = ui::activity_hit(areas.activity, self.activity_shown(), pos) else { return Ok(false) };
        if left {
            self.activity_click(item, pos, area)?;
        }
        Ok(true)
    }

    fn activity_click(&mut self, item: ui::ActivityItem, pos: Position, area: Rect) -> Result<()> {
        self.screen.nav = None;
        match item {
            ui::ActivityItem::Issues => return self.open_issues(area),
            ui::ActivityItem::Changes => self.toggle_changes(),
            ui::ActivityItem::Files => self.toggle_files(),
            ui::ActivityItem::Todo => self.toggle_todo(),
            ui::ActivityItem::Worktrees => self.open_worktrees(None),
            ui::ActivityItem::Flow => self.toggle_flow(),
            ui::ActivityItem::Plugin(plugin) => self.click_plugin_button(plugin, pos),
            ui::ActivityItem::Help => self.open_help(pos),
            ui::ActivityItem::More => self.open_more(pos),
        }
        Ok(())
    }

    pub(super) fn open_remote_menu(&mut self, at: Position) {
        let away = Via::ALL.into_iter().map(super::menus::MenuAction::Away);
        let actions = std::iter::once(super::menus::MenuAction::Browser).chain(away).collect();
        self.overlay = Some(Overlay::Menu { at, actions });
    }

    fn footer_hit(areas: &ui::Areas, pos: Position) -> Option<ui::ActivityItem> {
        [
            (ui::help_button(areas.more), ui::ActivityItem::Help),
            (ui::more_button(areas.more), ui::ActivityItem::More),
            (areas.worktrees, ui::ActivityItem::Worktrees),
        ]
        .into_iter()
        .find_map(|(r, item)| r.contains(pos).then_some(item))
    }

    pub(super) fn handle_paste(&mut self, text: &str, area: Rect) {
        self.screen.field_menu = None;
        if self.popup_paste(text) {
            return;
        }
        if matches!(self.overlay, Some(Overlay::Keys(_))) {
            self.overlay = None;
        }
        if self.overlay.is_some() {
            self.overlay_paste(text, area);
            return;
        }
        if self.typing_paste(text, area) || self.holds_input() {
            return;
        }
        if let Some(term) = self.term_mut() {
            term.emulator.scroll_to_bottom();
            term.typed_paste(text);
            if !term.paste(text) {
                self.messages.toast = Some(Toast::new(NOT_READING, ui::ToastIcon::Bug));
            }
        }
    }

    fn overlay_paste(&mut self, text: &str, area: Rect) {
        if matches!(&self.overlay, Some(Overlay::Settings(s)) if s.idle()) {
            self.share_pasted(text, area);
            return;
        }
        let clean = || text.chars().filter(|c| !c.is_control());
        match &mut self.overlay {
            Some(Overlay::Picker { picker, .. }) => clean().for_each(|c| picker.push(c)),
            Some(Overlay::Branches(picker)) => clean().for_each(|c| picker.push(c)),
            Some(Overlay::Search(search)) => {
                clean().for_each(|c| search.field.push(c));
                self.ask_sessions(Instant::now());
            }
            Some(Overlay::Queue(form)) => {
                let (focus, idle) = (form.focus, !form.busy && form.opened().is_none());
                if let Some(editor) = form.editor(focus).filter(|_| idle) {
                    editor.insert(text);
                }
            }
            Some(Overlay::Issues(b)) => b.paste(text),
            Some(Overlay::Settings(s)) => s.paste(text),
            Some(Overlay::ShareSettings(_)) => self.share_settings_paste(text, area),
            Some(Overlay::Flows { .. }) => self.flows_paste(text, area),
            Some(Overlay::StartFlow { .. }) => self.start_flow_paste(text, area),
            Some(Overlay::ProjectSettings(_)) => self.project_settings_paste(text, area),
            Some(Overlay::Worktrees(_)) => self.worktrees_paste(text),
            Some(_) => self.name_paste(text, area),
            None => {}
        }
    }

    fn typing_paste(&mut self, text: &str, area: Rect) -> bool {
        if self.tour_typing() {
            self.tour_paste(text);
            return true;
        }
        if self.todo_typing() {
            self.todo_paste(text);
            return true;
        }
        if self.files_typing() {
            self.files_paste(text, area);
            return true;
        }
        if self.filtering() {
            text.chars().filter(|c| !c.is_control()).for_each(|c| self.panels.changes.push_filter(c));
            self.panels.changes.scroll_to_top();
            return true;
        }
        false
    }
}

pub(super) fn wheel(kind: MouseEventKind) -> Option<isize> {
    match kind {
        MouseEventKind::ScrollUp => Some(-WHEEL_ROWS),
        MouseEventKind::ScrollDown => Some(WHEEL_ROWS),
        _ => None,
    }
}

pub(super) enum ListPointer {
    Wheel(isize),
    Press,
}

pub(super) fn list_pointer(kind: MouseEventKind) -> Option<ListPointer> {
    match (wheel(kind), kind) {
        (Some(delta), _) => Some(ListPointer::Wheel(delta)),
        (None, MouseEventKind::Down(MouseButton::Left)) => Some(ListPointer::Press),
        _ => None,
    }
}

pub(super) fn wheel_span((rows, body): (usize, Rect), confirm_lines: Option<usize>) -> (usize, usize) {
    confirm_lines.map_or((rows, 0), |lines| (usize::from(body.height), lines))
}
