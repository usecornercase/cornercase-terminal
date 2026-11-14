use clap::Parser as _;
use rstest::rstest;

use crate::cli::Cli;
use crate::cli::args::{Command, PluginAction};

fn parsed(args: &[&str]) -> Result<Cli, clap::Error> {
    Cli::try_parse_from(std::iter::once("cornercase").chain(args.iter().copied()))
}

fn boot_of(args: &[&str]) -> Option<String> {
    let cli = parsed(args).expect("parse");
    let Some(Command::Control(control)) = cli.command else { panic!("not a command for scripts") };
    control.boot().map(str::to_string)
}

mod boot {
    use super::*;

    #[rstest]
    #[case::a_pane(&["close", "--pane", "4"])]
    #[case::a_tab(&["close", "--tab", "4"])]
    #[case::a_workspace(&["close", "--workspace", "4"])]
    #[case::a_branch(&["close", "--worktree", "fix/login"])]
    #[case::a_project(&["close", "--project", "4"])]
    #[case::a_removal(&["close", "--workspace", "4", "--remove-worktree", "--force", "--unlock"])]
    #[case::a_check(&["close", "--workspace", "4", "--remove-worktree", "--check", "--json"])]
    #[case::an_agent(&["close", "--agent", "--pane", "4", "--check"])]
    #[case::rename_a_tab(&["rename", "--tab", "4", "x"])]
    #[case::rename_a_workspace(&["rename", "--workspace", "4", "x"])]
    #[case::rename_a_branch(&["rename", "--worktree", "fix/login", "x"])]
    #[case::rename_a_project(&["rename", "--project", "4", "x"])]
    #[case::rename_a_group(&["rename", "--group", "4", "x"])]
    #[case::rename_what_runs_here(&["rename", "x"])]
    #[case::mark_what_runs_here(&["mark"])]
    #[case::mark_a_pane(&["mark", "--pane", "4"])]
    #[case::mark_a_tab_reviewed(&["mark", "--tab", "4", "--reviewed"])]
    #[case::move_a_pane(&["move-pane", "--pane", "4", "--target", "5", "--place", "left", "--json"])]
    #[case::restart_every_pane(&["account", "restart", "claude"])]
    #[case::restart_a_pane_on_an_account(&["account", "restart", "claude", "work", "--pane", "4", "--json"])]
    #[case::restart_a_pane_on_its_own_login(&["account", "restart", "claude", "--system", "--pane", "4"])]
    #[case::read_the_context_here(&["context"])]
    #[case::read_a_panes_context(&["context", "--pane", "4", "--json"])]
    #[case::read_a_tabs_cached_context(&["context", "--tab", "4", "--cached"])]
    #[case::collapse_a_group(&["collapse", "--group", "4"])]
    #[case::collapse_a_project(&["collapse", "--project", "4"])]
    #[case::collapse_a_workspace(&["collapse", "--workspace", "4"])]
    #[case::collapse_a_branch(&["collapse", "--worktree", "fix/login"])]
    #[case::collapse_an_agent(&["collapse", "--pane", "4"])]
    #[case::expand_a_group(&["expand", "--group", "4"])]
    #[case::expand_a_project(&["expand", "--project", "4"])]
    #[case::expand_a_workspace(&["expand", "--workspace", "4"])]
    #[case::expand_a_branch(&["expand", "--worktree", "fix/login"])]
    #[case::expand_an_agent(&["expand", "--pane", "4"])]
    #[case::send_here(&["send", "x"])]
    #[case::send_to_a_pane(&["send", "--pane", "4", "--enter", "x"])]
    #[case::send_to_a_tab_and_wait(&["send", "--tab", "4", "--enter", "--wait", "--until", "turn-over", "--timeout", "60", "x"])]
    #[case::send_standard_input(&["send", "--pane", "4", "--force", "--json", "-"])]
    #[case::input_into_a_pane(&["input", "--pane", "4", r#"{"paste": "x"}"#])]
    #[case::input_from_standard_input(&["input", "--pane", "4", "--json", "-"])]
    #[case::input_into_a_popup(&["input", "--popup", "--pane", "4", r#"{"key": {"code": "enter"}}"#])]
    #[case::paste_a_file(&["paste-file", "--pane", "4", "shot.png"])]
    #[case::paste_standard_input(&["paste-file", "--pane", "4", "--name", "x.png", "--json"])]
    #[case::keys_a_pane(&["keys", "--pane", "4", "enter"])]
    #[case::keys_a_tab(&["keys", "--tab", "4", "ctrl+c"])]
    #[case::held_send_now(&["held", "--pane", "4", "--send-now"])]
    #[case::held_wait_turn(&["held", "--pane", "4", "--wait-turn", "--json"])]
    #[case::todo_send_to_a_pane(&["todo", "send", "3", "--pane", "4"])]
    #[case::todo_send_to_a_tab(&["todo", "send", "3", "--tab", "4", "--force"])]
    #[case::input_follow(&["input", "--pane", "4", "--follow", "-"])]
    #[case::input_follow_json(&["input", "--follow", "--json", "-"])]
    #[case::input_follow_into_a_popup(&["input", "--popup", "--follow", "-"])]
    #[case::scroll_up(&["scroll", "--pane", "4", "--up", "3"])]
    #[case::scroll_to_the_top(&["scroll", "--pane", "4", "--top"])]
    #[case::select_a_range(&["selection", "--pane", "4", "--from", "0,0", "--to", "3,1"])]
    #[case::clear_the_selection(&["selection", "--pane", "4", "--clear"])]
    #[case::copy_the_selection(&["selection", "--pane", "4", "--copy"])]
    #[case::split_here(&["split"])]
    #[case::split_a_pane_down(&["split", "--pane", "4", "--down"])]
    #[case::focus_a_pane(&["focus", "--pane", "4"])]
    #[case::focus_a_tab(&["focus", "--tab", "4"])]
    #[case::focus_a_workspace(&["focus", "--workspace", "4"])]
    #[case::focus_a_branch(&["focus", "--worktree", "fix/login"])]
    #[case::focus_a_project(&["focus", "--project", "4"])]
    #[case::focus_and_reveal(&["focus", "--pane", "4", "--reveal", "x", "--line", "2"])]
    #[case::set_a_ratio(&["ratio", "--tab", "4", "0.3"])]
    #[case::resize_a_surface(&["resize", "--surface", "4", "--cols", "80", "--rows", "24"])]
    #[case::redraw_a_surface(&["redraw", "--surface", "4"])]
    #[case::set_a_views_size(&["view-set", "--view", "7", "--cols", "80", "--rows", "24"])]
    #[case::claim_a_view(&["view-set", "--view", "7", "--claim"])]
    #[case::move_a_tab(&["move", "--tab", "4", "--last"])]
    #[case::move_a_project_into_a_group(&["move", "--project", "4", "--into", "2"])]
    #[case::move_a_branch(&["move", "--worktree", "x", "--before", "3"])]
    #[case::style_a_group(&["group", "style", "4", "--icon", "★"])]
    #[case::read_a_groups_style(&["group", "style", "4"])]
    #[case::delete_a_group(&["group", "delete", "4", "--close-projects"])]
    #[case::open_a_tab_here(&["new-tab"])]
    #[case::open_a_tab_in_a_workspace(&["new-tab", "--workspace", "4"])]
    #[case::open_a_tab_in_a_branch(&["new-tab", "--worktree", "x"])]
    #[case::run_the_setup_here(&["setup"])]
    #[case::run_the_setup_of_a_branch(&["setup", "--worktree", "x"])]
    #[case::start_in_a_workspace(&["start", "claude", "--workspace", "4"])]
    #[case::start_in_a_project(&["start", "--project", "4", "--prompt", "x"])]
    #[case::start_in_a_branch(&["start", "--worktree", "x"])]
    #[case::make_a_workspace(&["new-workspace", "x", "--project", "4"])]
    #[case::open_a_folder(&["open", "/x"])]
    #[case::open_a_folder_into_a_group(&["open", "/x", "--into", "2"])]
    #[case::close_agents_check(&["agents", "close", "--workspace", "4", "--check"])]
    #[case::reopen_agents(&["agents", "reopen", "--workspace", "4"])]
    #[case::forget_agents(&["agents", "forget", "--workspace", "4", "--yes"])]
    #[case::view_a_file(&["changes", "viewed", "a.rs", "--workspace", "4"])]
    #[case::clear_a_view(&["changes", "viewed", "a.rs", "--clear"])]
    #[case::stage_a_file(&["changes", "stage", "a.rs", "--workspace", "4"])]
    #[case::unstage_a_file(&["changes", "unstage", "a.rs", "--workspace", "4"])]
    #[case::discard_a_file(&["changes", "discard", "a.rs", "--workspace", "4", "--yes"])]
    #[case::choose_a_base(&["changes", "base", "main", "--workspace", "4"])]
    #[case::list_the_bases(&["changes", "base"])]
    #[case::queue_an_entry(&["queue", "add", "--workspace", "4", "--prompt", "x", "--at", "10:00"])]
    #[case::edit_an_entry(&["queue", "edit", "3", "--prompt", "y"])]
    #[case::add_an_account(&["account", "add", "claude", "work"])]
    #[case::log_an_account_in(&["account", "login", "claude", "work"])]
    #[case::log_in_to_the_own_login(&["account", "login", "claude", "--system"])]
    #[case::set_a_projects_scripts(&["config", "project", "--project", "4", "--setup", "make"])]
    #[case::set_a_projects_scripts_by_folder(&["config", "project", "/x", "--teardown", ""])]
    #[case::free_builds_of_a_workspace(&["worktrees", "free-builds", "--workspace", "4"])]
    #[case::free_builds_of_a_folder(&["worktrees", "free-builds", "--path", "/x", "--check"])]
    #[case::read_a_pane(&["read", "--pane", "4"])]
    #[case::read_here(&["read"])]
    #[case::read_a_tab_lines(&["read", "--tab", "4", "--lines", "10"])]
    #[case::read_several_last_messages(&["read", "--pane", "4", "--pane", "5", "--last-message", "--json"])]
    #[case::wait_for_a_state(&["wait", "--pane", "4", "--until", "turn-over", "--timeout", "60"])]
    #[case::wait_for_any(&["wait", "--pane", "4", "--tab", "5", "--any"])]
    #[case::wait_for_text(&["wait", "--tab", "4", "--text", "done"])]
    #[case::link_a_cell(&["link", "--pane", "4", "--cell", "3,1"])]
    #[case::follow_events(&["events"])]
    #[case::follow_a_panes_notices(&["events", "--pane", "4", "--notices", "--json"])]
    #[case::stream_a_tab(&["surface", "--tab", "4", "--cols", "80", "--rows", "24"])]
    #[case::stream_a_view(&["surface", "--view", "7", "--patches"])]
    #[case::stream_the_popup(&["surface", "--popup"])]
    #[case::open_a_view(&["view"])]
    #[case::open_a_view_on_a_tab(&["view", "--tab", "4", "--cols", "80", "--rows", "24"])]
    #[case::open_a_view_on_a_project(&["view", "--project", "4", "--hidden"])]
    #[case::list_processes(&["processes"])]
    #[case::list_the_processes_of_a_workspace(&["processes", "--workspace", "4", "--interval", "200"])]
    #[case::list_the_processes_of_a_branch(&["processes", "--worktree", "x"])]
    #[case::list_ports_of_a_workspace(&["ports", "--workspace", "4"])]
    #[case::list_every_port(&["ports", "--all"])]
    #[case::read_a_pull_request(&["pull-request", "--worktree", "x"])]
    #[case::list_changes(&["changes", "list", "--workspace", "4"])]
    #[case::follow_every_change(&["changes", "list", "--every", "--follow"])]
    #[case::read_a_diff(&["changes", "file", "a.rs", "--mode", "all"])]
    #[case::read_a_gap(&["changes", "gap", "a.rs", "--hunk", "1"])]
    #[case::list_shared_files(&["changes", "shared", "--worktree", "x"])]
    #[case::check_a_merge(&["changes", "merge-check", "--workspace", "4", "--with", "5"])]
    #[case::list_files(&["files", "list", "src", "--workspace", "4"])]
    #[case::read_a_file_following(&["files", "read", "a.rs", "--follow"])]
    #[case::search_files(&["files", "search", "x", "--names", "--worktree", "x"])]
    #[case::issue_sources(&["issue", "sources", "--project", "4"])]
    #[case::issue_list(&["issue", "list", "--project", "4"])]
    #[case::issue_read(&["issue", "read", "4", "--project", "4"])]
    #[case::issue_people(&["issue", "people", "github", "--project", "4"])]
    #[case::size_a_workspace(&["worktrees", "size", "--workspace", "4"])]
    #[case::size_a_folder(&["worktrees", "size", "--path", "/x"])]
    #[case::list_worktrees(&["worktrees", "list", "--project", "4"])]
    #[case::flow_list(&["flow", "list", "--project", "4"])]
    #[case::flow_run(&["flow", "run", "Pair", "--prompt", "x", "--project", "4"])]
    #[case::flow_run_in_a_view(&["flow", "run", "Pair", "--prompt", "x", "--focus", "--view", "7"])]
    #[case::flow_export(&["flow", "export", "Pair", "--project", "4"])]
    #[case::flow_import(&["flow", "import", "f.toml", "--to", "project", "--project", "4"])]
    #[case::flow_save(&["flow", "save", "f.json", "--project", "4"])]
    #[case::flow_messages(&["flow", "messages", "--workspace", "4", "--follow"])]
    #[case::flow_files(&["flow", "files", "--workspace", "4"])]
    #[case::flow_delete(&["flow", "delete", "Pair", "--project", "4"])]
    #[case::flow_default(&["flow", "default", "Pair", "--project", "4"])]
    #[case::flow_order(&["flow", "order", "A", "B", "--project", "4"])]
    #[case::flow_close_when_merged(&["flow", "close-when-merged", "--workspace", "4", "--off"])]
    #[case::go_to_a_notice(&["go", "cc-1-2"])]
    #[case::go_in_a_view(&["go", "cc-1-2", "--view", "7"])]
    #[case::clone_into_a_view(&["clone", "https://x/y.git", "--view", "7"])]
    #[case::clone_into_a_folder(&["clone", "https://x/y.git", "/x"])]
    fn goes_with_every_form_of_the_commands_that_take_it(#[case] args: &[&str]) {
        assert_eq!(boot_of(&[args, &["--boot", "1-2"]].concat()).as_deref(), Some("1-2"));
        assert_eq!(boot_of(args), None);
    }

    #[rstest]
    #[case::close(&["close", "--pane", "4", "--boot", ""])]
    #[case::rename(&["rename", "x", "--boot", ""])]
    #[case::mark(&["mark", "--pane", "4", "--boot", ""])]
    #[case::move_pane(&["move-pane", "--pane", "4", "--target", "5", "--place", "left", "--boot", ""])]
    #[case::account_restart(&["account", "restart", "claude", "--boot", ""])]
    #[case::context(&["context", "--boot", ""])]
    #[case::collapse(&["collapse", "--pane", "4", "--boot", ""])]
    #[case::expand(&["expand", "--group", "4", "--boot", ""])]
    #[case::send(&["send", "--pane", "4", "x", "--boot", ""])]
    #[case::input(&["input", "--pane", "4", r#"{"text": "x"}"#, "--boot", ""])]
    #[case::paste_file(&["paste-file", "--pane", "4", "--boot", ""])]
    #[case::keys(&["keys", "--pane", "4", "enter", "--boot", ""])]
    #[case::held(&["held", "--pane", "4", "--send-now", "--boot", ""])]
    #[case::todo_send(&["todo", "send", "3", "--pane", "4", "--boot", ""])]
    #[case::input_follow(&["input", "--pane", "4", "--follow", "-", "--boot", ""])]
    #[case::scroll(&["scroll", "--pane", "4", "--up", "3", "--boot", ""])]
    #[case::selection(&["selection", "--pane", "4", "--clear", "--boot", ""])]
    #[case::split(&["split", "--boot", ""])]
    #[case::focus(&["focus", "--pane", "4", "--boot", ""])]
    #[case::ratio(&["ratio", "--tab", "4", "0.3", "--boot", ""])]
    #[case::resize(&["resize", "--surface", "4", "--cols", "80", "--rows", "24", "--boot", ""])]
    #[case::redraw(&["redraw", "--surface", "4", "--boot", ""])]
    #[case::view_set(&["view-set", "--view", "7", "--claim", "--boot", ""])]
    #[case::move_a_tab_last(&["move", "--tab", "4", "--last", "--boot", ""])]
    #[case::group_style(&["group", "style", "4", "--boot", ""])]
    #[case::group_delete(&["group", "delete", "4", "--boot", ""])]
    #[case::new_tab(&["new-tab", "--boot", ""])]
    #[case::setup(&["setup", "--boot", ""])]
    #[case::start(&["start", "--boot", ""])]
    #[case::new_workspace(&["new-workspace", "x", "--boot", ""])]
    #[case::open(&["open", "/x", "--boot", ""])]
    #[case::agents_close(&["agents", "close", "--workspace", "4", "--boot", ""])]
    #[case::agents_reopen(&["agents", "reopen", "--workspace", "4", "--boot", ""])]
    #[case::agents_forget(&["agents", "forget", "--workspace", "4", "--yes", "--boot", ""])]
    #[case::changes_viewed(&["changes", "viewed", "a.rs", "--boot", ""])]
    #[case::changes_stage(&["changes", "stage", "a.rs", "--boot", ""])]
    #[case::changes_unstage(&["changes", "unstage", "a.rs", "--boot", ""])]
    #[case::changes_discard(&["changes", "discard", "a.rs", "--yes", "--boot", ""])]
    #[case::changes_base(&["changes", "base", "--boot", ""])]
    #[case::queue_add(&["queue", "add", "--workspace", "4", "--prompt", "x", "--at", "10:00", "--boot", ""])]
    #[case::queue_edit(&["queue", "edit", "3", "--prompt", "y", "--boot", ""])]
    #[case::account_add(&["account", "add", "claude", "work", "--boot", ""])]
    #[case::account_login(&["account", "login", "claude", "work", "--boot", ""])]
    #[case::config_project(&["config", "project", "--project", "4", "--setup", "make", "--boot", ""])]
    #[case::free_builds(&["worktrees", "free-builds", "--workspace", "4", "--boot", ""])]
    #[case::plugin_run(&["plugin", "run", "p", "a", "--pane", "4", "--boot", ""])]
    #[case::read_a_pane(&["read", "--pane", "4", "--boot", ""])]
    #[case::wait_for_a_state(&["wait", "--pane", "4", "--until", "turn-over", "--timeout", "60", "--boot", ""])]
    #[case::link_a_cell(&["link", "--pane", "4", "--cell", "3,1", "--boot", ""])]
    #[case::follow_events(&["events", "--boot", ""])]
    #[case::stream_a_tab(&["surface", "--tab", "4", "--cols", "80", "--rows", "24", "--boot", ""])]
    #[case::open_a_view(&["view", "--boot", ""])]
    #[case::list_processes(&["processes", "--boot", ""])]
    #[case::list_ports_of_a_workspace(&["ports", "--workspace", "4", "--boot", ""])]
    #[case::read_a_pull_request(&["pull-request", "--worktree", "x", "--boot", ""])]
    #[case::list_changes(&["changes", "list", "--workspace", "4", "--boot", ""])]
    #[case::read_a_diff(&["changes", "file", "a.rs", "--mode", "all", "--boot", ""])]
    #[case::read_a_gap(&["changes", "gap", "a.rs", "--hunk", "1", "--boot", ""])]
    #[case::list_shared_files(&["changes", "shared", "--worktree", "x", "--boot", ""])]
    #[case::check_a_merge(&["changes", "merge-check", "--workspace", "4", "--with", "5", "--boot", ""])]
    #[case::list_files(&["files", "list", "src", "--workspace", "4", "--boot", ""])]
    #[case::read_a_file_following(&["files", "read", "a.rs", "--follow", "--boot", ""])]
    #[case::search_files(&["files", "search", "x", "--names", "--worktree", "x", "--boot", ""])]
    #[case::issue_sources(&["issue", "sources", "--project", "4", "--boot", ""])]
    #[case::issue_list(&["issue", "list", "--project", "4", "--boot", ""])]
    #[case::issue_read(&["issue", "read", "4", "--project", "4", "--boot", ""])]
    #[case::issue_people(&["issue", "people", "github", "--project", "4", "--boot", ""])]
    #[case::size_a_workspace(&["worktrees", "size", "--workspace", "4", "--boot", ""])]
    #[case::list_worktrees(&["worktrees", "list", "--project", "4", "--boot", ""])]
    #[case::flow_list(&["flow", "list", "--project", "4", "--boot", ""])]
    #[case::flow_run(&["flow", "run", "Pair", "--prompt", "x", "--project", "4", "--boot", ""])]
    #[case::flow_export(&["flow", "export", "Pair", "--project", "4", "--boot", ""])]
    #[case::flow_import(&["flow", "import", "f.toml", "--to", "project", "--project", "4", "--boot", ""])]
    #[case::flow_save(&["flow", "save", "f.json", "--project", "4", "--boot", ""])]
    #[case::flow_messages(&["flow", "messages", "--workspace", "4", "--follow", "--boot", ""])]
    #[case::flow_files(&["flow", "files", "--workspace", "4", "--boot", ""])]
    #[case::flow_delete(&["flow", "delete", "Pair", "--project", "4", "--boot", ""])]
    #[case::flow_default(&["flow", "default", "Pair", "--project", "4", "--boot", ""])]
    #[case::flow_order(&["flow", "order", "A", "B", "--project", "4", "--boot", ""])]
    #[case::flow_close_when_merged(&["flow", "close-when-merged", "--workspace", "4", "--off", "--boot", ""])]
    #[case::go_to_a_notice(&["go", "cc-1-2", "--boot", ""])]
    #[case::clone_into_a_view(&["clone", "https://x/y.git", "--view", "7", "--boot", ""])]
    fn cannot_be_empty_so_an_unset_variable_never_reaches_the_server(#[case] args: &[&str]) {
        assert!(parsed(args).is_err());
    }

    #[rstest]
    #[case::status(&["status"])]
    #[case::config_set(&["config", "set", "a", "b"])]
    #[case::flow_runs(&["flow", "runs"])]
    #[case::flow_hide(&["flow", "hide", "Pair"])]
    #[case::flow_restore(&["flow", "restore", "Pair"])]
    #[case::processes_stop(&["processes", "stop", "5"])]
    #[case::ports_stop(&["ports", "stop", "8080"])]
    #[case::worktrees_remove(&["worktrees", "remove", "--path", "/x"])]
    #[case::todo_done(&["todo", "done", "3"])]
    #[case::queue_remove(&["queue", "remove", "3"])]
    fn is_no_flag_of_the_commands_that_would_ignore_it(#[case] args: &[&str]) {
        assert!(parsed(&[args, &["--boot", "1-2"]].concat()).is_err());
    }

    #[rstest]
    #[case::start(&["start", "--machine", "vps"])]
    #[case::new_workspace(&["new-workspace", "x", "--machine", "vps"])]
    #[case::focus(&["focus", "--machine", "vps", "--workspace", "4"])]
    #[case::flow_run(&["flow", "run", "Pair", "--prompt", "x", "--machine", "vps"])]
    fn is_the_boot_of_that_machine_with_the_ids_of_another_machine(#[case] args: &[&str]) {
        let with = [args, &["--boot", "1-2"]].concat();
        let cli = parsed(&with).expect("parse");
        let Some(Command::Control(control)) = cli.command else { panic!("not a command for scripts") };

        assert_eq!((control.boot(), control.machine()), (Some("1-2"), Some("vps")));
    }

    #[rstest]
    #[case::start(&["start"])]
    #[case::new_workspace(&["new-workspace", "x"])]
    #[case::focus(&["focus", "--pane", "4"])]
    #[case::flow_run(&["flow", "run", "Pair", "--prompt", "x"])]
    fn is_this_servers_without_a_machine(#[case] args: &[&str]) {
        let with = [args, &["--boot", "1-2"]].concat();
        let cli = parsed(&with).expect("parse");
        let Some(Command::Control(control)) = cli.command else { panic!("not a command for scripts") };

        assert_eq!((control.boot(), control.machine()), (Some("1-2"), None));
    }

    #[test]
    fn is_no_flag_of_a_machine_s_focus_with_a_view() {
        assert!(parsed(&["focus", "--machine", "vps", "--tab", "1", "--view", "41", "--boot", "1-2"]).is_err());
    }

    #[rstest]
    #[case::before_a_stop(&["processes", "--boot", "1-2", "stop", "5"])]
    #[case::before_a_port_stop(&["ports", "--boot", "1-2", "stop", "8080"])]
    #[case::with_the_list_of_ignored_files(&["clone", "--list-ignored", "--boot", "1-2"])]
    fn is_refused_where_it_would_mean_nothing(#[case] args: &[&str]) {
        assert!(parsed(args).is_err());
    }

    #[test]
    fn goes_with_plugin_run() {
        let boot_of_run = |args: &[&str]| {
            let cli = parsed(args).expect("parse");
            let Some(Command::Plugin(plugin)) = cli.command else { panic!("not a plugin command") };
            let PluginAction::Run(run) = plugin.action else { panic!("not plugin run") };
            run.boot.boot
        };
        assert_eq!(boot_of_run(&["plugin", "run", "p", "a", "--pane", "4", "--boot", "1-2"]).as_deref(), Some("1-2"));
        assert_eq!(boot_of_run(&["plugin", "run", "p", "a", "--pane", "4"]), None);
    }
}
