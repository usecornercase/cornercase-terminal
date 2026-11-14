use clap::CommandFactory;

use super::*;
use crate::cli::args::Cli;
use crate::cli::help::{BY_BRANCH, HERE_PANE, HERE_WORKSPACE, WAIT_PANE};

fn refused(args: &[&str]) -> Option<String> {
    let command = Cli::command();
    let matches = command
        .clone()
        .try_get_matches_from(std::iter::once("cornercase").chain(args.iter().copied()))
        .unwrap_or_else(|error| panic!("{args:?}: {error}"));
    refusal(&command, &matches)
}

#[rstest::rstest]
#[case::start(&["start", "--prompt", "x"], "a bot runs in no pane")]
#[case::send(&["send", "hi"], "a bot runs in no pane")]
#[case::read(&["read"], "a bot runs in no pane")]
#[case::close_by_branch(&["close", "--worktree", "fix"], "a bot runs in no pane")]
#[case::new_tab(&["new-tab"], "a bot runs in no pane")]
#[case::changes(&["changes", "list"], "a bot runs in no pane")]
#[case::kill(&["kill-server"], "a bot cannot stop its own server")]
#[case::restart(&["restart"], "a bot cannot stop its own server")]
#[case::update(&["update"], "a bot cannot stop its own server")]
fn a_bot_is_refused(#[case] args: &[&str], #[case] why: &str) {
    let found = refused(args).unwrap_or_else(|| panic!("{args:?} was allowed"));
    assert!(found.starts_with(why), "{found}");
}

#[test]
fn the_refusal_names_the_flags_that_take_a_place() {
    let found = refused(&["start", "--prompt", "x"]).expect("refused");
    assert!(found.contains("--project ID"), "{found}");
    assert!(found.contains("cornercase status --json"), "{found}");
}

#[rstest::rstest]
#[case::start(&["start", "--project", "3", "--prompt", "x"])]
#[case::send(&["send", "--pane", "4", "hi"])]
#[case::read(&["read", "--pane", "4"])]
#[case::close(&["close", "--workspace", "2"])]
#[case::status(&["status"])]
#[case::status_json(&["status", "--json"])]
#[case::search(&["search", "foo"])]
#[case::recap(&["recap"])]
#[case::worktrees(&["worktrees", "list"])]
#[case::notify(&["notify", "hi"])]
#[case::ports_all(&["ports", "--all"])]
#[case::size_of_a_path(&["worktrees", "size", "--path", "/x"])]
fn a_bot_may_name_its_place_or_use_what_needs_none(#[case] args: &[&str]) {
    assert_eq!(refused(args), None, "{args:?}");
}

#[test]
fn the_defaults_that_say_where_a_command_runs_carry_the_words_the_guard_looks_for() {
    for text in [HERE_PANE, HERE_WORKSPACE, BY_BRANCH, WAIT_PANE] {
        assert!(text.contains(HERE), "{text}");
    }
}
