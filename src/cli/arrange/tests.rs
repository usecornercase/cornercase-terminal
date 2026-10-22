use std::path::PathBuf;

use clap::Parser;
use rstest::rstest;

use super::*;
use crate::cli::args::{Cli, Command, Control};
use crate::control::arrange::{Locked, RoleTree};

fn parse(args: &[&str]) -> std::result::Result<Cli, clap::Error> {
    Cli::try_parse_from(std::iter::once("cornercase").chain(args.iter().copied()))
}

fn control(args: &[&str]) -> Control {
    match parse(args).expect("parse").command {
        Some(Command::Control(control)) => control,
        other => panic!("not a control command: {other:?}"),
    }
}

mod parsing {
    use super::*;

    #[rstest]
    #[case::a_tab_before(&["move", "--tab", "9", "--before", "7"])]
    #[case::a_group_last(&["move", "--group", "3", "--last"])]
    #[case::a_workspace_by_branch(&["move", "--worktree", "fix/login", "--last"])]
    #[case::a_project_into_a_group(&["move", "--project", "5", "--into", "12"])]
    #[case::a_project_out_of_its_group(&["move", "--project", "5", "--ungroup"])]
    #[case::a_pane_swap(&["move-pane", "--pane", "1", "--target", "2", "--place", "swap"])]
    #[case::a_pane_below_as_json(&["move-pane", "--pane", "1", "--target", "2", "--place", "below", "--json"])]
    #[case::a_collapsed_group(&["collapse", "--group", "3"])]
    #[case::an_expanded_workspace(&["expand", "--workspace", "4"])]
    #[case::a_folded_branch(&["collapse", "--worktree", "fix/login"])]
    #[case::a_collapsed_agent(&["collapse", "--pane", "5"])]
    #[case::an_expanded_agent(&["expand", "--pane", "5"])]
    #[case::a_new_group(&["group", "new", "side", "projects"])]
    #[case::a_style(&["group", "style", "3", "--icon", "★", "--colour", "99"])]
    #[case::a_style_read(&["group", "style", "3", "--json"])]
    #[case::a_deleted_group(&["group", "delete", "3", "--close-projects"])]
    #[case::unlocking(&["close", "--workspace", "1", "--remove-worktree", "--unlock", "--force"])]
    #[case::checking(&["close", "--worktree", "fix/login", "--remove-worktree", "--check", "--json"])]
    #[case::an_agent(&["close", "--agent", "--pane", "7"])]
    #[case::an_agent_checked(&["close", "--agent", "--pane", "7", "--check", "--json"])]
    #[case::an_agent_as_json(&["close", "--agent", "--pane", "7", "--json"])]
    fn the_arranging_commands_parse(#[case] args: &[&str]) {
        parse(args).expect("parse");
    }

    #[test]
    fn a_move_carries_its_row_and_place() {
        let Control::Move(args) = control(&["move", "--project", "5", "--into", "12"]) else { panic!("not move") };

        assert_eq!((args.project, args.into, args.before, args.last), (Some(5), Some(12), None, false));
    }

    #[test]
    fn a_fold_carries_its_pane() {
        let Control::Collapse(args) = control(&["collapse", "--pane", "5"]) else { panic!("not collapse") };

        assert_eq!((args.pane, args.group), (Some(5), None));
    }

    #[test]
    fn several_words_make_one_group_name() {
        let Control::Group(args) = control(&["group", "new", "side", "projects"]) else { panic!("not group") };
        let GroupAction::New { name, .. } = args.action else { panic!("not new") };

        assert_eq!(name.join(" "), "side projects");
    }

    #[rstest]
    #[case::a_move_without_a_place(&["move", "--tab", "1"])]
    #[case::a_move_without_a_row(&["move", "--before", "2"])]
    #[case::a_tab_into_a_group(&["move", "--tab", "1", "--into", "2"])]
    #[case::two_places(&["move", "--project", "1", "--before", "2", "--ungroup"])]
    #[case::a_pane_without_a_place(&["move-pane", "--pane", "1", "--target", "2"])]
    #[case::a_pane_on_a_diagonal(&["move-pane", "--pane", "1", "--target", "2", "--place", "diagonal"])]
    #[case::folding_nothing(&["collapse"])]
    #[case::folding_a_tab(&["collapse", "--tab", "1"])]
    #[case::folding_a_pane_and_a_group(&["collapse", "--pane", "1", "--group", "2"])]
    #[case::a_style_without_a_group(&["group", "style"])]
    #[case::a_group_without_a_name(&["group", "new"])]
    #[case::unlocking_without_removing(&["close", "--workspace", "1", "--unlock"])]
    #[case::checking_while_forcing(&["close", "--workspace", "1", "--remove-worktree", "--check", "--force"])]
    #[case::json_without_checking(&["close", "--workspace", "1", "--remove-worktree", "--json"])]
    #[case::an_agent_without_its_pane(&["close", "--agent"])]
    #[case::an_agent_by_its_tab(&["close", "--agent", "--tab", "1"])]
    #[case::an_agent_by_its_workspace(&["close", "--agent", "--workspace", "1"])]
    #[case::an_agent_and_its_worktree(&["close", "--agent", "--pane", "1", "--remove-worktree"])]
    #[case::a_branch_without_an_agent(&["close", "--workspace", "1", "--remove-worktree", "--delete-branch"])]
    #[case::an_agent_checked_and_forced(&["close", "--agent", "--pane", "1", "--check", "--force"])]
    #[case::an_agent_forced(&["close", "--agent", "--pane", "1", "--force"])]
    #[case::an_agent_unlocked(&["close", "--agent", "--pane", "1", "--unlock"])]
    #[case::an_agent_with_its_branch(&["close", "--agent", "--pane", "1", "--delete-branch"])]
    fn wrong_usage_exits_with_2(#[case] args: &[&str]) {
        assert_eq!(parse(args).expect_err("wrong usage").exit_code(), 2);
    }

    #[test]
    fn forcing_an_agent_names_the_agent() {
        let error = parse(&["close", "--agent", "--pane", "1", "--force"]).expect_err("wrong usage").to_string();

        assert!(error.contains("'--agent' cannot be used with '--force'"), "{error}");
    }
}

mod removal_text {
    use super::*;

    fn removal() -> Removal {
        Removal {
            workspace: 7,
            path: PathBuf::from("/w/fix-login"),
            branch: Some("fix/login".into()),
            ..Removal::default()
        }
    }

    #[test]
    fn a_clean_worktree_loses_nothing() {
        assert_eq!(removal_lines(&removal()), ["workspace 7  fix/login  /w/fix-login", "changes: none", "locked: no"]);
    }

    #[test]
    fn changes_a_lock_roles_and_a_teardown_are_all_said() {
        let removal = Removal {
            changed: true,
            lock: Some(Locked {
                reason: "claude session x".into(),
                path: PathBuf::from("/w/fix-login"),
                role: None,
                pid: Some(4242),
                running: Some(true),
            }),
            roles: vec![
                RoleTree { role: "coder A".into(), path: PathBuf::from("/w/a"), branch: "fix--a".into() },
                RoleTree { role: "coder B".into(), path: PathBuf::from("/w/b"), branch: "fix--b".into() },
            ],
            teardown: true,
            ..removal()
        };

        assert_eq!(
            removal_lines(&removal),
            [
                "workspace 7  fix/login  /w/fix-login",
                "changes: not committed, removing it deletes them (--force)",
                "locked: claude session x; process 4242 still runs and may be using it (--unlock)",
                "role worktrees: coder A in /w/a (branch fix--a); coder B in /w/b (branch fix--b), removed with it",
                "teardown: the project's teardown runs first",
            ]
        );
    }

    #[test]
    fn a_role_lock_names_the_role_and_a_dead_process() {
        let removal = Removal {
            lock: Some(Locked {
                reason: "pid 9".into(),
                path: PathBuf::from("/w/a"),
                role: Some("coder A".into()),
                pid: Some(9),
                running: Some(false),
            }),
            ..removal()
        };

        assert_eq!(
            removal_lines(&removal)[2],
            "locked: pid 9; the worktree of coder A in /w/a; process 9 is gone (--unlock)"
        );
    }
}

mod agent_text {
    use super::*;
    use crate::control::arrange::{AgentClosing, PaneRef};

    fn pane(pane: u64, agent: Option<&str>, workspace: u64) -> PaneRef {
        PaneRef { pane, agent: agent.map(str::to_string), project: 1, workspace, tab: pane }
    }

    #[test]
    fn closing_tabs_says_nothing_is_removed() {
        let closing = AgentClosing {
            pane: 7,
            agent: Some("claude".into()),
            workspace: 2,
            tabs: vec![4, 6],
            stops: vec![pane(7, Some("claude"), 2), pane(8, None, 2)],
            keeps: vec![pane(12, Some("codex"), 5)],
            ..AgentClosing::default()
        };

        assert_eq!(
            closing_lines(&closing),
            [
                "pane 7  claude  workspace 2",
                "closes: tabs 4, 6; nothing is removed from disk",
                "stops: pane 7 (claude), pane 8",
                "keeps running: pane 12 (codex) in workspace 5",
            ]
        );
    }

    #[test]
    fn a_team_says_its_run_ends() {
        let closing = AgentClosing {
            pane: 7,
            workspace: 7,
            tabs: vec![4],
            stops: vec![pane(7, None, 7)],
            run: Some(3),
            ..AgentClosing::default()
        };

        assert_eq!(
            closing_lines(&closing),
            [
                "pane 7  -  workspace 7",
                "closes: tabs 4; nothing is removed from disk",
                "stops: pane 7",
                "flow run 3 ends"
            ]
        );
    }
}
