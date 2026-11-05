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

#[test]
fn an_agent_named_twice_is_wrong_usage_before_asking_the_server() {
    let args = ["add", "--at", "in 2h", "--prompt", "fix", "--account", "claude=home", "--account", "claude=work"];
    assert!(matches!(run(action(&args)), Err(Error::WrongUsage(_))));
}

#[test]
fn lists_the_accounts_an_entry_starts_with() {
    let value = json!({"entries": [
        {"id": 1, "at": 0, "label": "fix", "agent": "claude", "state": "pending", "accounts": {"claude": "home"}},
        {"id": 2, "at": 0, "label": "docs", "agent": "codex", "state": "pending", "accounts": {"codex": ""}},
    ]});

    assert_eq!(
        lines(&value, "UTC"),
        "1 1970-01-01 00:00 pending fix · claude · claude: home\n\
         2 1970-01-01 00:00 pending docs · codex · codex: System\n"
    );
}

#[test]
fn takes_accounts_for_add_and_edit() {
    for args in [
        &["add", "--at", "in 2h", "--prompt", "fix", "--account", "claude=home", "--account", "codex="][..],
        &["edit", "3", "--account", "claude="],
    ] {
        let (QueueAction::Add(add) | QueueAction::Edit { add, .. }) = action(args) else { panic!("{args:?}") };
        assert!(!add.accounts.is_empty(), "{args:?}");
    }
    assert!(Cli::try_parse_from(["cornercase", "queue", "edit", "3", "--account", "claude"]).is_err());
}

#[test]
fn removals_take_json_and_restore_takes_the_undo_number() {
    for (args, command, json) in [
        (&["remove", "5"][..], control::Queue::Remove(5), false),
        (&["remove", "5", "--json"][..], control::Queue::Remove(5), true),
        (&["clear-started"][..], control::Queue::ClearStarted, false),
        (&["clear-started", "--json"][..], control::Queue::ClearStarted, true),
        (&["restore", "8"][..], control::Queue::Restore(8), false),
    ] {
        assert_eq!(request_of(action(args)).expect("queue request"), (command, json), "{args:?}");
    }
}

mod routines {
    use super::*;

    #[test]
    fn takes_every_flow_and_cleanup_and_the_pause_and_resume_actions() {
        let QueueAction::Add(add) =
            action(&["add", "--every", "day 09:00", "--flow", "Quick", "--cleanup", "--prompt", "go"])
        else {
            panic!("not add")
        };
        let request = queue_add(add, true).expect("request");
        assert_eq!(request.repeat.as_deref(), Some("day 09:00"));
        assert_eq!((request.flow.as_deref(), request.cleanup), (Some("Quick"), Some(true)));
        assert_eq!(request.at, "");
        let (pause, _) = request_of(action(&["pause", "4"])).expect("pause");
        let (resume, _) = request_of(action(&["resume", "4"])).expect("resume");
        assert_eq!((pause, resume), (control::Queue::Pause(4), control::Queue::Resume(4)));
    }

    #[test]
    fn a_routine_without_a_prompt_or_a_flow_without_every_is_wrong_usage() {
        for args in [&["add", "--every", "2h"][..], &["add", "--flow", "Quick", "--prompt", "go", "--at", "in 2h"]] {
            assert!(matches!(run(action(args)), Err(Error::WrongUsage(_))), "{args:?}");
        }
    }

    #[test]
    fn every_does_not_mix_with_at_an_issue_a_workspace_or_a_worktree() {
        let cases = [
            ["--prompt", "go", "--at", "in 2h"],
            ["--prompt", "go", "--workspace", "3"],
            ["--prompt", "go", "--worktree", "x"],
            ["--issue", "7", "--project", "1"],
        ];
        for other in cases {
            let args = ["cornercase", "queue", "add", "--every", "2h"].into_iter().chain(other);
            let error = Cli::try_parse_from(args).expect_err("clap");
            assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict, "{other:?}");
        }
    }

    #[test]
    fn lists_a_routine_its_runs_and_a_flow() {
        let value = json!({"entries": [
            {"id": 1, "at": 0, "label": "review", "agent": "claude", "state": "pending", "repeat": "day 09:00", "next": 0},
            {"id": 2, "at": 0, "label": "triage", "agent": "claude", "state": "pending", "repeat": "2h", "paused": true},
            {"id": 3, "at": 0, "label": "fleet", "agent": "", "state": "pending", "repeat": "monday 09:00", "flow": "Quick"},
            {"id": 4, "at": 0, "label": "review", "agent": "claude", "state": "kept", "pane": 7, "server": "s",
             "reason": "2 uncommitted", "routine": 1},
            {"id": 5, "at": 0, "label": "review", "agent": "claude", "state": "removed", "routine": 1},
        ]});

        assert_eq!(
            lines(&value, "UTC"),
            "1 1970-01-01 00:00 pending review · claude · ↻ every day 09:00\n\
             2 1970-01-01 00:00 paused triage · claude · ↻ every 2h\n\
             3 1970-01-01 00:00 pending fleet · flow Quick · ↻ every monday 09:00\n\
             4 1970-01-01 00:00 kept: 2 uncommitted review · claude · routine 1\n\
             5 1970-01-01 00:00 removed review · claude · routine 1\n"
        );
    }
}
