use std::path::PathBuf;

use clap::{CommandFactory, Parser};
use rstest::rstest;

use super::args::{RestartArgs, UpdateArgs};
use super::status::cell;
use super::*;
use crate::control::{GroupInfo, Ids, PaneInfo, PaneRows, ProjectInfo, TabInfo, WorkspaceInfo};

fn parse(args: &[&str]) -> std::result::Result<Cli, clap::Error> {
    Cli::try_parse_from(std::iter::once("cornercase").chain(args.iter().copied()))
}

mod parsing {
    use super::*;

    #[test]
    fn the_definitions_are_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn no_command_opens_the_window() {
        assert!(parse(&[]).expect("parse").command.is_none());
    }

    #[test]
    fn a_command_after_two_dashes_keeps_its_words() {
        let cli = parse(&["new-tab", "--name", "tests", "--", "cargo", "test", "--locked"]).expect("parse");

        let Some(Command::Control(Control::NewTab(new))) = cli.command else { panic!("not new-tab") };
        assert_eq!((new.name.as_deref(), typed(&new.command)), (Some("tests"), Some("cargo test --locked".into())));
    }

    #[test]
    fn a_dash_reads_the_text_from_standard_input() {
        let cli = parse(&["send", "--pane", "4", "--enter", "-"]).expect("parse");

        let Some(Command::Control(Control::Send(send))) = cli.command else { panic!("not send") };
        assert_eq!((send.target.pane, send.text.as_deref()), (Some(4), Some("-")));
    }

    #[test]
    fn the_old_commands_stay() {
        let update = parse(&["update", "--check", "-y"]).expect("parse");
        let kill = parse(&["kill-server"]).expect("parse");
        let restart = parse(&["restart", "--yes"]).expect("parse");

        assert!(matches!(update.command, Some(Command::Update(UpdateArgs { check: true, yes: true, .. }))));
        assert!(matches!(kill.command, Some(Command::KillServer)));
        assert!(matches!(restart.command, Some(Command::Restart(RestartArgs { yes: true, .. }))));
    }

    #[rstest]
    #[case::now(&["restart"], update::When::Now)]
    #[case::when_idle(&["restart", "--when-idle"], update::When::Idle { timeout: None })]
    #[case::with_a_timeout(&["update", "--when-idle", "--timeout", "90"], update::When::Idle { timeout: Some(90.0) })]
    fn a_restart_can_wait_for_the_agents(#[case] args: &[&str], #[case] expected: update::When) {
        let when = match parse(args).expect("parse").command {
            Some(Command::Restart(RestartArgs { when, .. }) | Command::Update(UpdateArgs { when, .. })) => when,
            other => panic!("not a restart: {other:?}"),
        };

        assert_eq!(when.when(), expected);
    }

    #[rstest]
    #[case::wait_without_enter(&["send", "--wait", "hi"])]
    #[case::a_timeout_without_enter(&["send", "--timeout", "5", "hi"])]
    #[case::sending_until_without_waiting(&["send", "--enter", "--until", "turn-over", "hi"])]
    #[case::starting_until_without_waiting(&["start", "--until", "turn-over"])]
    #[case::nothing_to_send(&["send", "--pane", "1"])]
    #[case::a_pane_and_a_tab(&["send", "--pane", "1", "--tab", "2", "hi"])]
    #[case::nothing_to_close(&["close"])]
    #[case::two_things_to_close(&["close", "--pane", "1", "--tab", "2"])]
    #[case::removing_the_worktree_of_a_tab(&["close", "--tab", "1", "--remove-worktree"])]
    #[case::forcing_without_removing(&["close", "--workspace", "1", "--force"])]
    #[case::two_conditions(&["wait", "--until", "idle", "--text", "x"])]
    #[case::an_unknown_state(&["wait", "--until", "sleeping"])]
    #[case::a_broken_pattern(&["wait", "--text", "("])]
    #[case::a_negative_timeout(&["wait", "--timeout", "-1"])]
    #[case::a_restart_timeout_without_waiting(&["restart", "--timeout", "60"])]
    #[case::checking_and_waiting(&["update", "--check", "--when-idle"])]
    #[case::any_and_all(&["wait", "--any", "--all", "--pane", "1", "--pane", "2"])]
    #[case::no_lines(&["read", "--lines", "0"])]
    #[case::lines_of_the_last_message(&["read", "--last-message", "--lines", "5"])]
    #[case::an_unknown_key(&["keys", "hello"])]
    #[case::two_prompts(&["start", "--prompt", "a", "--prompt-file", "b"])]
    #[case::a_worktree_and_a_workspace(&["start", "--worktree", "x", "--workspace", "1"])]
    #[case::two_things_to_rename(&["rename", "--tab", "1", "--group", "2", "x"])]
    #[case::a_branch_and_a_workspace_to_close(&["close", "--worktree", "x", "--workspace", "1"])]
    #[case::a_branch_and_a_tab_to_show(&["focus", "--worktree", "x", "--tab", "1"])]
    #[case::a_branch_and_a_workspace_for_a_tab(&["new-tab", "--worktree", "x", "--workspace", "1"])]
    #[case::a_branch_and_a_project_to_rename(&["rename", "--worktree", "x", "--project", "1", "y"])]
    #[case::a_blank_branch(&["close", "--worktree", " "])]
    #[case::an_unknown_command(&["frobnicate"])]
    fn wrong_usage_exits_with_2(#[case] args: &[&str]) {
        assert_eq!(parse(args).expect_err("wrong usage").exit_code(), 2);
    }

    #[rstest]
    #[case::two_panes(&["wait", "--pane", "1", "--pane", "2"], None)]
    #[case::a_pane_and_a_tab(&["wait", "--pane", "1", "--tab", "2"], None)]
    #[case::one_pane(&["wait", "--pane", "1"], Some(false))]
    #[case::any(&["wait", "--any", "--pane", "1", "--tab", "2"], Some(true))]
    #[case::all_of_one(&["wait", "--all", "--pane", "1"], Some(true))]
    fn several_panes_need_any_or_all(#[case] args: &[&str], #[case] expected: Option<bool>) {
        let Some(Command::Control(Control::Wait(wait))) = parse(args).expect("parse").command else {
            panic!("not wait")
        };

        match several(&wait) {
            Ok(several) => assert_eq!(Some(several), expected),
            Err(e) => assert_eq!((expected, e.exit_code()), (None, 2)),
        }
    }

    #[rstest]
    #[case::wait(&["wait", "--until", "turn-over"])]
    #[case::send(&["send", "--enter", "--wait", "--until", "turn-over", "hi"])]
    #[case::start(&["start", "claude", "--wait", "--until", "turn-over"])]
    fn turn_over_is_a_state_to_wait_for(#[case] args: &[&str]) {
        let until = match parse(args).expect("parse").command {
            Some(Command::Control(Control::Wait(wait))) => wait.until,
            Some(Command::Control(Control::Send(send))) => send.until,
            Some(Command::Control(Control::Start(start))) => start.until,
            _ => None,
        };

        assert_eq!(until.map(Until::from), Some(Until::TurnOver));
    }

    #[rstest]
    #[case::repeated(&["events", "--pane", "4", "--pane", "7"])]
    #[case::listed(&["events", "--pane", "4", "7", "--json"])]
    fn events_follow_the_panes_given(#[case] args: &[&str]) {
        let Some(Command::Control(Control::Events(events))) = parse(args).expect("parse").command else {
            panic!("not events")
        };

        assert_eq!(events.panes, [4, 7]);
    }

    #[test]
    fn send_gives_up_on_the_confirmation_after_a_timeout_and_can_force_its_way() {
        let cli = parse(&["send", "--enter", "--timeout", "5", "--force", "hi"]).expect("parse");

        let Some(Command::Control(Control::Send(send))) = cli.command else { panic!("not send") };
        assert_eq!((send.timeout, send.force, send.wait), (Some(5.0), true, false));
    }

    #[test]
    fn a_worktree_is_closed_by_its_branch() {
        let cli = parse(&["close", "--worktree", " fix/login ", "--remove-worktree", "--force"]).expect("parse");

        let Some(Command::Control(Control::Close(close))) = cli.command else { panic!("not close") };
        assert_eq!(close.which.worktree.as_deref(), Some("fix/login"));
        assert!(close.remove_worktree && close.force);
    }

    #[test]
    fn the_last_message_is_read_from_a_pane_or_a_tab() {
        let cli = parse(&["read", "--tab", "4", "--last-message", "--json"]).expect("parse");

        let Some(Command::Control(Control::Read(read))) = cli.command else { panic!("not read") };
        assert_eq!((read.tabs, read.last_message, read.print.json), (vec![4], true, true));
    }

    #[test]
    fn the_server_stays_out_of_the_help() {
        let help = Cli::command().render_help().to_string();

        assert!(!help.lines().any(|line| line.trim_start().starts_with("server ")), "{help}");
    }
}

mod output {
    use super::*;

    fn report() -> Report {
        let pane = |id, program: &str, active| PaneInfo {
            id,
            path: Some(PathBuf::from("/home/ana/shop")),
            program: Some(program.into()),
            active,
            ..PaneInfo::default()
        };
        let claude = PaneInfo {
            agent: Some("claude".into()),
            status: Some("working".into()),
            model: Some("Opus 5.5".into()),
            context: Some(23),
            caller: true,
            ..pane(5, "claude", false)
        };
        let tab = TabInfo {
            id: 3,
            name: "claude".into(),
            status: Some("working".into()),
            active: true,
            panes: vec![pane(4, "zsh", true), claude],
        };
        let workspace = WorkspaceInfo {
            id: 2,
            name: "fix/login".into(),
            path: PathBuf::from("/home/ana/.cornercase/worktrees/shop/fix-login"),
            branch: Some("fix/login".into()),
            worktree: true,
            behind: 2,
            active: true,
            tabs: vec![tab],
        };
        let project = |id, name: &str, group| ProjectInfo {
            id,
            name: name.into(),
            path: format!("/home/ana/{name}").into(),
            group,
            ..ProjectInfo::default()
        };
        Report {
            version: "0.9.0".into(),
            caller: Some(5),
            shown: Ids { project: Some(1), workspace: Some(2), tab: Some(3), pane: Some(4) },
            groups: vec![GroupInfo { id: 8, name: "work".into(), collapsed: true }],
            projects: vec![
                ProjectInfo { workspaces: vec![workspace], ..project(1, "shop", None) },
                project(9, "api", Some(8)),
            ],
            keep_awake: None,
        }
    }

    #[test]
    fn status_is_a_tree_of_ids_with_what_matters() {
        let text = render(&report(), Some(Path::new("/home/ana")));

        assert_eq!(
            text,
            "project 1  shop  ~/shop  (shown)\n\
                 \x20 workspace 2  fix/login  ~/.cornercase/worktrees/shop/fix-login  branch fix/login  worktree  2 behind  (shown)\n\
                 \x20   tab 3  claude  working  (shown)\n\
                 \x20     pane 4  zsh  ~/shop  (shown)\n\
                 \x20     pane 5  claude  working  Opus 5.5 · 23%  ~/shop  (you)\n\
                 group 8  work  collapsed\n\
                 \x20 project 9  api  ~/api\n"
        );
    }

    #[test]
    fn an_agent_left_with_a_background_shell_still_reads_working() {
        let mut report = report();
        report.projects[0].workspaces[0].tabs[0].panes[1].background_shell = true;

        let text = render(&report, Some(Path::new("/home/ana")));

        assert!(text.contains("pane 5  claude  working (background shell)  Opus 5.5 · 23%  ~/shop  (you)\n"), "{text}");
    }

    #[test]
    fn panes_are_one_row_each_under_a_header_with_a_dash_for_what_is_missing() {
        let mut report = report();
        report.projects[0].workspaces[0].tabs[0].panes[0].at_prompt = Some(true);
        report.projects[0].workspaces[0].tabs[0].panes[1].survey = true;

        let text = pane_table(&report.panes()).expect("a table");

        assert_eq!(
            text,
            "pane\ttab\tworkspace\tproject\tprogram\tagent\tstatus\tbackground_shell\tdialog\tat_prompt\tmodel\t\
                 context\tcaller\tshown\tactive\ttab_name\tworkspace_name\tbranch\tworktree\tproject_name\tpath\t\
                 survey\n\
                 4\t3\t2\t1\tzsh\t-\t-\tfalse\tfalse\ttrue\t-\t-\tfalse\ttrue\ttrue\t\
                 claude\tfix/login\tfix/login\ttrue\tshop\t/home/ana/shop\tfalse\n\
                 5\t3\t2\t1\tclaude\tclaude\tworking\tfalse\tfalse\t-\tOpus 5.5\t23\ttrue\tfalse\tfalse\t\
                 claude\tfix/login\tfix/login\ttrue\tshop\t/home/ana/shop\ttrue\n"
        );
    }

    #[test]
    fn panes_follow_the_sidebar_with_grouped_projects_after_the_loose_ones() {
        let one_pane = |project: u64, group| {
            let pane = PaneInfo { id: project * 10, ..PaneInfo::default() };
            let tab = TabInfo { id: project * 10 - 1, panes: vec![pane], ..TabInfo::default() };
            let workspace = WorkspaceInfo { id: project * 10 - 2, tabs: vec![tab], ..WorkspaceInfo::default() };
            ProjectInfo { id: project, group, workspaces: vec![workspace], ..ProjectInfo::default() }
        };
        let report = Report {
            groups: vec![GroupInfo { id: 8, ..GroupInfo::default() }, GroupInfo { id: 7, ..GroupInfo::default() }],
            projects: vec![one_pane(1, Some(7)), one_pane(2, Some(8)), one_pane(3, None), one_pane(4, Some(6))],
            ..Report::default()
        };

        let order: Vec<(u64, u64)> = report.panes().iter().map(|row| (row.project, row.pane)).collect();

        assert_eq!(order, [(3, 30), (4, 40), (2, 20), (1, 10)]);
    }

    #[rstest]
    #[case::nothing(Value::Null, "-")]
    #[case::an_empty_name(serde_json::json!(""), "-")]
    #[case::a_name_with_a_tab_and_a_line_break(serde_json::json!("a\tb\nc"), "a b c")]
    #[case::a_name_with_spaces(serde_json::json!("review #42"), "review #42")]
    #[case::a_flag(serde_json::json!(false), "false")]
    #[case::a_number(serde_json::json!(23), "23")]
    fn a_cell_never_breaks_its_row(#[case] value: Value, #[case] expected: &str) {
        assert_eq!(cell(&value), expected);
    }

    #[test]
    fn panes_as_json_are_an_object_holding_the_rows() {
        let rows = PaneRows { panes: report().panes() };

        let value = serde_json::to_value(&rows).expect("json");

        let panes = value["panes"].as_array().expect("a list");
        assert_eq!(panes.len(), 2);
        assert_eq!(
            (&panes[1]["pane"], &panes[1]["status"], &panes[1]["caller"], &panes[0]["agent"]),
            (&serde_json::json!(5), &serde_json::json!("working"), &serde_json::json!(true), &Value::Null)
        );
    }

    fn event(what: What, ids: Ids) -> Value {
        let event = Event { time: "2026-10-08T12:00:00.000Z".into(), what, ids };
        serde_json::to_value(event).expect("json")
    }

    const PANE: Ids = Ids { project: Some(1), workspace: Some(2), tab: Some(3), pane: Some(4) };
    const TAB: Ids = Ids { pane: None, ..PANE };

    #[rstest]
    #[case::a_pane_opened(What::Opened { kind: control::Kind::Pane }, PANE, "pane 4  opened in tab 3")]
    #[case::a_project_opened(
        What::Opened { kind: control::Kind::Project },
        Ids { project: Some(1), ..Ids::default() },
        "project 1  opened"
    )]
    #[case::a_tab_closed(What::Closed { kind: control::Kind::Tab }, TAB, "tab 3  closed")]
    #[case::an_agent_at_work(
        What::Status { agent: "claude".into(), from: Some("working".into()), to: Some("waiting".into()) },
        PANE,
        "pane 4  claude  working → waiting"
    )]
    #[case::an_agent_arrived(
        What::Status { agent: "codex".into(), from: None, to: Some("idle".into()) },
        PANE,
        "pane 4  codex  none → idle"
    )]
    #[case::a_program_ended(What::Exited { program: "cargo".into() }, PANE, "pane 4  cargo  exited")]
    #[case::one_dropped(
        What::Dropped { count: 1 },
        Ids::default(),
        "1 event dropped: this command fell behind; `cornercase status` shows where things are now"
    )]
    fn an_event_reads_as_one_line(#[case] what: What, #[case] ids: Ids, #[case] expected: &str) {
        assert_eq!(event_line(&event(what, ids)), expected);
    }

    #[test]
    fn an_event_from_a_newer_server_is_printed_as_it_came() {
        let value = serde_json::json!({"time": "2026-10-08T12:00:00.000Z", "event": "renamed", "tab": 3});

        assert_eq!(event_line(&value), value.to_string());
    }

    #[rstest]
    #[case::a_dialog(false, true, false, "idle (dialog open)")]
    #[case::both(true, true, false, "working (background shell, dialog open)")]
    #[case::the_survey(true, false, true, "working (background shell, survey open)")]
    fn what_covers_an_agent_is_said_after_its_status(
        #[case] background_shell: bool,
        #[case] dialog: bool,
        #[case] survey: bool,
        #[case] shown: &str,
    ) {
        let mut report = report();
        let pane = &mut report.projects[0].workspaces[0].tabs[0].panes[1];
        pane.status = Some(if background_shell { "working" } else { "idle" }.into());
        (pane.background_shell, pane.dialog, pane.survey) = (background_shell, dialog, survey);

        let text = render(&report, Some(Path::new("/home/ana")));

        assert!(text.contains(&format!("pane 5  claude  {shown}  Opus 5.5 · 23%")), "{text}");
    }

    #[rstest]
    #[case::a_state(Some("idle"), None, &["idle"])]
    #[case::a_matching_line(Some("text"), Some("test result: ok"), &["test result: ok"])]
    #[case::no_wait(None, None, &[])]
    fn the_ending_is_the_state_or_the_line(
        #[case] ended: Option<&str>,
        #[case] line: Option<&str>,
        #[case] expected: &[&str],
    ) {
        let done = Done { ended: ended.map(str::to_string), line: line.map(str::to_string), ..Done::default() };

        assert_eq!(ending(&done), expected);
    }

    #[test]
    fn several_panes_end_one_line_each_with_the_id_they_were_given() {
        let pane = |tab, pane, ended: &str, line: Option<&str>| Done {
            ids: Ids { tab, pane: Some(pane), ..Ids::default() },
            ended: Some(ended.into()),
            line: line.map(str::to_string),
            ..Done::default()
        };
        let done = Done {
            panes: vec![
                pane(None, 4, "idle", None),
                pane(Some(9), 12, "text", Some("ok")),
                pane(None, 7, "closed", None),
            ],
            ..Done::default()
        };

        assert_eq!(endings(&done), ["4 idle", "9 ok", "7 closed"]);
    }
}

mod known_agents {
    #[test]
    fn the_skill_names_every_known_agent() {
        for agent in crate::agents::Known::ALL {
            assert!(super::SKILL.contains(agent.title()), "missing {}", agent.title());
        }
    }
}
