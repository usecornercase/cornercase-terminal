use super::{Handled, json, name_of, none};
use crate::app::menus::Target;
use crate::app::{App, Toast};
use crate::control::{self, Done, Item, TodoItem, TodoList};
use crate::notify::{self, Notification};
use crate::project::Hierarchy;
use crate::ui;

impl App {
    pub(super) fn rename_request(&mut self, caller: Option<u64>, rename: control::Rename) -> Handled {
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
                let here = self.here(caller);
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
}
