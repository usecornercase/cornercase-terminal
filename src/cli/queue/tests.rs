use clap::Parser;
use serde_json::json;

use super::*;
use crate::cli::{Cli, Command, Control};

fn action(args: &[&str]) -> QueueAction {
    let cli = Cli::try_parse_from(["cornercase", "queue"].into_iter().chain(args.iter().copied())).expect("parse");
    let Some(Command::Control(Control::Queue(queue))) = cli.command else { panic!("not queue") };
    queue.action
}

#[test]
fn adding_without_a_time_or_a_prompt_is_wrong_usage_before_asking_the_server() {
    for args in [&["add", "--prompt", "fix it"][..], &["add", "--at", "in 2h"]] {
        assert!(matches!(run(action(args)), Err(Error::WrongUsage(_))), "{args:?}");
    }
}

#[test]
fn lists_entries_from_another_release_without_failing() {
    let value = json!({"entries": [
        {"id": 3, "at": 0, "label": "fix", "agent": "claude", "state": "started", "pane": 7, "server": "1-2"},
        {"id": 4, "at": 0, "label": "docs", "agent": "codex", "state": "failed", "reason": "gone"},
        {"id": 5, "at": 0, "label": "new", "agent": "grok", "state": "paused", "colour": "red"},
        {"id": 6, "label": "odd", "task": {"at": "soon"}},
    ]});

    assert_eq!(
        lines(&value, "UTC"),
        "3 1970-01-01 00:00 started · pane 7 · server 1-2 fix · claude\n\
         4 1970-01-01 00:00 failed: gone docs · codex\n\
         5 1970-01-01 00:00 paused new · grok\n\
         6 1970-01-01 00:00  odd · \n"
    );
}
