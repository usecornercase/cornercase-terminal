use std::time::Instant;

use super::{Asker, Handled, json, name_of, none, pane_ids};
use crate::app::menus::{Marked, Target};
use crate::app::{App, Toast};
use crate::control::contract::{Code, Refusal};
use crate::control::{self, Command, Done, Item, Request, TodoItem, TodoList};
use crate::host::notify::{self, Notification};
use crate::log;
use crate::project::Hierarchy;
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
        self.toast = Some(Toast::new(message, ui::ToastIcon::Check));
        Ok(Some(json(&Done::default())))
    }

    pub(super) fn todo_request(&mut self, todo: control::Todo) -> Handled {
        let done = match todo {
            control::Todo::Add(text) => {
                Done { todo: Some(self.todos.add(&text).ok_or("the todo needs a text")?), ..Done::default() }
            }
            control::Todo::List => {
                let todos =
                    self.todos.items().iter().map(|i| TodoItem { id: i.id, text: i.text.clone(), done: i.done });
                return Ok(Some(json(&TodoList { todos: todos.collect() })));
            }
            control::Todo::Done(id) => {
                self.todos.item(id).ok_or_else(|| none("todo", id))?;
                self.todos.toggle(id);
                Done::default()
            }
            control::Todo::Rm(id) => {
                self.todos.remove(id).ok_or_else(|| none("todo", id))?;
                Done::default()
            }
        };
        Ok(Some(json(&done)))
    }

    pub(super) fn todo_send_request(
        &mut self,
        client: u64,
        asker: Asker,
        todo: &control::TodoSend,
        now: Instant,
    ) -> Handled {
        let item = self.todos.item(todo.id).ok_or_else(|| none("todo", todo.id))?;
        if item.done {
            return Err(format!("todo {} is already done, so it is not sent; uncheck it first", todo.id).into());
        }
        if self.requests.todo_sends.values().any(|sending| sending.todo == todo.id) {
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
        self.requests.todo_sends.insert(client, super::Sending { todo: todo.id, text, confirmed });
        self.send_request(client, asker, send, now)
    }

    pub(in crate::app) fn send_todo(&mut self, client: u64, todo: u64, pane: u64, now: Instant) {
        let command = Command::TodoSend(control::TodoSend { id: todo, pane: Some(pane), ..Default::default() });
        let request = Request { caller: None, server: None, contract: None, boot: None, view: None, command };
        self.run_request(client, Ok(request), 0, None, now);
    }
}
