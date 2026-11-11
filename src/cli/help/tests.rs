use clap::CommandFactory as _;
use rstest::rstest;

use super::{AFTER_HELP, CONTRACT_HELP, READ_HELP, SEND_HELP, STATUS_HELP, WAIT_HELP};
use crate::agents::Known;
use crate::cli::Cli;
use crate::control::PaneRow;

#[rstest]
#[case::send(SEND_HELP)]
#[case::read(READ_HELP)]
#[case::wait(WAIT_HELP)]
fn the_help_names_every_known_agent(#[case] text: &str) {
    for agent in Known::ALL {
        assert!(text.contains(agent.title()), "missing {}", agent.title());
    }
}

#[test]
fn the_status_help_lists_the_columns_of_panes_in_their_order() {
    let (_, list) = STATUS_HELP.split_once("The columns are").expect("the list of columns");
    let (list, _) = list.split_once(';').expect("the end of the list");
    let without_notes: String = list.split('(').filter_map(|part| part.rsplit(')').next()).collect();

    let named: Vec<&str> =
        without_notes.split([',', ' ', '\n']).filter(|word| !word.is_empty() && *word != "and").collect();

    assert_eq!(named, PaneRow::COLUMNS);
}

#[test]
fn the_view_help_names_the_commands_that_take_a_view() {
    for command in ["flow run", "open", "clone", "new-workspace", "account login"] {
        assert!(super::VIEW_HELP.contains(&format!("`{command}`")), "missing {command}");
    }
}

fn taking_a_boot(command: &clap::Command, prefix: &str, found: &mut Vec<String>) {
    for sub in command.get_subcommands() {
        let name = format!("{prefix}{}", sub.get_name());
        if sub.get_arguments().any(|arg| arg.get_id() == "boot") {
            found.push(name.clone());
        }
        taking_a_boot(sub, &format!("{name} "), found);
    }
}

fn named_between(text: &str, from: &str, to: &str) -> Vec<String> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let (_, rest) = text.split_once(from).expect("the start of the list");
    let (list, _) = rest.split_once(to).expect("the end of the list");
    let mut names: Vec<String> = list.split(", ").flat_map(|part| part.split(" and ")).map(str::to_string).collect();
    names.sort();
    names
}

#[test]
fn the_help_names_every_command_that_takes_a_boot() {
    let mut taking = Vec::new();
    taking_a_boot(&Cli::command(), "", &mut taking);
    taking.sort();

    assert_eq!(named_between(AFTER_HELP, "A command given --boot (", ")"), taking);
    assert_eq!(named_between(CONTRACT_HELP, "since ids last only one run; ", " take it as"), taking);
}
