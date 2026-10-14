use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::interaction::Drag;
use super::layout::{Focus, Side};
use super::overlay::Overlay;
use super::toast::NOT_READING;
use super::toast::Toast;
use crate::error::Result;
use crate::list::Search;
use crate::ui::{self};

pub(super) const WHEEL_ROWS: isize = 3;

impl App {
    pub(super) fn handle_key(&mut self, key: KeyEvent, area: Rect) -> Result<()> {
        let dragging = |app: &mut Self| match &app.drag {
            Some(Drag::Border(_) | Drag::Divider { .. } | Drag::Row(_) | Drag::Pane(_)) => {
                app.drag = None;
                true
            }
            Some(Drag::Selection { .. } | Drag::Link(_) | Drag::Lines { .. }) | None => false,
        };
        if key.code == KeyCode::Esc && dragging(self) {
            return Ok(());
        }
        match &self.overlay {
            Some(Overlay::Keys(group)) => return self.keys_key(*group, key, area),
            Some(Overlay::Menu { .. }) | None if self.is_prefix(key) => {
                dragging(self);
                self.overlay = Some(Overlay::Keys(None));
            }
            Some(Overlay::Menu { .. }) if key.code == KeyCode::Esc => self.overlay = None,
            None if self.nav.is_some() && key.code == KeyCode::Esc => self.nav = None,
            Some(Overlay::Picker { .. }) => return self.picker_key(key, area),
            Some(Overlay::Branches(_)) => self.branches_key(key, area),
            Some(Overlay::Queue(_)) => {
                self.queue_key(key);
            }
            Some(Overlay::Issues(_)) => return self.issues_key(key, area),
            Some(Overlay::Settings(_)) => self.settings_key(key, area),
            Some(Overlay::Usage) => {
                self.usage_key(key);
            }
            Some(Overlay::Search(_)) => self.search_key(key, area),
            None if self.todo_typing() => self.todo_key(key, area),
            None if self.files_typing() => self.files_key(key, area),
            None if self.filtering() => self.filter_key(key),
            None if key.code == KeyCode::Esc && self.panel_shown() => self.close_panel(),
            Some(Overlay::Menu { .. }) | None => self.forward_key(key),
            Some(_) => return self.form_key(key, area),
        }
        Ok(())
    }

    pub(super) fn forward_key(&mut self, key: KeyEvent) {
        let Some(term) = self.term_mut() else { return };
        term.emulator.scroll_to_bottom();
        let bytes = term.emulator.encode_key(key);
        if !bytes.is_empty() && !term.write(&bytes) {
            self.toast = Some(Toast::new(NOT_READING, ui::ToastIcon::Bug));
        }
    }

    pub(super) fn handle_mouse(&mut self, ev: MouseEvent, area: Rect) -> Result<()> {
        let areas = self.layout(area).shown(self.nav);
        let pos = Position::new(ev.column, ev.row);
        self.hover = Some(pos);

        if self.continue_drag(ev, &areas, area) {
            return Ok(());
        }
        if self.toast_mouse(ev, pos, area) {
            return Ok(());
        }
        if self.overlay.is_some() {
            return self.overlay_mouse(ev, pos, area);
        }
        let in_panel = self.panel_shown() && areas.changes.contains(pos);
        if matches!(ev.kind, MouseEventKind::Down(_)) && !in_panel {
            if let Some(filter) = &mut self.changes.filter {
                filter.focused = false;
            }
            self.commit_todo();
            self.files.unfocus();
        }
        if in_panel {
            return self.side_mouse(ev, pos, areas.changes, area);
        }
        if let Some(delta) = wheel(ev.kind)
            && self.scroll_column(&areas, pos, delta)
        {
            return Ok(());
        }

        let left = ev.kind == MouseEventKind::Down(MouseButton::Left);
        let right = ev.kind == MouseEventKind::Down(MouseButton::Right);
        if let Some(border) = areas.border_hit(pos) {
            if left {
                self.press_border(border, Instant::now());
            }
            return Ok(());
        }
        if areas.search_button.contains(pos) {
            if left {
                self.overlay = Some(Overlay::Search(Search::default()));
            }
            return Ok(());
        }
        if self.panel_buttons(&areas, pos, left) {
            return Ok(());
        }
        if areas.bar.contains(pos) {
            if left {
                self.toggle_nav();
            }
            return Ok(());
        }
        if let Some(nav) = [(areas.back, ui::Nav::Projects), (areas.agents_button, ui::Nav::Agents)]
            .into_iter()
            .find_map(|(r, nav)| r.contains(pos).then_some(nav))
        {
            if left {
                self.nav = Some(nav);
            }
            return Ok(());
        }
        if areas.agents.contains(pos) {
            self.agents_mouse(&areas, pos, ev.kind, area);
            return Ok(());
        }
        if self.activity_mouse(&areas, pos, left, area)? || self.footer_mouse(&areas, pos, left) {
            return Ok(());
        }
        if areas.list.contains(pos) {
            return self.list_mouse(&areas, pos, ev.kind, area);
        }
        if areas.issues.contains(pos) {
            if left && self.issues_available() {
                self.nav = None;
                return self.open_issues(area);
            }
            return Ok(());
        }
        if areas.workspaces.contains(pos) {
            if left && areas.workspaces_list.contains(pos) {
                return self.click_workspaces(areas.workspaces_list, areas.pitch, pos, area);
            }
            if right && areas.workspaces_list.contains(pos) {
                self.open_workspace_menu(areas.workspaces_list, areas.pitch, pos);
            }
            return Ok(());
        }
        if self.nav.is_some() && areas.compact() {
            return Ok(());
        }
        self.main_mouse(ev, pos, &areas, area)
    }

    fn side_mouse(&mut self, ev: MouseEvent, pos: Position, panel: Rect, area: Rect) -> Result<()> {
        match self.side {
            Some(Side::Queue) => self.queue_mouse(ev, pos, panel, area),
            Some(Side::Todo) => self.todo_mouse(ev, pos, panel, area),
            Some(Side::Files) => return self.files_mouse(ev, pos, panel, area),
            Some(Side::Changes) => return self.changes_mouse(ev, pos, panel, area),
            None => {}
        }
        Ok(())
    }

    fn main_mouse(&mut self, ev: MouseEvent, pos: Position, areas: &ui::Areas, area: Rect) -> Result<()> {
        if areas.tab_bar.contains(pos) {
            return self.tab_bar_mouse(areas, pos, ev.kind, area);
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
        let todo = if areas.compact() || self.project().is_some() { areas.todo_button } else { Rect::default() };
        let files = if self.project().is_some() { areas.files_button } else { Rect::default() };
        let toggle: fn(&mut Self) = if changes.contains(pos) {
            Self::toggle_changes
        } else if ui::queue_button(areas, self.queued.queue.pending()).contains(pos) {
            Self::toggle_queue
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
        match self.drag.clone() {
            Some(Drag::Border(border)) => self.drag_border(border, ev, area),
            Some(Drag::Divider { tab, path }) => self.drag_divider(tab, &path, ev, areas.pane),
            Some(Drag::Pane(drag)) => self.drag_pane(drag, ev, areas.pane, area),
            Some(Drag::Lines { workspace, path, anchor }) => {
                self.drag_lines(workspace, &path, anchor, ev, areas.changes);
            }
            Some(Drag::Selection { term, link }) => {
                let pane = self.tab().and_then(|t| t.rect(areas.pane, term)).unwrap_or(areas.pane);
                self.drag_selection(term, link, ev, pane);
            }
            Some(Drag::Row(drag)) => self.drag_row(drag, ev, areas, area),
            Some(Drag::Link(_)) | None => return false,
        }
        true
    }

    pub(super) fn scroll_column(&mut self, areas: &ui::Areas, pos: Position, delta: isize) -> bool {
        let items = if areas.pitch > 1 { delta.signum() } else { delta };
        if areas.tab_bar.contains(pos) {
            self.scroll.tab_bar = self.tab_strip(areas).scrolled(delta.signum());
            true
        } else if areas.agents.contains(pos) {
            self.scroll.agents = self.agent_layout(areas).scrolled(items);
            true
        } else if areas.sidebar.contains(pos) {
            self.scroll.projects = self.sidebar_layout(areas).scrolled(items);
            true
        } else if areas.workspaces.contains(pos) {
            let tabs = self.tab_lines();
            let layout = ui::workspace_layout(areas.workspaces_list, areas.pitch, &tabs, self.scroll.workspaces);
            self.scroll.workspaces = layout.scrolled(items);
            true
        } else {
            false
        }
    }

    fn toggle_nav(&mut self) {
        if self.nav.is_none() {
            self.show_side(None);
        }
        self.nav = match self.nav {
            Some(_) => None,
            None if self.project().is_some() => Some(ui::Nav::Workspaces),
            None => Some(ui::Nav::Projects),
        };
        self.scroll.followed = Focus::default();
    }

    pub(super) fn activity_shown(&self) -> ui::ActivityShown {
        ui::ActivityShown {
            issues: self.issues_available(),
            changes: self.changes_label().is_some(),
            files: self.project().is_some(),
            update: self.update_label().is_some(),
            remote: self.remote_button().is_some(),
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
        self.nav = None;
        match item {
            ui::ActivityItem::Issues => return self.open_issues(area),
            ui::ActivityItem::Changes => self.toggle_changes(),
            ui::ActivityItem::Files => self.toggle_files(),
            ui::ActivityItem::Todo => self.toggle_todo(),
            ui::ActivityItem::Queue => self.toggle_queue(),
            ui::ActivityItem::Update => self.open_update(),
            ui::ActivityItem::Remote => self.open_remote_menu(pos),
            ui::ActivityItem::Usage => self.open_usage(),
            ui::ActivityItem::Settings => self.open_settings(),
            ui::ActivityItem::Quit => self.outbox.detach = true,
        }
        Ok(())
    }

    fn open_remote_menu(&mut self, at: Position) {
        let actions = vec![super::menus::MenuAction::Browser, super::menus::MenuAction::Away];
        self.overlay = Some(Overlay::Menu { at, actions });
    }

    fn footer_mouse(&mut self, areas: &ui::Areas, pos: Position, left: bool) -> bool {
        let update = self.update_label().is_some_and(|label| ui::update_button(areas.settings, &label).contains(pos));
        let remote = self.remote_button().is_some_and(|on| ui::remote_button(areas.usage, on).contains(pos));
        let hit = update || remote || [areas.quit, areas.usage, areas.settings].iter().any(|r| r.contains(pos));
        if hit && left {
            self.nav = None;
            if update {
                self.open_update();
            } else if remote {
                self.open_remote_menu(pos);
            } else if areas.quit.contains(pos) {
                self.outbox.detach = true;
            } else if areas.usage.contains(pos) {
                self.open_usage();
            } else {
                self.open_settings();
            }
        }
        hit
    }

    pub(super) fn handle_paste(&mut self, text: &str) {
        if matches!(self.overlay, Some(Overlay::Keys(_))) {
            self.overlay = None;
        }
        if let Some(Overlay::Picker { picker, .. }) = &mut self.overlay {
            text.chars().filter(|c| !c.is_control()).for_each(|c| picker.push(c));
            return;
        }
        if let Some(Overlay::Search(search)) = &mut self.overlay {
            text.chars().filter(|c| !c.is_control()).for_each(|c| search.push(c));
            return;
        }
        if let Some(Overlay::Branches(picker)) = &mut self.overlay {
            text.chars().filter(|c| !c.is_control()).for_each(|c| picker.push(c));
            return;
        }
        if let Some(Overlay::Queue(form)) = &mut self.overlay {
            if !form.busy {
                form.editor().insert(text);
            }
            return;
        }
        if let Some(Overlay::Issues(b)) = &mut self.overlay {
            b.paste(text);
            return;
        }
        if let Some(Overlay::Settings(s)) = &mut self.overlay {
            s.paste(text);
            return;
        }
        if let Some(overlay) = &mut self.overlay {
            if let Some(input) = overlay.input() {
                input.extend(text.chars().filter(|c| !c.is_control()));
            }
            return;
        }
        if self.todo_typing() {
            self.todo_paste(text);
            return;
        }
        if self.files_typing() {
            self.files_paste(text);
            return;
        }
        if self.filtering()
            && let Some(filter) = &mut self.changes.filter
        {
            text.chars().filter(|c| !c.is_control()).for_each(|c| filter.push(c));
            self.changes.scroll = 0;
            return;
        }
        if let Some(term) = self.term_mut() {
            term.emulator.scroll_to_bottom();
            if !term.paste(text) {
                self.toast = Some(Toast::new(NOT_READING, ui::ToastIcon::Bug));
            }
        }
    }
}

pub(super) fn wheel(kind: MouseEventKind) -> Option<isize> {
    match kind {
        MouseEventKind::ScrollUp => Some(-WHEEL_ROWS),
        MouseEventKind::ScrollDown => Some(WHEEL_ROWS),
        _ => None,
    }
}
