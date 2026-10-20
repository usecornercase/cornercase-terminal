use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use serde_json::Value;

use super::input::wheel;
use super::interaction::{Drag, Grab, RowDrag};
use super::layout::Side;
use super::menus::MenuAction;
use super::overlay::Overlay;
use super::toast::Undo;
use super::{App, Toast};
use crate::project::Hierarchy;
use crate::todo::editor::Editor;
use crate::todo::{Field, Removed};
use crate::ui;
use crate::ui::todo::{self as panel, Hit};

const DELETED: &str = "deleted";
const NO_AGENT: &str = "no agent is running";
pub(in crate::app) const CLIENTS: u64 = 1 << 61;

impl App {
    fn todo_area(&self, area: Rect) -> Rect {
        self.layout(area).shown(self.screen.nav).changes
    }

    pub(super) fn todo_view(&self, panel_area: Rect) -> panel::View {
        let field = self.panels.todo.field.as_ref();
        let items = self
            .panels
            .todos
            .items()
            .iter()
            .map(|item| panel::Item {
                id: item.id,
                text: item.text.clone(),
                done: item.done,
                editing: field.filter(|f| f.item == Some(item.id)).map(|f| f.editor.clone()),
            })
            .collect();
        let adding = field.filter(|f| f.item.is_none()).map(|f| f.editor.clone());
        let light = self.themes.light();
        let muted = ui::muted(self.themes.host());
        let mut view = panel::View {
            items,
            scroll: self.panels.todo.scroll,
            adding,
            light,
            muted,
            drag: None,
            compact: self.screen.drawn.compact(),
        };
        if let Some((drag, pos)) = self.row_drag().filter(|d| d.moved).zip(self.screen.hover)
            && let Grab::Todo(id) = drag.target
        {
            view.drag = Some((id, panel::drop(panel_area, &view, id, pos)));
        }
        view
    }

    pub(super) fn todo_typing(&self) -> bool {
        self.overlay.is_none() && self.screen.side == Some(Side::Todo) && self.panels.todo.field.is_some()
    }

    pub(super) fn toggle_todo(&mut self) {
        let side = (self.screen.side != Some(Side::Todo)).then_some(Side::Todo);
        self.show_side(side);
        self.screen.nav = None;
    }

    pub(super) fn close_todo(&mut self) {
        self.commit_todo();
    }

    pub(super) fn commit_todo(&mut self) {
        let Some(field) = self.panels.todo.field.take() else { return };
        let text = field.editor.text();
        match field.item {
            Some(id) => _ = self.panels.todos.edit(id, &text),
            None => _ = self.panels.todos.add(&text),
        }
    }

    fn submit_todo(&mut self, area: Rect) {
        let Some(field) = self.panels.todo.field.take() else { return };
        let text = field.editor.text();
        if let Some(id) = field.item {
            self.panels.todos.edit(id, &text);
        } else if let Some(id) = self.panels.todos.add(&text) {
            self.panels.todo.field = Some(Field::default());
            self.reveal_todo(id, area);
        }
    }

    fn reveal_todo(&mut self, id: u64, area: Rect) {
        let panel_area = self.todo_area(area);
        let view = self.todo_view(panel_area);
        if let Some(i) = view.items.iter().position(|i| i.id == id) {
            self.panels.todo.scroll = panel::rows(panel_area, &view).reveal(i);
        }
    }

    pub(super) fn todo_key(&mut self, key: KeyEvent, area: Rect) {
        let width = panel::text_width(self.todo_area(area));
        let Some(field) = &mut self.panels.todo.field else { return };
        let editor = &mut field.editor;
        match key.code {
            KeyCode::Esc if editor.is_empty() => self.close_panel(),
            KeyCode::Esc => self.panels.todo.field = None,
            KeyCode::Enter => self.submit_todo(area),
            KeyCode::Backspace => editor.backspace(),
            KeyCode::Delete => editor.delete(),
            KeyCode::Left => editor.left(),
            KeyCode::Right => editor.right(),
            KeyCode::Home => editor.home(),
            KeyCode::End => editor.end(),
            KeyCode::Up => editor.up(width),
            KeyCode::Down => editor.down(width),
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                editor.insert(c.encode_utf8(&mut [0; 4]));
            }
            _ => {}
        }
    }

    pub(super) fn todo_paste(&mut self, text: &str) {
        if let Some(field) = &mut self.panels.todo.field {
            field.editor.insert(text);
        }
    }

    pub(super) fn todo_mouse(&mut self, ev: MouseEvent, pos: Position, panel_area: Rect, area: Rect) {
        if let Some(delta) = wheel(ev.kind) {
            self.scroll_todo(panel_area, delta);
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let hit = panel::hit(panel_area, &self.todo_view(panel_area), pos);
        if !matches!(hit, Some(Hit::Field { .. })) {
            self.commit_todo();
        }
        match hit {
            Some(Hit::Close) => self.show_side(None),
            Some(Hit::ClearDone) => self.clear_done_todos(),
            Some(Hit::Add) => self.panels.todo.field = Some(Field::default()),
            Some(Hit::Field { line, col, .. }) => {
                if let Some(field) = &mut self.panels.todo.field {
                    field.editor.place(panel::text_width(panel_area), line, col);
                }
            }
            Some(Hit::Check(id)) => self.panels.todos.toggle(id),
            Some(Hit::Delete(id)) => self.delete_todo(id),
            Some(Hit::Send(id)) => self.ask_todo_agent(id, pos),
            Some(Hit::Item { id, row, .. }) => {
                let target = Grab::Todo(id);
                self.screen.drag =
                    Some(Drag::Row(RowDrag { target, row, moved: false, area, scrolled: None, fold: false }));
            }
            None => {}
        }
    }

    fn clear_done_todos(&mut self) {
        if let Some(removed) = self.panels.todos.clear_done() {
            self.removed_todos(format!("cleared {} done", removed.len()), removed);
        }
    }

    fn delete_todo(&mut self, id: u64) {
        if let Some(removed) = self.panels.todos.remove(id) {
            self.removed_todos(DELETED.into(), removed);
        }
    }

    fn ask_todo_agent(&mut self, todo: u64, at: Position) {
        let panes: Vec<u64> = self
            .agent_places()
            .into_iter()
            .map(|(p, w, t, i)| self.projects[p].workspaces[w].tabs[t].panes[i].id)
            .collect();
        if panes.is_empty() {
            self.messages.toast = Some(Toast::new(NO_AGENT, ui::ToastIcon::Bug));
            return;
        }
        let actions = panes.into_iter().map(|pane| MenuAction::SendTodo { todo, pane }).collect();
        self.overlay = Some(Overlay::Menu { at, actions });
    }

    fn todo_agent(&self, pane: u64) -> Option<(String, String, String)> {
        let (p, w, _) = self.projects.tab_of_pane(pane)?;
        let project = &self.projects[p];
        let term = self.projects.pane_by(pane)?;
        let place = if project.workspaces.len() > 1 {
            format!("{} › {}", self.project_label(project), project.workspaces[w].label())
        } else {
            self.project_label(project)
        };
        let icon = term.agent.status().map(ui::status_text).unwrap_or_default();
        Some((icon, place, term.agent.agent().unwrap_or_default().to_string()))
    }

    pub(super) fn todo_agent_label(&self, pane: u64) -> String {
        let base = |id: u64| self.todo_agent(id).map(|(icon, place, kind)| format!("{icon} {place} · {kind}"));
        let Some(label) = base(pane) else { return String::new() };
        let others: Vec<u64> = self
            .agent_places()
            .into_iter()
            .map(|(p, w, t, i)| self.projects[p].workspaces[w].tabs[t].panes[i].id)
            .filter(|id| *id != pane)
            .collect();
        let tabbed = |id: u64| {
            let (p, w, t) = self.projects.tab_of_pane(id)?;
            let tab = self.projects[p].workspaces[w].tabs[t].label(&self.config);
            base(id).map(|label| format!("{label} · {tab}"))
        };
        if !others.iter().any(|id| base(*id).as_ref() == Some(&label)) {
            return label;
        }
        let with_tab = tabbed(pane).unwrap_or(label);
        if others.iter().any(|id| tabbed(*id).as_ref() == Some(&with_tab)) {
            return format!("{with_tab} · pane {pane}");
        }
        with_tab
    }

    pub(super) fn send_todo_from_panel(&mut self, todo: u64, pane: u64, now: std::time::Instant) {
        let client = CLIENTS + self.panels.todo.next_client;
        self.panels.todo.next_client += 1;
        self.panels.todo.sending.insert(client, self.todo_agent_place(pane));
        self.send_todo(client, todo, pane, now);
    }

    fn todo_agent_place(&self, pane: u64) -> String {
        self.todo_agent(pane).map(|(_, place, kind)| format!("{kind} in {place}")).unwrap_or_default()
    }

    pub(in crate::app) fn todo_answered(&mut self, client: u64, reply: &Result<Value, String>) -> bool {
        let Some(place) = self.panels.todo.sending.remove(&client) else { return false };
        if place.is_empty() {
            return true;
        }
        self.messages.toast = Some(match reply {
            Ok(value) if value.get("unconfirmed").is_some() => {
                Toast::new(format!("sent to {place}, not confirmed: stays to do"), ui::ToastIcon::Check)
            }
            Ok(value) if value.get("checked") == Some(&Value::Bool(false)) => {
                Toast::new(format!("sent to {place}, but it changed meanwhile: stays to do"), ui::ToastIcon::Check)
            }
            Ok(_) => Toast::new(format!("sent to {place}"), ui::ToastIcon::Check),
            Err(reason) => Toast::new(reason.clone(), ui::ToastIcon::Bug),
        });
        true
    }

    fn removed_todos(&mut self, message: String, removed: Removed) {
        self.messages.toast =
            Some(Toast { undo: Some(Undo::Todos(removed)), ..Toast::new(message, ui::ToastIcon::Check) });
    }

    pub(super) fn click_todo(&mut self, id: u64, row: Rect, pos: Position, area: Rect) {
        let Some(item) = self.panels.todos.item(id) else { return };
        let mut editor = Editor::new(&item.text);
        let (line, col) = panel::spot(row, pos);
        editor.place(panel::text_width(self.todo_area(area)), line, col);
        self.panels.todo.field = Some(Field { item: Some(id), editor });
    }

    pub(super) fn drop_todo(&mut self, id: u64, pos: Position, area: Rect) {
        let panel_area = self.todo_area(area);
        if let Some(before) = panel::drop(panel_area, &self.todo_view(panel_area), id, pos) {
            self.panels.todos.move_before(id, before);
        }
    }

    pub(super) fn scroll_todo(&mut self, panel_area: Rect, delta: isize) {
        self.panels.todo.scroll = panel::rows(panel_area, &self.todo_view(panel_area)).scrolled(delta);
    }

    pub(super) fn auto_scroll_todo(&mut self, panel_area: Rect, pos: Position) -> bool {
        let rows = panel::rows(panel_area, &self.todo_view(panel_area));
        let Some(delta) = rows.edge(pos) else { return false };
        self.panels.todo.scroll = rows.scrolled(delta);
        true
    }
}
