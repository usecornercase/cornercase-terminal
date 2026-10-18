use super::args::todo::TodoAction;
use super::{id, print_json, say};
use crate::client::{answer, ask, use_todo_panel};
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
        TodoAction::Rm { id, print } => removal(control::Todo::Rm(id), print.json),
        TodoAction::Edit { id, text } => panel(control::Todo::Edit { id, text: text.join(" ") }),
        TodoAction::Undone { id } => panel(control::Todo::Undone(id)),
        TodoAction::Move { id, before, last: _ } => panel(control::Todo::Move { id, before }),
        TodoAction::ClearDone(print) => {
            use_todo_panel()?;
            removal(control::Todo::ClearDone, print.json)
        }
        TodoAction::Restore { undo } => panel(control::Todo::Restore(undo)),
    }
}

fn panel(todo: control::Todo) -> Result<()> {
    use_todo_panel()?;
    ask("todo", control::Command::Todo(todo)).map(drop)
}

fn removal(todo: control::Todo, json: bool) -> Result<()> {
    let value = ask("todo", control::Command::Todo(todo))?;
    if json {
        return print_json(&value);
    }
    let list: TodoList = answer(value)?;
    if let Some(undo) = list.undo {
        println!("{undo}");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
