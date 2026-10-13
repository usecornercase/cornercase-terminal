use rstest::rstest;

use super::{READ_HELP, SEND_HELP, WAIT_HELP};
use crate::agents::Known;

#[rstest]
#[case::send(SEND_HELP)]
#[case::read(READ_HELP)]
#[case::wait(WAIT_HELP)]
fn the_help_names_every_known_agent(#[case] text: &str) {
    for agent in Known::ALL {
        assert!(text.contains(agent.title()), "missing {}", agent.title());
    }
}
