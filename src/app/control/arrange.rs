use std::time::Instant;

use ratatui::layout::Rect;

use super::{Asker, Handled, conflict, json, malformed, name_of, none, pane_ids, sized};
use crate::app::menus::{Marked, Target};
use crate::app::{App, Toast};
use crate::control::arrange::{GroupDelete, GroupNew, GroupStyle, Move, MovePane, Styled};
use crate::control::contract::{Code, Refusal};
use crate::control::terminal::Moved;
use crate::control::{self, Command, Done, Item, Layout, Request, TodoItem, TodoList};
use crate::host::notify::{self, Notification};
use crate::log;
use crate::project::Hierarchy;
use crate::project::group::{GROUP_COLOURS, GROUP_ICONS};
use crate::ui;

impl App {
    pub(super) fn mark_request(&mut self, asker: Asker, mark: &control::Mark) -> Handled {
        let pane = self.target(asker, mark.pane, mark.tab)?;
        if self.projects.pane_by(pane).is_none() {
            return Err(none("pane", pane));
        }
        if self.mark(Marked::Pane(pane), !mark.reviewed) == 0 {
            return Err(format!("pane {pane} runs no agent cornercase reads, so there is nothing to mark").into());
        }
        Ok(Some(json(&pane_ids(pane))))
    }

    pub(in crate::app) fn go_request(&mut self, asker: Asker, go: &control::Go) -> Handled {
        let item = match self.act_on_notice(&go.token) {
            None => {
                let message = format!(
                    "there is no notice {}: it is from another start of the server, or older than the last {} notices",
                    go.token,
                    notify::KEPT
                );
                return Err(Refusal::new(Code::NotFound, message));
            }
            Some(Err(gone)) => return Err(Refusal::new(Code::NotFound, gone)),
            Some(Ok((_, item))) => item,
        };
        let projection = self.show_to(asker.view, item)?;
        let ids = match asker.view {
            Some(view) => self.view_ids(view),
            None => self.project().map(|_| self.shown_ids(self.active)).unwrap_or_default(),
        };
        log::info!("activity", "notice gone to", view = asker.view.unwrap_or_default());
        Ok(Some(json(&Done { ids, projection, ..Done::default() })))
    }

    pub(super) fn rename_request(&mut self, asker: Asker, rename: control::Rename) -> Handled {
        let target = match rename.item {
            Some(Item::Group(id)) => {
                self.group_index(id).ok_or_else(|| none("group", id))?;
                Target::Group(id)
            }
            Some(Item::Project(id)) => {
                self.projects.project_index(id).ok_or_else(|| none("project", id))?;
                Target::Project(id)
            }
            Some(Item::Workspace(id)) => {
                let (p, _) = self.usable_workspace(id)?;
                Target::Workspace(self.projects[p].id, id)
            }
            Some(Item::Tab(id)) => self.tab_target(self.projects.tab_index(id)).ok_or_else(|| none("tab", id))?,
            Some(Item::Pane(id)) => self.tab_target(self.projects.tab_of_pane(id)).ok_or_else(|| none("pane", id))?,
            None => {
                let here = self.here(asker);
                let found = here.project.zip(here.workspace).zip(here.tab).map(|((p, w), t)| (p, w, t));
                self.tab_target(found).ok_or("no tab is open here; pass --tab")?
            }
        };
        let name = name_of(Some(rename.name));
        if matches!(target, Target::Group(_)) && name.is_none() {
            return Err("a group needs a name".into());
        }
        self.rename(target, name);
        Ok(Some(json(&Done::default())))
    }

    pub(in crate::app) fn tab_target(&self, found: Option<(usize, usize, usize)>) -> Option<Target> {
        let (p, w, t) = found?;
        let project = &self.projects[p];
        let workspace = &project.workspaces[w];
        Some(Target::Tab(project.id, workspace.id, workspace.tabs[t].id))
    }

    pub(super) fn notify_request(&mut self, text: &str) -> Handled {
        let message = notify::clean(text).trim().to_string();
        if message.is_empty() {
            return Err("the notification needs a text".into());
        }
        self.outbox.notifications.extend(Notification::new(&message, &self.config.desktop_notifications));
        self.messages.toast = Some(Toast::new(message, ui::ToastIcon::Check));
        Ok(Some(json(&Done::default())))
    }

    pub(super) fn todo_request(&mut self, todo: control::Todo) -> Handled {
        match todo {
            control::Todo::Add(text) => self.todo_added(&text),
            control::Todo::List => Ok(Some(json(&TodoList { todos: self.todo_items(), undo: None }))),
            control::Todo::Done(id) => self.todo_toggled(id, false),
            control::Todo::Undone(id) => self.todo_toggled(id, true),
            control::Todo::Edit { id, text } => self.todo_edited(id, &text),
            control::Todo::Move { id, before } => self.todo_moved(id, before),
            control::Todo::Rm(id) => {
                let removed = self.panels.todos.remove(id).ok_or_else(|| none("todo", id))?;
                Ok(Some(json(&self.removed_by_request(removed))))
            }
            control::Todo::ClearDone => {
                let list = self
                    .panels
                    .todos
                    .clear_done()
                    .map_or_else(TodoList::default, |removed| self.removed_by_request(removed));
                Ok(Some(json(&list)))
            }
            control::Todo::Restore(undo) => self.todo_restored(undo),
        }
    }

    fn todo_added(&mut self, text: &str) -> Handled {
        let id = self.panels.todos.add(text).ok_or("the todo needs a text")?;
        Ok(Some(json(&Done { todo: Some(id), ..Done::default() })))
    }

    fn todo_toggled(&mut self, id: u64, only_if_done: bool) -> Handled {
        let done = self.panels.todos.item(id).ok_or_else(|| none("todo", id))?.done;
        if !only_if_done || done {
            self.panels.todos.toggle(id);
        }
        Ok(Some(json(&Done::default())))
    }

    fn todo_edited(&mut self, id: u64, text: &str) -> Handled {
        self.panels.todos.item(id).ok_or_else(|| none("todo", id))?;
        if !self.panels.todos.edit(id, text) {
            return Err("the todo needs a text".into());
        }
        if self.panels.todo.field.as_ref().is_some_and(|field| field.item == Some(id)) {
            self.panels.todo.field = None;
        }
        Ok(Some(json(&Done::default())))
    }

    fn todo_moved(&mut self, id: u64, before: Option<u64>) -> Handled {
        self.panels.todos.item(id).ok_or_else(|| none("todo", id))?;
        if let Some(before) = before {
            self.panels.todos.item(before).ok_or_else(|| none("todo", before))?;
            if before == id {
                return Err(malformed(format!("todo {id} cannot move before itself")));
            }
        }
        self.panels.todos.move_to(id, before);
        Ok(Some(json(&Done::default())))
    }

    fn todo_restored(&mut self, undo: u64) -> Handled {
        let removed = self.panels.todo.undos.take(undo).ok_or_else(|| {
            Refusal::new(
                Code::NotFound,
                format!(
                    "there is no removal {undo} to undo: it was undone already, or is older than the last {}",
                    crate::todo::UNDOS
                ),
            )
        })?;
        let todos = removed.items().map(todo_item).collect();
        self.panels.todos.restore(removed);
        Ok(Some(json(&TodoList { todos, undo: None })))
    }

    pub(in crate::app) fn todo_items(&self) -> Vec<TodoItem> {
        self.panels.todos.items().iter().map(todo_item).collect()
    }

    fn removed_by_request(&mut self, removed: crate::todo::Removed) -> TodoList {
        let todos = removed.items().map(todo_item).collect();
        if let Some(field) = &self.panels.todo.field
            && field.item.is_some_and(|open| removed.items().any(|item| item.id == open))
        {
            self.panels.todo.field = None;
        }
        let undo = self.panels.todo.undos.keep(removed);
        TodoList { todos, undo: Some(undo) }
    }

    pub(super) fn todo_send_request(
        &mut self,
        client: u64,
        asker: Asker,
        todo: &control::TodoSend,
        now: Instant,
    ) -> Handled {
        let item = self.panels.todos.item(todo.id).ok_or_else(|| none("todo", todo.id))?;
        if item.done {
            return Err(format!("todo {} is already done, so it is not sent; uncheck it first", todo.id).into());
        }
        if self.wire.requests.todo_sends.values().any(|sending| sending.todo == todo.id) {
            return Err(format!("todo {} is already being sent; wait for its answer", todo.id).into());
        }
        let pane = self.target(asker, todo.pane, todo.tab)?;
        let term = self.projects.pane_by(pane).ok_or_else(|| none("pane", pane))?;
        if !self.runs_agent(term) {
            return Err(format!("pane {pane} runs no agent; `cornercase status --panes` lists the agents").into());
        }
        let text = item.text.clone();
        let confirmed = self.confirms(term, Some(&text));
        let send = control::SendText {
            text: Some(text.clone()),
            enter: true,
            pane: todo.pane,
            tab: todo.tab,
            force: todo.force,
            ..control::SendText::default()
        };
        self.wire.requests.todo_sends.insert(client, super::Sending { todo: todo.id, text, confirmed });
        self.send_request(client, asker, send, now)
    }

    pub(in crate::app) fn send_todo(&mut self, client: u64, todo: u64, pane: u64, now: Instant) {
        let command = Command::TodoSend(control::TodoSend { id: todo, pane: Some(pane), ..Default::default() });
        let request = Request { caller: None, server: None, contract: None, boot: None, view: None, command };
        self.run_request(client, Ok(request), None, now);
    }
}

impl App {
    pub(super) fn move_request(&mut self, moving: &Move) -> Handled {
        if matches!(moving.item, Item::Pane(_)) {
            return Err(malformed("`move` takes a group, project, workspace or tab; move a pane with `move-pane`"));
        }
        if (moving.into.is_some() || moving.ungroup) && !matches!(moving.item, Item::Project(_)) {
            return Err(malformed("only a project moves into or out of a group"));
        }
        let places =
            usize::from(moving.before.is_some()) + usize::from(moving.into.is_some()) + usize::from(moving.ungroup);
        if places > 1 {
            return Err(malformed("say one place: `before`, `into` or `ungroup`"));
        }
        if let (Some(before), Item::Group(id) | Item::Project(id) | Item::Workspace(id) | Item::Tab(id)) =
            (moving.before, moving.item)
            && before == id
        {
            return Err(malformed(format!("{} cannot move before itself", moving.item)));
        }
        match moving.item {
            Item::Group(id) => self.move_group_request(id, moving.before)?,
            Item::Project(id) => self.move_project_request(id, moving)?,
            Item::Workspace(id) => self.move_workspace_request(id, moving.before)?,
            Item::Tab(id) => self.move_tab_request(id, moving.before)?,
            Item::Pane(_) => {}
        }
        Ok(Some(json(&Done::default())))
    }

    fn move_group_request(&mut self, id: u64, before: Option<u64>) -> Result<(), Refusal> {
        let from = self.group_index(id).ok_or_else(|| none("group", id))?;
        let to = match before {
            Some(before) => self.group_index(before).ok_or_else(|| none("group", before))?,
            None => self.groups.len(),
        };
        self.move_group(from, to);
        Ok(())
    }

    fn move_project_request(&mut self, id: u64, moving: &Move) -> Result<(), Refusal> {
        let p = self.projects.project_index(id).ok_or_else(|| none("project", id))?;
        let own = self.projects[p].group.filter(|g| self.group_index(*g).is_some());
        let (group, before) = if let Some(before) = moving.before {
            let q = self.projects.project_index(before).ok_or_else(|| none("project", before))?;
            (self.projects[q].group.filter(|g| self.group_index(*g).is_some()), Some(before))
        } else if let Some(into) = moving.into {
            self.group_index(into).ok_or_else(|| none("group", into))?;
            (Some(into), None)
        } else if moving.ungroup {
            (None, None)
        } else {
            (own, None)
        };
        log::info!("app", "project moved", project = id, group = group.unwrap_or_default());
        self.move_project(id, group, before);
        Ok(())
    }

    fn move_workspace_request(&mut self, id: u64, before: Option<u64>) -> Result<(), Refusal> {
        let (p, w) = self.usable_workspace(id)?;
        let to = match before {
            Some(before) => {
                let (other, v) = self.projects.workspace_index(before).ok_or_else(|| none("workspace", before))?;
                if other != p {
                    return Err(conflict(format!(
                        "workspace {before} is in another project: a workspace moves only among its project's"
                    )));
                }
                v
            }
            None => self.projects[p].workspaces.len(),
        };
        self.move_workspace(p, w, to);
        Ok(())
    }

    fn move_tab_request(&mut self, id: u64, before: Option<u64>) -> Result<(), Refusal> {
        let (p, w, t) = self.projects.tab_index(id).ok_or_else(|| none("tab", id))?;
        let to = match before {
            Some(before) => {
                let (op, ow, u) = self.projects.tab_index(before).ok_or_else(|| none("tab", before))?;
                if (op, ow) != (p, w) {
                    return Err(conflict(format!(
                        "tab {before} is in another workspace: a tab moves only among its workspace's"
                    )));
                }
                u
            }
            None => self.projects[p].workspaces[w].tabs.len(),
        };
        self.move_tab(p, w, t, to);
        Ok(())
    }

    pub(super) fn fold_request(&mut self, command: &str, item: Item, collapsed: bool) -> Handled {
        let target = match item {
            Item::Group(id) => {
                self.group_index(id).ok_or_else(|| none("group", id))?;
                Target::Group(id)
            }
            Item::Project(id) => {
                self.projects.project_index(id).ok_or_else(|| none("project", id))?;
                Target::Project(id)
            }
            Item::Workspace(id) => {
                let (p, _) = self.usable_workspace(id)?;
                Target::Workspace(self.projects[p].id, id)
            }
            Item::Pane(id) => {
                let term = self.projects.pane_by(id).ok_or_else(|| none("pane", id))?;
                if term.agent.status().is_none() {
                    return Err(conflict(format!(
                        "pane {id} runs no agent cornercase lists, so it has no agents to fold"
                    )));
                }
                self.set_agent_fold(id, collapsed);
                return Ok(Some(json(&Done::default())));
            }
            Item::Tab(_) => {
                return Err(malformed(format!("`{command}` takes a group, project, workspace or pane")));
            }
        };
        self.set_collapsed(target, collapsed);
        Ok(Some(json(&Done::default())))
    }

    pub(super) fn group_new_request(&mut self, new: &GroupNew) -> Handled {
        let name = name_of(Some(new.name.clone())).ok_or("a group needs a name")?;
        let id = self.add_group(name);
        Ok(Some(json(&Done { group: Some(id), ..Done::default() })))
    }

    pub(super) fn group_style_request(&mut self, style: &GroupStyle) -> Handled {
        let id = style.group.ok_or_else(|| malformed("`group-style` needs the group: pass its id"))?;
        self.group_index(id).ok_or_else(|| none("group", id))?;
        let icon = match &style.icon {
            Some(text) => {
                let mut chars = text.chars();
                let icon = chars.next().filter(|icon| chars.next().is_none() && GROUP_ICONS.contains(icon));
                Some(icon.ok_or_else(|| {
                    let icons: Vec<String> = GROUP_ICONS.iter().map(char::to_string).collect();
                    malformed(format!("the icon is one of {}", icons.join(" ")))
                })?)
            }
            None => None,
        };
        if let Some(colour) = style.colour.filter(|colour| !GROUP_COLOURS.contains(colour)) {
            let colours: Vec<String> = GROUP_COLOURS.iter().map(u8::to_string).collect();
            return Err(malformed(format!("the colour {colour} is not offered; it is one of {}", colours.join(" "))));
        }
        if icon.is_some() || style.colour.is_some() {
            self.style_group(id, icon, style.colour);
        }
        let entry = self.group(id).ok_or_else(|| none("group", id))?;
        let styled = Styled {
            group: id,
            icon: entry.icon.to_string(),
            colour: entry.colour,
            icons: GROUP_ICONS.iter().map(char::to_string).collect(),
            colours: GROUP_COLOURS.to_vec(),
        };
        Ok(Some(json(&styled)))
    }

    pub(super) fn group_delete_request(&mut self, delete: &GroupDelete) -> Handled {
        let id = delete.group.ok_or_else(|| malformed("`group-delete` needs the group: pass its id"))?;
        self.group_index(id).ok_or_else(|| none("group", id))?;
        if delete.close_projects {
            self.delete_group_and_projects(id);
        } else {
            self.delete_group(id);
        }
        Ok(Some(json(&Done::default())))
    }

    pub(super) fn move_pane_request(&mut self, moving: &MovePane, area: Option<Rect>) -> Handled {
        let (Some(pane), Some(target), Some(place)) = (moving.pane, moving.target, moving.place) else {
            return Err(malformed("`move-pane` needs --pane, --target and --place"));
        };
        if pane == target {
            return Err(malformed("a pane cannot move onto itself"));
        }
        let (p, w, t) = self.projects.tab_of_pane(pane).ok_or_else(|| none("pane", pane))?;
        let (op, ow, ot) = self.projects.tab_of_pane(target).ok_or_else(|| none("pane", target))?;
        if (p, w, t) != (op, ow, ot) {
            return Err(conflict(format!("pane {target} is in another tab: a pane moves within its tab")));
        }
        let window = sized(area)?;
        let tab_id = self.projects[p].workspaces[w].tabs[t].id;
        let room = self.tab_area(tab_id, window);
        let tab = &mut self.projects[p].workspaces[w].tabs[t];
        let layout = tab.rearranged(room, pane, target, place.into()).ok_or_else(|| {
            conflict(format!(
                "tab {tab_id} has no room for every pane after that move ({}×{})",
                room.width, room.height
            ))
        })?;
        log::info!("app", "pane moved", pane = pane, tab = tab_id, place = format!("{place:?}"));
        tab.land(pane, layout);
        let answer = Moved { tab: tab_id, layout: Layout::from(&tab.layout) };
        self.resize(window);
        Ok(Some(json(&answer)))
    }
}

fn todo_item(item: &crate::todo::Item) -> TodoItem {
    TodoItem { id: item.id, text: item.text.clone(), done: item.done }
}
