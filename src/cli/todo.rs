use super::args::todo::TodoAction;
use super::{id, print_json, say};
use crate::client::{answer, ask};
use crate::control::{self, TodoList};
use crate::error::Result;

pub(super) fn run(action: TodoAction) -> Result<()> {
    match action {
        TodoAction::Add { text } => {
            let value = ask("todo", control::Command::Todo(control::Todo::Add(text.join(" "))))?;
            say(value, false, |done| id(done.todo))
        }
        TodoAction::List(print) => {
            let value = ask("todo", control::Command::Todo(control::Todo::List))?;
            if print.json {
                return print_json(&value);
            }
            let list: TodoList = answer(value)?;
            for item in list.todos {
                println!("{} [{}] {}", item.id, if item.done { 'x' } else { ' ' }, item.text);
            }
            Ok(())
        }
        TodoAction::Done { id } => ask("todo", control::Command::Todo(control::Todo::Done(id))).map(drop),
        TodoAction::Send { id, pane, tab, force, print } => {
            let send = control::TodoSend { id, pane, tab, force };
            say(ask("todo-send", control::Command::TodoSend(send))?, print.json, |done| {
                let pane = done.ids.pane.map_or_else(String::new, |pane| format!(" to pane {pane}"));
                if done.unconfirmed == Some(true) {
                    return vec![format!(
                        "sent todo {id}{pane}, but cornercase cannot tell whether the agent took it, so it stays to do"
                    )];
                }
                if done.checked == Some(false) {
                    return vec![format!("sent todo {id}{pane}, but it changed while it was sent, so it stays to do")];
                }
                vec![format!("sent todo {id}{pane}")]
            })
        }
        TodoAction::Rm { id } => ask("todo", control::Command::Todo(control::Todo::Rm(id))).map(drop),
    }
}
