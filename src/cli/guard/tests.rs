use clap::{ArgMatches, Command, CommandFactory};

use super::*;
use crate::cli::args::Cli;
use crate::cli::help::{BY_BRANCH, EXPORT_TO, HERE, HERE_PANE, HERE_WORKSPACE, TO_FILE, WAIT_PANE};

fn checked_by(guard: fn(&Command, &ArgMatches, &str) -> Option<String>, args: &[&str]) -> Option<String> {
    let command = Cli::command();
    let matches = command
        .clone()
        .try_get_matches_from(std::iter::once("cornercase").chain(args.iter().copied()))
        .unwrap_or_else(|error| panic!("{args:?}: {error}"));
    guard(&command, &matches, "prs")
}

fn refused(args: &[&str]) -> Option<String> {
    checked_by(refusal, args)
}

#[rstest::rstest]
#[case::start(&["start", "--prompt", "x"], "a bot runs in no pane")]
#[case::send(&["send", "hi"], "a bot runs in no pane")]
#[case::read(&["read"], "a bot runs in no pane")]
#[case::close_by_branch(&["close", "--worktree", "fix"], "a bot runs in no pane")]
#[case::new_tab(&["new-tab"], "a bot runs in no pane")]
#[case::changes(&["changes", "list"], "a bot runs in no pane")]
#[case::edit_access(&["bot", "edit", "prs", "--access", "cornercase"], "a bot cannot change a bot's access")]
#[case::new_full(&["bot", "new", "x", "--access", "full"], "a bot cannot change a bot's access")]
#[case::itself(&["bot", "say", "PRs", "hi"], "a bot cannot write to itself")]
#[case::kill(&["kill-server"], "a bot cannot stop its own server")]
#[case::restart(&["restart"], "a bot cannot stop its own server")]
#[case::update(&["update"], "a bot cannot stop its own server")]
#[case::export_to(&["config", "export", "--to", "/x"], "a bot cannot have cornercase write to a file: leave out --to")]
#[case::proxy(&["proxy"], "a bot cannot attach as a window")]
#[case::remote_window(&["remote", "vps"], "a bot cannot attach as a window")]
#[case::server(&["server"], "a bot cannot run cornercase's server")]
#[case::give(&["bot", "secret", "give", "prs", "3"], "a bot cannot give or decline what a bot asks for")]
#[case::decline(&["bot", "secret", "decline", "prs", "3"], "a bot cannot give or decline what a bot asks for")]
#[case::plugin_enable(&["plugin", "enable", "x"], crate::bots::NO_PLUGINS)]
#[case::plugin_install(&["plugin", "install", "x"], crate::bots::NO_PLUGINS)]
#[case::plugin_run(&["plugin", "run", "x", "y", "--project", "1"], crate::bots::NO_PLUGINS)]
#[case::plugin_windows(&["plugin", "windows"], crate::bots::NO_PLUGINS)]
#[case::machines_add(&["machines", "add", "vps", "me@vps"], crate::bots::NO_MACHINES)]
#[case::machines_update(&["machines", "update", "vps"], crate::bots::NO_MACHINES)]
#[case::machines_remove(&["machines", "remove", "vps"], crate::bots::NO_MACHINES)]
#[case::tunnel_open(&["tunnel", "open", "5173"], crate::bots::NO_TUNNELS)]
#[case::tunnel_hold(&["tunnel", "hold", "5173"], crate::bots::NO_TUNNELS)]
fn a_bot_is_refused(#[case] args: &[&str], #[case] why: &str) {
    let found = refused(args).unwrap_or_else(|| panic!("{args:?} was allowed"));
    assert!(found.starts_with(why), "{found}");
}

#[test]
fn a_bot_opens_no_window_but_a_request_line_without_a_command_is_left_to_its_own_answer() {
    assert_eq!(checked_by(run_refusal, &[]).as_deref(), Some("a bot cannot attach as a window"));
    assert_eq!(checked_by(run_refusal, &["--remote", "vps"]).as_deref(), Some("a bot cannot attach as a window"));
    assert_eq!(refused(&[]), None);
    assert_eq!(checked_by(run_refusal, &["status"]), None);
}

#[test]
fn the_refusal_names_the_flags_that_take_a_place() {
    let found = refused(&["start", "--prompt", "x"]).expect("refused");
    assert!(found.contains("--project ID"), "{found}");
    assert!(found.contains("cornercase status --json"), "{found}");
}

#[rstest::rstest]
#[case::setup(&["bot", "setup", "other"])]
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
#[case::say_to_another(&["bot", "say", "other", "hi"])]
#[case::new(&["bot", "new", "x"])]
#[case::edit(&["bot", "edit", "prs", "--name", "y"])]
#[case::about(&["bot", "about", "other", "--json"])]
#[case::ports_all(&["ports", "--all"])]
#[case::size_of_a_path(&["worktrees", "size", "--path", "/x"])]
#[case::export(&["config", "export"])]
#[case::import_preview(&["config", "import", "/x", "--preview"])]
#[case::known_machines(&["remote", "--known"])]
#[case::ask(&["bot", "secret", "ask", "pw", "--about", "to log in"])]
#[case::plugin_list(&["plugin", "list"])]
#[case::plugin_status(&["plugin", "status"])]
#[case::machines_list(&["machines"])]
#[case::machines_retry(&["machines", "retry"])]
#[case::tunnels(&["tunnels"])]
fn a_bot_may_name_its_place_or_use_what_needs_none(#[case] args: &[&str]) {
    assert_eq!(refused(args), None, "{args:?}");
}

#[test]
fn the_defaults_that_say_where_a_command_runs_carry_the_words_the_guard_looks_for() {
    for text in [HERE_PANE, HERE_WORKSPACE, BY_BRANCH, WAIT_PANE] {
        assert!(text.contains(HERE), "{text}");
    }
}

#[test]
fn the_flags_that_write_a_file_carry_the_words_the_guard_looks_for() {
    assert!(EXPORT_TO.contains(TO_FILE), "{EXPORT_TO}");
}
