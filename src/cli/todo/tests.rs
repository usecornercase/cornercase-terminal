use clap::Parser;

use super::*;
use crate::cli::Cli;
use crate::cli::args::{Command, Control};

fn parse(args: &[&str]) -> std::result::Result<Cli, clap::Error> {
    Cli::try_parse_from(std::iter::once("cornercase").chain(args.iter().copied()))
}

fn action(args: &[&str]) -> TodoAction {
    let Some(Command::Control(Control::Todo(todo))) = parse(args).expect("parse").command else { panic!("not todo") };
    todo.action
}

#[test]
fn move_needs_a_place() {
    assert!(parse(&["todo", "move", "3"]).is_err());
}

#[test]
fn move_takes_one_place() {
    assert!(parse(&["todo", "move", "3", "--before", "5", "--last"]).is_err());
}

#[test]
fn move_before_or_last_parses() {
    assert!(matches!(action(&["todo", "move", "3", "--before", "5"]), TodoAction::Move { id: 3, before: Some(5), .. }));
    assert!(matches!(action(&["todo", "move", "3", "--last"]), TodoAction::Move { id: 3, before: None, last: true }));
}

#[test]
fn edit_joins_the_words() {
    let TodoAction::Edit { id, text } = action(&["todo", "edit", "3", "two", "words"]) else { panic!("not edit") };
    assert_eq!((id, text.join(" ")), (3, "two words".to_string()));
}
