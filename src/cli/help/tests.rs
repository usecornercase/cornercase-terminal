use rstest::rstest;

use super::{READ_HELP, SEND_HELP, STATUS_HELP, WAIT_HELP};
use crate::agents::Known;
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
    for command in ["flow run", "open", "clone", "new-workspace"] {
        assert!(super::VIEW_HELP.contains(&format!("`{command}`")), "missing {command}");
    }
}
