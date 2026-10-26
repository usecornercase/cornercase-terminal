use std::path::PathBuf;

use clap::{CommandFactory, Parser};
use rstest::rstest;

use super::args::PluginAction;
use super::args::issue::IssueAction;
use super::args::todo::TodoAction;
use super::args::{KillServerArgs, RestartArgs, StartServerArgs, UpdateArgs};
use super::status::cell;
use super::*;
use crate::control::{
    GroupInfo, Ids, PaneInfo, PaneRows, ProjectInfo, PullRequestInfo, StackInfo, TabInfo, WorkspaceInfo,
};

pub(super) fn parse(args: &[&str]) -> std::result::Result<Cli, clap::Error> {
    Cli::try_parse_from(std::iter::once("cornercase").chain(args.iter().copied()))
}

mod parsing {
    use super::*;

    #[test]
    fn the_definitions_are_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn start_takes_an_issue_and_its_source_but_not_a_prompt_with_it() {
        let cli = parse(&["start", "claude", "--issue", "7", "--source", "github", "--project", "3"]).expect("parse");
        let Some(Command::Control(Control::Start(start))) = cli.command else { panic!("not a start") };
        assert_eq!(
            (start.issue.key.as_deref(), start.issue.source.as_deref(), start.project),
            (Some("7"), Some("github"), Some(3))
        );
        assert!(parse(&["start", "--issue", "7", "--prompt", "x"]).is_err());
        assert!(parse(&["start", "--source", "github"]).is_err());
        assert!(parse(&["start", "--issue", "7", "--source", "bugzilla"]).is_err());
        assert!(parse(&["start", "--issue", "7", "--project", "1", "--workspace", "2"]).is_err());
    }

    #[test]
    fn a_plugin_run_may_leave_out_its_target_for_a_button_action() {
        let run = |args: &[&str]| match parse(args).expect("parse").command {
            Some(Command::Plugin(plugin)) => plugin.action,
            _ => panic!("not plugin"),
        };
        let PluginAction::Run(alone) = run(&["plugin", "run", "a", "b"]) else { panic!("not run") };
        assert_eq!((alone.which.pane, alone.which.tab, alone.which.project), (None, None, None));
        let PluginAction::Run(on) = run(&["plugin", "run", "a", "b", "--pane", "4"]) else { panic!("not run") };
        assert_eq!(on.which.pane, Some(4));
        assert!(matches!(run(&["plugin", "popup-close"]), PluginAction::PopupClose));
        assert!(matches!(run(&["plugin", "actions", "--context", "button"]), PluginAction::Actions { .. }));
        let PluginAction::Status(print) = run(&["plugin", "status", "--json"]) else { panic!("not status") };
        assert!(print.json);
        let PluginAction::Status(print) = run(&["plugin", "status"]) else { panic!("not status") };
        assert!(!print.json);
    }

    #[test]
    fn a_flow_needs_a_task_or_an_issue() {
        assert!(parse(&["flow", "run", "Quick", "--issue", "ENG-42"]).is_ok());
        assert!(parse(&["flow", "run", "Quick"]).is_err());
        assert!(parse(&["flow", "run", "Quick", "--issue", "ENG-42", "--prompt", "x"]).is_err());
    }

    #[test]
    fn the_issue_filters_have_the_dialogs_names_as_aliases() {
        let cli = parse(&["issue", "list", "--owner", "me", "--reporter", "@bo", "--source", "jira"]).expect("parse");
        let Some(Command::Control(Control::Issue(args))) = cli.command else { panic!("not an issue command") };
        let IssueAction::List(list) = args.action else { panic!("not a list") };
        assert_eq!((list.assignee.as_deref(), list.author.as_deref()), (Some("me"), Some("@bo")));
        assert!(parse(&["issue", "people", "bugzilla"]).is_err());
        assert!(parse(&["issue", "read", "#7"]).is_ok());
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
        assert!(matches!(kill.command, Some(Command::KillServer(KillServerArgs { yes: false }))));
        assert!(matches!(restart.command, Some(Command::Restart(RestartArgs { yes: true, .. }))));
    }

    #[rstest]
    #[case::defaults(&["start-server"], (None, None, None, false))]
    #[case::a_size(&["start-server", "--cols", "200", "--rows", "60"], (Some(200), Some(60), None, false))]
    #[case::json(&["start-server", "--json"], (None, None, None, true))]
    #[case::a_theme(&["start-server", "--theme", "dark.json"], (None, None, Some("dark.json"), false))]
    #[case::a_theme_from_stdin(&["start-server", "--theme", "-"], (None, None, Some("-"), false))]
    fn start_server_takes_a_size_a_theme_and_json(
        #[case] args: &[&str],
        #[case] expected: (Option<u16>, Option<u16>, Option<&str>, bool),
    ) {
        let cli = parse(args).expect("parse");

        let Some(Command::StartServer(StartServerArgs { cols, rows, theme, print })) = cli.command else {
            panic!("not start-server")
        };
        assert_eq!((cols, rows, theme.as_deref(), print.json), expected);
    }

    #[rstest]
    #[case::read(&["theme"], (None, false))]
    #[case::a_file(&["theme", "dark.json"], (Some("dark.json"), false))]
    #[case::clear(&["theme", "--clear"], (None, true))]
    fn theme_takes_a_file_or_clear(#[case] args: &[&str], #[case] expected: (Option<&str>, bool)) {
        let cli = parse(args).expect("parse");

        let Some(Command::Control(Control::Theme(theme))) = cli.command else { panic!("not theme") };
        assert_eq!((theme.file.as_deref(), theme.clear), expected);
    }

    #[rstest]
    #[case::notices(&["events", "--notices"])]
    #[case::hidden_view(&["view", "--hidden"])]
    #[case::go(&["go", "cc-1-2"])]
    fn the_notice_commands_parse(#[case] args: &[&str]) {
        assert!(parse(args).is_ok());
    }

    #[test]
    fn theme_refuses_a_file_with_clear_and_a_view() {
        assert!(parse(&["theme", "dark.json", "--clear"]).is_err());
        assert!(parse(&["theme", "--view", "3"]).is_err());
    }

    #[rstest]
    #[case::plain(&["kill-server"], false)]
    #[case::short(&["kill-server", "-y"], true)]
    #[case::long(&["kill-server", "--yes"], true)]
    fn kill_server_takes_yes(#[case] args: &[&str], #[case] expected: bool) {
        let cli = parse(args).expect("parse");

        assert!(matches!(cli.command, Some(Command::KillServer(KillServerArgs { yes })) if yes == expected));
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
    #[case::a_surface_without_a_tab(&["surface", "--cols", "80", "--rows", "24"])]
    #[case::a_surface_without_a_size(&["surface", "--tab", "3"])]
    #[case::input_without_a_pane(&["input", r#"{"text":"x"}"#])]
    #[case::input_that_is_not_json(&["input", "--pane", "1", "hello"])]
    #[case::input_of_an_unknown_kind(&["input", "--pane", "1", r#"{"focus":true}"#])]
    #[case::scrolling_two_ways(&["scroll", "--pane", "1", "--up", "2", "--top"])]
    #[case::scrolling_no_way(&["scroll", "--pane", "1"])]
    #[case::scrolling_no_rows(&["scroll", "--pane", "1", "--up", "0"])]
    #[case::a_selection_end_alone(&["selection", "--pane", "1", "--to", "1,1"])]
    #[case::clearing_and_copying(&["selection", "--pane", "1", "--clear", "--copy"])]
    #[case::a_selection_start_alone(&["selection", "--pane", "1", "--from", "1,1"])]
    #[case::a_bad_cell(&["selection", "--pane", "1", "--from", "3", "--to", "4,4"])]
    #[case::a_link_without_a_cell(&["link", "--pane", "1"])]
    #[case::a_link_with_a_bad_cell(&["link", "--pane", "1", "--cell", "3"])]
    #[case::a_link_without_a_pane(&["link", "--cell", "1,2"])]
    #[case::a_ratio_over_one(&["ratio", "--tab", "1", "1.5"])]
    #[case::a_divider_in_the_middle(&["ratio", "--tab", "1", "--path", "middle", "0.5"])]
    #[case::resizing_nothing(&["resize", "--cols", "80", "--rows", "24"])]
    #[case::redrawing_nothing(&["redraw"])]
    #[case::a_flow_without_a_task(&["flow", "run", "Quick"])]
    #[case::an_import_without_a_file(&["flow", "import"])]
    #[case::an_import_to_nowhere(&["flow", "import", "f.toml", "--to", "other"])]
    #[case::a_save_without_a_file(&["flow", "save"])]
    #[case::a_save_to_nowhere(&["flow", "save", "d.json", "--to", "cloud"])]
    #[case::an_export_without_a_name(&["flow", "export"])]
    #[case::a_default_without_a_name(&["flow", "default"])]
    #[case::a_default_and_clear(&["flow", "default", "Quick", "--clear"])]
    #[case::an_order_without_names(&["flow", "order"])]
    #[case::an_order_and_clear(&["flow", "order", "Quick", "--clear"])]
    #[case::a_flow_with_two_tasks(&["flow", "run", "Quick", "--prompt", "x", "--prompt-file", "y"])]
    #[case::a_plugin_run_on_two_targets(&["plugin", "run", "a", "b", "--pane", "1", "--tab", "2"])]
    #[case::a_plugin_action_context_that_is_a_group(&["plugin", "actions", "--context", "group"])]
    #[case::a_negative_revision(&["plugin", "run", "a", "b", "--pane", "1", "--revision", "-1"])]
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
    fn a_todo_is_sent_to_a_pane_or_a_tab_but_not_both() {
        let cli = parse(&["todo", "send", "3", "--pane", "5", "--force", "--json"]).expect("parse");

        let Some(Command::Control(Control::Todo(todo))) = cli.command else { panic!("not todo") };
        let TodoAction::Send { id, pane, tab, force, print } = todo.action else { panic!("not send") };
        assert_eq!((id, pane, tab, force, print.json), (3, Some(5), None, true, true));
        assert!(parse(&["todo", "send", "3", "--pane", "5", "--tab", "6"]).is_err());
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
    fn a_flow_runs_by_name_with_its_task_and_branch() {
        let cli = parse(&["flow", "run", "Full issue", "--prompt", "Fix it", "--worktree", "fix", "--focus", "--json"]);

        let Some(Command::Control(Control::Flow(flow))) = cli.expect("parse").command else { panic!("not flow") };
        let args::FlowAction::Run(run) = flow.action else { panic!("not run") };
        assert_eq!(
            (run.name.as_str(), run.prompt.as_deref(), run.worktree.as_deref()),
            ("Full issue", Some("Fix it"), Some("fix"))
        );
        assert!(run.create.focus && run.create.print.json);
    }

    #[test]
    fn a_default_and_an_order_are_chosen_by_name_or_cleared() {
        let flow = |words: &[&str]| {
            let cli = parse(words).expect("parse");
            let Some(Command::Control(Control::Flow(flow))) = cli.command else { panic!("not flow") };
            flow.action
        };
        let args::FlowAction::Default(default) = flow(&["flow", "default", "Quick"]) else { panic!("not default") };
        assert_eq!((default.name.as_deref(), default.clear), (Some("Quick"), false));
        let args::FlowAction::Default(default) = flow(&["flow", "default", "--clear", "--json"]) else {
            panic!("not default")
        };
        assert_eq!((default.name, default.clear, default.print.json), (None, true, true));
        let args::FlowAction::Order(order) = flow(&["flow", "order", "A", "B", "--project", "2"]) else {
            panic!("not order")
        };
        assert_eq!((order.names, order.clear, order.project), (vec!["A".to_string(), "B".to_string()], false, Some(2)));
    }

    #[test]
    fn a_flow_is_exported_and_imported_with_its_options() {
        let cli = parse(&["flow", "export", "Quick", "--json"]).expect("parse");
        let Some(Command::Control(Control::Flow(flow))) = cli.command else { panic!("not flow") };
        let args::FlowAction::Export(export) = flow.action else { panic!("not export") };
        assert_eq!((export.name.as_str(), export.print.json), ("Quick", true));

        let cli =
            parse(&["flow", "import", "f.toml", "--to", "project", "--name", "X", "--replace", "--yes", "--json"])
                .expect("parse");
        let Some(Command::Control(Control::Flow(flow))) = cli.command else { panic!("not flow") };
        let args::FlowAction::Import(import) = flow.action else { panic!("not import") };
        assert_eq!(
            (import.source, import.to, import.name.as_deref()),
            ("f.toml".to_string(), args::ImportTo::Project, Some("X"))
        );
        assert!(import.replace && import.yes && import.print.json);

        let cli = parse(&["flow", "import", "-"]).expect("parse");
        let Some(Command::Control(Control::Flow(flow))) = cli.command else { panic!("not flow") };
        let args::FlowAction::Import(import) = flow.action else { panic!("not import") };
        assert_eq!((import.source, import.to, import.yes), ("-".to_string(), args::ImportTo::Yours, false));
    }

    #[test]
    fn a_setup_can_be_previewed_without_importing_it() {
        let cli = parse(&["config", "import", "team.toml", "--only", "agents,interface", "--preview", "--json"])
            .expect("parse");
        let Some(Command::Control(Control::Config(config))) = cli.command else { panic!("not config") };
        let Some(args::config::ConfigAction::Import { source, only, yes, preview }) = config.action else {
            panic!("not import")
        };
        assert_eq!((source, only), ("team.toml".to_string(), vec!["agents".to_string(), "interface".into()]));
        assert!(preview && config.json && !yes);
        assert!(parse(&["config", "import", "team.toml", "--preview", "--yes"]).is_err());
    }

    #[test]
    fn status_follows_with_or_without_json_but_never_with_panes() {
        for args in [&["status", "--follow"][..], &["status", "--follow", "--json"]] {
            let Some(Command::Control(Control::Status(status))) = parse(args).expect("parse").command else {
                panic!("not status")
            };
            assert!(status.follow && !status.panes);
        }

        let refused = parse(&["status", "--follow", "--panes"]).expect_err("conflict");
        assert_eq!(refused.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn status_app_follows_but_never_with_panes_or_a_view() {
        for args in [&["status", "--app"][..], &["status", "--app", "--follow"]] {
            let Some(Command::Control(Control::Status(status))) = parse(args).expect("parse").command else {
                panic!("not status")
            };
            assert!(status.app && !status.panes);
        }
        for args in [&["status", "--app", "--panes"][..], &["status", "--app", "--view", "2"]] {
            let refused = parse(args).expect_err("conflict");
            assert_eq!(refused.kind(), clap::error::ErrorKind::ArgumentConflict);
        }
    }

    #[rstest]
    #[case::status(&["status", "--view", "41"])]
    #[case::focus(&["focus", "--tab", "3", "--view", "41"])]
    #[case::new_tab(&["new-tab", "--focus", "--view", "41"])]
    #[case::start(&["start", "claude", "--view", "41"])]
    #[case::send(&["send", "--view", "41", "--enter"])]
    #[case::keys(&["keys", "--view", "41", "enter"])]
    #[case::read(&["read", "--view", "41"])]
    #[case::wait(&["wait", "--view", "41"])]
    #[case::rename(&["rename", "--view", "41", "x"])]
    #[case::surface(&["surface", "--view", "41"])]
    #[case::view_set(&["view-set", "--view", "41", "--claim"])]
    #[case::go(&["go", "cc-1-2", "--view", "41"])]
    fn view_names_the_view_a_command_acts_in(#[case] args: &[&str]) {
        let cli = parse(args).expect("parse");

        let Some(Command::Control(control)) = cli.command else { panic!("not a control command") };
        assert_eq!(control.view(), Some(41));
    }

    #[test]
    fn commands_without_a_view_name_none() {
        let cli = parse(&["status"]).expect("parse");

        let Some(Command::Control(control)) = cli.command else { panic!("not a control command") };
        assert_eq!(control.view(), None);
    }

    #[rstest]
    #[case::view_with_a_tab(&["surface", "--view", "4", "--tab", "3"])]
    #[case::view_with_a_size(&["surface", "--view", "4", "--cols", "80", "--rows", "24"])]
    #[case::half_a_size(&["view", "--cols", "80"])]
    #[case::two_starts(&["view", "--tab", "3", "--project", "1"])]
    #[case::follow_with_a_view(&["status", "--follow", "--view", "4"])]
    #[case::set_without_a_view(&["view-set", "--claim"])]
    #[case::set_both_ways(&["view-set", "--view", "4", "--active", "--inactive"])]
    #[case::visible_and_hidden(&["view-set", "--view", "4", "--visible", "--hidden"])]
    fn a_view_is_refused_where_it_makes_no_sense(#[case] args: &[&str]) {
        let refused = parse(args).expect_err("refused");

        assert!(
            matches!(
                refused.kind(),
                clap::error::ErrorKind::ArgumentConflict | clap::error::ErrorKind::MissingRequiredArgument
            ),
            "{refused}"
        );
    }

    #[test]
    fn a_view_opens_with_a_size_and_a_place() {
        let cli = parse(&["view", "--cols", "120", "--rows", "40", "--inactive", "--tab", "7"]).expect("parse");

        let Some(Command::Control(Control::View(view))) = cli.command else { panic!("not view") };
        assert_eq!((view.cols, view.rows, view.inactive, view.tab), (Some(120), Some(40), true, Some(7)));
    }

    #[test]
    fn surface_takes_a_tab_and_a_size() {
        let cli = parse(&["surface", "--tab", "3", "--cols", "120", "--rows", "40"]).expect("parse");

        let Some(Command::Control(Control::Surface(surface))) = cli.command else { panic!("not surface") };
        assert_eq!((surface.tab, surface.cols, surface.rows), (Some(3), Some(120), Some(40)));
    }

    #[rstest]
    #[case::a_tab(&["surface", "--tab", "3", "--cols", "120", "--rows", "40", "--patches"])]
    #[case::a_view(&["surface", "--view", "4", "--patches"])]
    fn surface_can_ask_for_patches(#[case] args: &[&str]) {
        let cli = parse(args).expect("parse");

        let Some(Command::Control(Control::Surface(surface))) = cli.command else { panic!("not surface") };
        assert!(surface.patches);
    }

    #[test]
    fn redraw_names_its_surface() {
        let cli = parse(&["redraw", "--surface", "4"]).expect("parse");

        let Some(Command::Control(Control::Redraw(redraw))) = cli.command else { panic!("not redraw") };
        assert_eq!(redraw.surface, 4);
    }

    #[test]
    fn input_keeps_its_events_in_order() {
        let cli = parse(&["input", "--pane", "3", r#"{"text":"x"}"#, r#"{"key":{"code":"enter"}}"#]).expect("parse");

        let Some(Command::Control(Control::Input(input))) = cli.command else { panic!("not input") };
        assert_eq!(
            (input.pane, input.events),
            (3, vec![r#"{"text":"x"}"#.to_string(), r#"{"key":{"code":"enter"}}"#.to_string()])
        );
    }

    #[test]
    fn link_takes_its_pane_and_cell() {
        let cli = parse(&["link", "--pane", "4", "--cell", "10,3", "--json"]).expect("parse");

        let Some(Command::Control(Control::Link(link))) = cli.command else { panic!("not link") };
        assert_eq!((link.pane, link.cell.x, link.cell.y, link.print.json), (4, 10, 3, true));
    }

    #[test]
    fn ratio_names_its_divider_from_the_outside_in() {
        let cli = parse(&["ratio", "--tab", "3", "--path", "second,first", "0.25"]).expect("parse");

        let Some(Command::Control(Control::Ratio(ratio))) = cli.command else { panic!("not ratio") };
        assert_eq!((ratio.tab, ratio.ratio, ratio.path.len()), (3, 0.25, 2));
    }

    #[test]
    fn start_takes_a_model_and_a_mode() {
        let cli = parse(&["start", "claude", "--mode", "plan", "--model", "opus", "--effort", "high"]).expect("parse");

        let Some(Command::Control(Control::Start(start))) = cli.command else { panic!("not start") };
        let chosen = (start.mode.as_deref(), start.model.as_deref(), start.effort.as_deref());
        assert_eq!(chosen, (Some("plan"), Some("opus"), Some("high")));
    }

    #[test]
    fn the_server_stays_out_of_the_help() {
        let help = Cli::command().render_help().to_string();

        assert!(!help.lines().any(|line| line.trim_start().starts_with("server ")), "{help}");
    }
}

mod output {
    use super::*;

    fn pane(id: u64, program: &str, active: bool) -> PaneInfo {
        PaneInfo {
            id,
            path: Some(PathBuf::from("/home/ana/shop")),
            program: Some(program.into()),
            active,
            ..PaneInfo::default()
        }
    }

    fn tab() -> TabInfo {
        let claude = PaneInfo {
            agent: Some("claude".into()),
            status: Some("working".into()),
            model: Some("Opus 5.5".into()),
            context: Some(23),
            caller: true,
            ..pane(5, "claude", false)
        };
        TabInfo {
            id: 3,
            name: "claude".into(),
            status: Some("working".into()),
            active: true,
            panes: vec![pane(4, "zsh", true), claude],
            custom_name: None,
            layout: None,
            controller: None,
        }
    }

    fn workspace() -> WorkspaceInfo {
        WorkspaceInfo {
            id: 2,
            name: "fix/login".into(),
            path: PathBuf::from("/home/ana/.cornercase/worktrees/shop/fix-login"),
            branch: Some("fix/login".into()),
            worktree: true,
            behind: 2,
            pull_request: None,
            setup: None,
            run_dir: None,
            run: None,
            active: true,
            custom_name: None,
            collapsed: false,
            changes: None,
            closed_agents: None,
            post: None,
            tabs: vec![tab()],
        }
    }

    fn project(id: u64, name: &str, group: Option<u64>) -> ProjectInfo {
        ProjectInfo { id, name: name.into(), path: format!("/home/ana/{name}").into(), group, ..ProjectInfo::default() }
    }

    fn report() -> Report {
        Report {
            version: "0.9.0".into(),
            boot: String::new(),
            caller: Some(5),
            shown: Ids { project: Some(1), workspace: Some(2), tab: Some(3), pane: Some(4) },
            groups: vec![GroupInfo { id: 8, name: "work".into(), collapsed: true, icon: String::new(), colour: 0 }],
            projects: vec![
                ProjectInfo { workspaces: vec![workspace()], ..project(1, "shop", None) },
                project(9, "api", Some(8)),
            ],
            keep_awake: None,
            view: None,
            theme: None,
            config: None,
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
    fn a_workspace_with_a_pull_request_names_it_and_its_checks() {
        let mut report = report();
        let pull = |checks: Option<&str>| PullRequestInfo {
            number: 482,
            state: "open".into(),
            checks: checks.map(String::from),
            ..PullRequestInfo::default()
        };
        report.projects[0].workspaces[0].pull_request = Some(pull(Some("failing")));
        let failing = render(&report, Some(Path::new("/home/ana")));
        report.projects[0].workspaces[0].pull_request = Some(pull(None));
        let unchecked = render(&report, Some(Path::new("/home/ana")));

        assert!(failing.contains("worktree  2 behind  pull request #482 open, checks failing  (shown)\n"), "{failing}");
        assert!(unchecked.contains("worktree  2 behind  pull request #482 open  (shown)\n"), "{unchecked}");
    }

    #[test]
    fn a_pull_request_with_conflicts_says_so_before_its_checks() {
        let mut report = report();
        let mut pull = PullRequestInfo {
            number: 482,
            state: "open".into(),
            checks: Some("passing".into()),
            conflicts: true,
            base: Some("main".into()),
            ..PullRequestInfo::default()
        };
        report.projects[0].workspaces[0].pull_request = Some(pull.clone());
        let named = render(&report, Some(Path::new("/home/ana")));
        pull.base = None;
        report.projects[0].workspaces[0].pull_request = Some(pull);
        let bare = render(&report, Some(Path::new("/home/ana")));

        assert!(named.contains("pull request #482 open, conflicts with main, checks passing  (shown)\n"), "{named}");
        assert!(bare.contains("pull request #482 open, conflicts, checks passing  (shown)\n"), "{bare}");
    }

    #[test]
    fn a_pull_request_in_a_stack_names_its_layer() {
        let mut report = report();
        report.projects[0].workspaces[0].pull_request = Some(PullRequestInfo {
            number: 483,
            state: "open".into(),
            checks: Some("passing".into()),
            stack: Some(StackInfo { number: 484, base: "main".into(), position: 2, size: 3, layers: Vec::new() }),
            ..PullRequestInfo::default()
        });

        let said = render(&report, Some(Path::new("/home/ana")));

        assert!(
            said.contains("pull request #483 open, checks passing, layer 2 of 3 of stack #484 on main  (shown)\n"),
            "{said}"
        );
    }

    #[test]
    fn a_workspace_with_a_merge_request_names_it_and_its_pipeline() {
        let mut report = report();
        report.projects[0].workspaces[0].pull_request = Some(PullRequestInfo {
            number: 486,
            state: "open".into(),
            checks: Some("passing".into()),
            conflicts: true,
            base: Some("main".into()),
            provider: "gitlab".into(),
            ..PullRequestInfo::default()
        });

        let text = render(&report, Some(Path::new("/home/ana")));

        assert!(text.contains("merge request !486 open, conflicts with main, pipeline passing  (shown)\n"), "{text}");
    }

    #[test]
    fn an_agent_left_with_a_background_shell_still_reads_working() {
        let mut report = report();
        report.projects[0].workspaces[0].tabs[0].panes[1].background_shell = true;

        let text = render(&report, Some(Path::new("/home/ana")));

        assert!(text.contains("pane 5  claude  working (background shell)  Opus 5.5 · 23%  ~/shop  (you)\n"), "{text}");
    }

    #[test]
    fn an_agent_whose_context_was_compacted_says_so_where_the_share_goes() {
        let mut report = report();
        let pane = &mut report.projects[0].workspaces[0].tabs[0].panes[1];
        pane.context = None;
        pane.compacted = true;

        let text = render(&report, Some(Path::new("/home/ana")));
        assert!(text.contains("pane 5  claude  working  Opus 5.5 · compacted  ~/shop  (you)\n"), "{text}");

        report.projects[0].workspaces[0].tabs[0].panes[1].model = None;
        let text = render(&report, Some(Path::new("/home/ana")));
        assert!(text.contains("pane 5  claude  working  compacted  ~/shop  (you)\n"), "{text}");
    }

    #[test]
    fn an_agent_with_subagents_says_how_many() {
        let mut report = report();
        report.projects[0].workspaces[0].tabs[0].panes[1].subagents = 2;

        let text = render(&report, Some(Path::new("/home/ana")));

        assert!(text.contains("pane 5  claude  working (2 subagents)  Opus 5.5 · 23%  ~/shop  (you)\n"), "{text}");
    }

    #[test]
    fn a_pane_started_from_another_names_its_parent() {
        let mut report = report();
        report.projects[0].workspaces[0].tabs[0].panes[1].parent = Some(4);

        let text = render(&report, Some(Path::new("/home/ana")));

        assert!(text.contains("pane 5  claude  working  Opus 5.5 · 23%  ~/shop  (parent 4, you)\n"), "{text}");
    }

    #[test]
    fn a_collapsed_agent_says_so() {
        let mut report = report();
        report.projects[0].workspaces[0].tabs[0].panes[1].parent = Some(4);
        report.projects[0].workspaces[0].tabs[0].panes[1].collapsed = true;

        let text = render(&report, Some(Path::new("/home/ana")));

        assert!(text.contains("~/shop  (parent 4, collapsed, you)\n"), "{text}");
    }

    #[test]
    fn a_flow_member_names_its_role() {
        let mut report = report();
        report.projects[0].workspaces[0].tabs[0].panes[1].parent = Some(4);
        report.projects[0].workspaces[0].tabs[0].panes[1].role = Some("coder".into());

        let text = render(&report, Some(Path::new("/home/ana")));

        assert!(
            text.contains("pane 5  claude  working  Opus 5.5 · 23%  ~/shop  (parent 4, role coder, you)\n"),
            "{text}"
        );
    }

    #[test]
    fn panes_are_one_row_each_under_a_header_with_a_dash_for_what_is_missing() {
        let mut report = report();
        report.projects[0].workspaces[0].tabs[0].panes[0].at_prompt = Some(true);
        report.projects[0].workspaces[0].tabs[0].panes[1].survey = true;
        report.projects[0].workspaces[0].tabs[0].panes[1].resuming = true;
        report.projects[0].workspaces[0].tabs[0].panes[1].parent = Some(4);
        report.projects[0].workspaces[0].tabs[0].panes[1].subagents = 2;
        report.projects[0].workspaces[0].tabs[0].panes[1].role = Some("coder".into());
        report.projects[0].workspaces[0].tabs[0].panes[1].held_sends = 1;
        report.projects[0].workspaces[0].tabs[0].panes[1].draft = true;
        report.projects[0].workspaces[0].tabs[0].panes[1].account_restart = Some("waiting".into());
        report.projects[0].workspaces[0].tabs[0].panes[1].pid = Some(321);
        report.projects[0].workspaces[0].tabs[0].panes[1].memory = Some(4096);
        report.projects[0].workspaces[0].tabs[0].panes[1].compacted = true;

        let text = pane_table(&report.panes()).expect("a table");

        assert_eq!(
            text,
            "pane\ttab\tworkspace\tproject\tprogram\tagent\tstatus\tbackground_shell\tdialog\tat_prompt\tmodel\t\
                 context\tcaller\tshown\tactive\ttab_name\tworkspace_name\tbranch\tworktree\tproject_name\tpath\t\
                 survey\tresuming\tparent\tsubagents\trole\theld_sends\tdraft\taccount_restart\tpid\tmemory\tcompacted\n\
                 4\t3\t2\t1\tzsh\t-\t-\tfalse\tfalse\ttrue\t-\t-\tfalse\ttrue\ttrue\t\
                 claude\tfix/login\tfix/login\ttrue\tshop\t/home/ana/shop\tfalse\tfalse\t-\t0\t-\t0\tfalse\t-\t-\t-\tfalse\n\
                 5\t3\t2\t1\tclaude\tclaude\tworking\tfalse\tfalse\t-\tOpus 5.5\t23\ttrue\tfalse\tfalse\t\
                 claude\tfix/login\tfix/login\ttrue\tshop\t/home/ana/shop\ttrue\ttrue\t4\t2\tcoder\t1\ttrue\twaiting\t321\t4096\ttrue\n"
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
    #[case::a_pane_opened(What::Opened { kind: control::Kind::Pane, parent: None, accounts: std::collections::BTreeMap::new() }, PANE, "pane 4  opened in tab 3")]
    #[case::a_pane_opened_from_another(
        What::Opened { kind: control::Kind::Pane, parent: Some(2), accounts: std::collections::BTreeMap::new() },
        PANE,
        "pane 4  opened in tab 3 from pane 2"
    )]
    #[case::a_project_opened(
        What::Opened { kind: control::Kind::Project, parent: None, accounts: std::collections::BTreeMap::new() },
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
    #[case::a_clipboard_write(What::Clipboard { text: "hello".into() }, PANE, "pane 4  clipboard  5 bytes")]
    #[case::one_byte(What::Clipboard { text: "a".into() }, PANE, "pane 4  clipboard  1 byte")]
    #[case::a_bell(What::Bell { count: 1 }, PANE, "pane 4  bell")]
    #[case::bells(What::Bell { count: 3 }, PANE, "pane 4  bell  3 times")]
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
    #[case::not_waiting(None, "working")]
    #[case::waiting_to_restart(Some("waiting"), "working (restarts after this turn)")]
    fn an_agent_waiting_to_restart_says_so_after_its_status(#[case] restart: Option<&str>, #[case] shown: &str) {
        let mut report = report();
        let pane = &mut report.projects[0].workspaces[0].tabs[0].panes[1];
        pane.account_restart = restart.map(str::to_string);

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

mod away;
mod flows;
mod focus;
mod known_agents;
