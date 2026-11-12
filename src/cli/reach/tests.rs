use std::ffi::OsString;

use clap::Parser;
use rstest::rstest;

use super::*;

fn parsed(args: &[&str]) -> Cli {
    Cli::try_parse_from(std::iter::once("cornercase").chain(args.iter().copied())).expect("parse")
}

fn nothing(_: &str) -> Option<OsString> {
    None
}

fn set(value: &'static str) -> impl Fn(&str) -> Option<OsString> {
    move |name| (name == REMOTE_ENV).then(|| OsString::from(value))
}

fn devbox() -> Remote {
    Remote { destination: "devbox".into(), command: None, option: "--command" }
}

mod parsing {
    use super::*;

    #[rstest]
    #[case::before(&["--remote", "devbox", "status"])]
    #[case::after(&["status", "--remote", "devbox"])]
    fn remote_goes_before_or_after_the_command(#[case] args: &[&str]) {
        assert_eq!(parsed(args).remote.as_deref(), Some("devbox"));
    }

    #[test]
    fn an_empty_destination_is_wrong_usage() {
        assert!(Cli::try_parse_from(["cornercase", "--remote", "", "status"]).is_err());
    }
}

mod reaching {
    use super::*;

    #[rstest]
    #[case::status(&["--remote", "devbox", "status"])]
    #[case::start_server(&["--remote", "devbox", "start-server"])]
    #[case::stats(&["--remote", "devbox", "stats"])]
    #[case::plugin_status(&["--remote", "devbox", "plugin", "status"])]
    #[case::machines(&["--remote", "devbox", "machines", "retry", "vps"])]
    #[case::the_window(&["--remote", "devbox"])]
    fn goes_there(#[case] args: &[&str]) {
        let chosen = chosen(&parsed(args), &nothing).expect("allowed");

        assert_eq!(chosen.map(|remote| remote.destination), Some("devbox".to_string()));
    }

    #[test]
    fn the_variable_does_what_the_flag_does() {
        let chosen = chosen(&parsed(&["status"]), &set("devbox")).expect("allowed");

        assert_eq!(chosen, Some(Remote { option: "--remote-command", ..devbox() }));
    }

    #[test]
    fn the_flag_wins_over_the_variable() {
        let chosen = chosen(&parsed(&["--remote", "other", "status"]), &set("devbox")).expect("allowed");

        assert_eq!(chosen.map(|remote| remote.destination), Some("other".to_string()));
    }

    #[test]
    fn an_empty_variable_is_unset() {
        assert_eq!(chosen(&parsed(&["status"]), &set("")).expect("allowed"), None);
    }

    #[test]
    fn nothing_given_stays_here() {
        assert_eq!(chosen(&parsed(&["status"]), &nothing).expect("allowed"), None);
    }

    #[rstest]
    #[case::kill_server(&["kill-server"])]
    #[case::restart(&["restart"])]
    #[case::update(&["update"])]
    #[case::logs(&["logs"])]
    #[case::plugin_link(&["plugin", "link", "/srv/my-plugin"])]
    #[case::plugin_install(&["plugin", "install", "acme/notes"])]
    #[case::plugin_update(&["plugin", "update"])]
    #[case::plugin_remove(&["plugin", "remove", "acme.notes"])]
    #[case::plugin_unlink(&["plugin", "unlink", "acme.notes"])]
    #[case::plugin_enable(&["plugin", "enable", "acme.notes"])]
    #[case::plugin_disable(&["plugin", "disable", "acme.notes"])]
    #[case::plugin_list(&["plugin", "list", "--json"])]
    #[case::plugin_logs(&["plugin", "logs", "acme.notes", "-n", "60"])]
    fn the_commands_that_run_as_processes_go_there(#[case] args: &[&str]) {
        let mut all = vec!["--remote", "devbox"];
        all.extend(args);

        let flag = chosen(&parsed(&all), &nothing).expect("allowed");
        let variable = chosen(&parsed(args), &set("devbox")).expect("allowed");

        assert_eq!(flag.map(|remote| remote.destination), Some("devbox".to_string()));
        assert_eq!(variable.map(|remote| remote.destination), Some("devbox".to_string()));
    }

    #[test]
    fn keeping_window_processes_refuses_it_and_says_why() {
        let error = chosen(&parsed(&["--remote", "devbox", "plugin", "windows"]), &nothing).expect_err("refused");

        assert!(matches!(&error, Error::WrongUsage(_)), "{error:?}");
        assert_eq!(
            error.to_string(),
            "`plugin windows` runs on the machine that shows the window, so it takes no --remote: run it there"
        );
    }

    #[test]
    fn keeping_window_processes_names_the_variable_when_it_is_the_cause() {
        let error = chosen(&parsed(&["plugin", "windows"]), &set("devbox")).expect_err("refused");

        assert!(error.to_string().contains("CORNERCASE_REMOTE=devbox sets: unset it, or run it there"), "{error}");
    }

    #[rstest]
    #[case::remote(&["remote", "otherbox"])]
    #[case::the_known_machines(&["remote", "--known"])]
    #[case::remembering_a_machine(&["remote", "--remember", "devbox"])]
    #[case::forgetting_a_machine(&["remote", "--forget", "devbox"])]
    #[case::proxy(&["proxy"])]
    #[case::server(&["server"])]
    #[case::skill(&["skill"])]
    fn ignore_the_variable(#[case] args: &[&str]) {
        assert_eq!(chosen(&parsed(args), &set("devbox")).expect("allowed"), None);
    }

    #[rstest]
    #[case::a_window(&["--remote", "x", "remote", "y"])]
    #[case::the_known_machines(&["--remote", "x", "remote", "--known"])]
    #[case::remembering_a_machine(&["--remote", "x", "remote", "--remember", "y"])]
    fn the_window_command_refuses_the_flag(#[case] args: &[&str]) {
        let error = chosen(&parsed(args), &nothing).expect_err("refused");

        assert!(matches!(error, Error::WrongUsage(_)), "{error:?}");
        assert!(error.to_string().contains("`remote` takes no --remote"), "{error}");
    }

    #[test]
    fn remote_command_needs_a_destination() {
        let error = chosen(&parsed(&["--remote-command", "cc", "status"]), &nothing).expect_err("refused");

        assert!(error.to_string().contains("--remote-command needs"), "{error}");
    }

    #[test]
    fn remote_command_goes_with_the_variable() {
        let chosen = chosen(&parsed(&["--remote-command", "~/bin/cc", "status"]), &set("devbox")).expect("allowed");

        assert_eq!(chosen.and_then(|remote| remote.command), Some("~/bin/cc".to_string()));
    }
}

mod forwarding {
    use super::*;

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    fn there_of(args: &[&str]) -> Vec<String> {
        there(&parsed(args).command.expect("a command"), &devbox()).expect("goes there").expect("its words")
    }

    #[rstest]
    #[case::kill_server(&["kill-server", "--yes"])]
    #[case::restart(&["restart", "--yes", "--when-idle", "--timeout", "1.5"])]
    #[case::restart_now(&["restart"])]
    #[case::update_check(&["update", "--check"])]
    #[case::update_json(&["update", "--json"])]
    #[case::update_check_json(&["update", "--check", "--json"])]
    #[case::update_when_idle(&["update", "--yes", "--when-idle"])]
    #[case::logs_follow(&["logs", "-n", "50", "--follow"])]
    #[case::logs_path(&["logs", "--path"])]
    #[case::logs_default(&["logs"])]
    #[case::plugin_link_from_the_root(&["plugin", "link", "/srv/my-plugin"])]
    #[case::plugin_link_in_the_home_folder(&["plugin", "link", "~/code/my-plugin"])]
    #[case::plugin_install(&["plugin", "install", "acme/notes"])]
    #[case::plugin_install_preview(&["plugin", "install", "acme/notes@v1", "--preview", "--json"])]
    #[case::plugin_install_a_folder(&["plugin", "install", "/srv/tools.git//plugins/notes@main"])]
    #[case::plugin_install_the_commit(&[
        "plugin", "install", "~/code/tools//plugins/notes", "--commit", COMMIT, "--yes",
    ])]
    #[case::plugin_update_all(&["plugin", "update", "--yes"])]
    #[case::plugin_update_preview(&["plugin", "update", "acme.notes", "--preview", "--json"])]
    #[case::plugin_update_the_commit(&["plugin", "update", "acme.notes", "--commit", COMMIT, "--yes"])]
    #[case::plugin_remove(&["plugin", "remove", "acme.notes"])]
    #[case::plugin_unlink(&["plugin", "unlink", "acme.notes"])]
    #[case::plugin_enable(&["plugin", "enable", "acme.notes"])]
    #[case::plugin_disable_an_odd_id(&["plugin", "disable", "--", "-odd"])]
    #[case::plugin_list(&["plugin", "list"])]
    #[case::plugin_list_json(&["plugin", "list", "--json"])]
    #[case::plugin_logs_follow(&["plugin", "logs", "acme.notes", "-n", "60", "--follow"])]
    #[case::plugin_logs_path(&["plugin", "logs", "acme.notes", "--path"])]
    fn what_goes_there_parses_to_the_same_command(#[case] args: &[&str]) {
        let sent = there_of(args);

        let again = parsed(&sent.iter().map(String::as_str).collect::<Vec<_>>());

        assert_eq!(format!("{:?}", again.command), format!("{:?}", parsed(args).command), "{sent:?}");
    }

    #[test]
    fn the_json_flag_goes_there() {
        assert_eq!(there_of(&["update", "--check", "--json"]), ["update", "--check", "--json"]);
    }

    #[test]
    fn a_whole_number_of_seconds_has_no_decimals() {
        assert_eq!(
            there_of(&["update", "--when-idle", "--timeout", "3600"]),
            ["update", "--when-idle", "--timeout", "3600"]
        );
    }

    #[test]
    fn the_id_goes_last_after_a_double_dash() {
        assert_eq!(
            there_of(&["plugin", "logs", "acme.notes", "-n", "60"]),
            ["plugin", "logs", "--lines", "60", "--", "acme.notes"]
        );
    }

    #[rstest]
    #[case::status(&["status"])]
    #[case::plugin_status(&["plugin", "status"])]
    #[case::plugin_actions(&["plugin", "actions"])]
    #[case::plugin_run(&["plugin", "run", "acme.notes", "open", "--pane", "5"])]
    #[case::plugin_popup_close(&["plugin", "popup-close"])]
    #[case::plugin_windows(&["plugin", "windows"])]
    #[case::start_server(&["start-server"])]
    fn the_others_do_not_run_as_processes(#[case] args: &[&str]) {
        assert!(there(&parsed(args).command.expect("a command"), &devbox()).is_none());
    }

    #[rstest]
    #[case::link(&["plugin", "link", "/x"])]
    #[case::install(&["plugin", "install", "acme/notes"])]
    #[case::update(&["plugin", "update"])]
    #[case::remove(&["plugin", "remove", "a"])]
    #[case::unlink(&["plugin", "unlink", "a"])]
    #[case::list(&["plugin", "list"])]
    #[case::status(&["plugin", "status"])]
    #[case::actions(&["plugin", "actions"])]
    #[case::run(&["plugin", "run", "a", "b"])]
    #[case::popup_close(&["plugin", "popup-close"])]
    #[case::windows(&["plugin", "windows"])]
    #[case::enable(&["plugin", "enable", "a"])]
    #[case::disable(&["plugin", "disable", "a"])]
    #[case::logs(&["plugin", "logs", "a"])]
    fn a_plugin_command_runs_there_exactly_when_its_kind_says_so(#[case] args: &[&str]) {
        let command = parsed(args).command.expect("a command");

        assert_eq!(kind(Some(&command)) == Kind::Run, there(&command, &devbox()).is_some());
    }
}

mod plugin_folders {
    use super::*;

    #[rstest]
    #[case::link_a_name(&["plugin", "link", "my-plugin"])]
    #[case::link_from_here(&["plugin", "link", "./my-plugin"])]
    #[case::install_from_here(&["plugin", "install", "./tools//plugins/notes"])]
    #[case::install_from_above(&["plugin", "install", "../tools"])]
    fn a_relative_folder_is_refused_before_anything_runs(#[case] args: &[&str]) {
        let command = parsed(args).command.expect("a command");

        let refused = there(&command, &devbox()).expect("goes there").expect_err("refused");

        assert!(matches!(&refused, Error::WrongUsage(_)), "{refused:?}");
        assert!(refused.to_string().contains("absolute path on devbox"), "{refused}");
    }
}

#[cfg(unix)]
mod outcomes {
    use std::os::unix::process::ExitStatusExt;

    use super::*;

    fn status(code: i32) -> ExitStatus {
        ExitStatus::from_raw(code << 8)
    }

    #[rstest]
    #[case::done(0, true)]
    #[case::failed(1, false)]
    fn the_status_there_is_the_status_here(#[case] code: i32, #[case] done: bool) {
        assert_eq!(outcome(&devbox(), status(code)).expect("passes through"), done);
    }

    #[test]
    fn a_partial_answer_stays_partial() {
        assert!(matches!(outcome(&devbox(), status(3)), Err(Error::Partial)));
    }

    #[rstest]
    #[case::usage(2, 2)]
    #[case::ssh(255, 255)]
    #[case::interrupted(130, 130)]
    #[case::update_by_hand(6, 6)]
    fn another_code_is_passed_on_without_a_message(#[case] code: i32, #[case] passed: u8) {
        assert!(matches!(outcome(&devbox(), status(code)), Err(Error::Exited(c)) if c == passed));
    }

    #[rstest]
    #[case::sh(127)]
    #[case::bash(126)]
    fn a_missing_command_names_the_option(#[case] code: i32) {
        let error = outcome(&devbox(), status(code)).expect_err("missing");

        assert!(error.to_string().ends_with("with --command"), "{error}");
    }
}

mod paths {
    use super::*;

    fn there(path: &str, windows: bool) -> Result<PathBuf> {
        path_on(Path::new(path), Some(&devbox()), windows, &|| false)
    }

    #[rstest]
    #[case::a_name("src", false)]
    #[case::a_name_from_windows("src", true)]
    #[case::a_drive_from_unix(r"C:\srv", false)]
    fn a_relative_path_is_refused_there(#[case] path: &str, #[case] windows: bool) {
        let error = there(path, windows).expect_err("refused");

        assert!(matches!(&error, Error::WrongUsage(_)), "{error:?}");
        assert!(error.to_string().contains("absolute path on devbox"), "{error}");
    }

    #[rstest]
    #[case::with_dots("/a/../b", false, "/a/../b")]
    #[case::from_windows("/srv/x", true, "/srv/x")]
    #[case::with_backslashes_from_windows(r"/srv\shop\x", true, "/srv/shop/x")]
    #[case::a_backslash_from_unix(r"/srv\x", false, r"/srv\x")]
    fn a_path_from_the_root_goes_with_the_remote_s_slashes(
        #[case] path: &str,
        #[case] windows: bool,
        #[case] sent: &str,
    ) {
        assert_eq!(there(path, windows).expect("kept").to_str(), Some(sent));
    }

    #[rstest]
    #[case::a_drive(r"C:\srv\x")]
    #[case::turned_by_git_bash("C:/Program Files/Git/srv/x")]
    #[case::the_root_of_a_drive(r"\srv\x")]
    #[case::a_share(r"\\host\share\x")]
    #[case::relative_to_a_drive("c:x")]
    fn a_windows_path_is_refused_with_how_to_give_a_unix_one(#[case] path: &str) {
        let error = there(path, true).expect_err("refused");

        assert!(matches!(&error, Error::WrongUsage(_)), "{error:?}");
        assert!(error.to_string().contains("is a Windows path"), "{error}");
        assert!(error.to_string().contains("MSYS_NO_PATHCONV=1"), "{error}");
    }

    #[rstest]
    #[case::the_home_folder("~")]
    #[case::inside_it("~/src/shop")]
    fn a_path_in_the_home_folder_goes_as_it_is_here_and_there(#[case] path: &str) {
        assert_eq!(open_path(Path::new(path)).expect("kept"), PathBuf::from(path));
    }

    #[test]
    fn a_path_that_only_looks_like_a_home_folder_is_a_relative_one() {
        assert!(open_path(Path::new("~user/x")).expect("absolute").is_absolute());
        assert!(open_path(Path::new("./~/x")).expect("absolute").is_absolute());
    }

    fn on_windows(path: &str, windows: bool) -> Result<PathBuf> {
        path_on(Path::new(path), Some(&devbox()), windows, &|| true)
    }

    #[rstest]
    #[case::a_drive(r"C:\srv\x", r"C:\srv\x")]
    #[case::a_drive_with_slashes("C:/srv/x", r"C:\srv\x")]
    #[case::a_share(r"\\host\share\x", r"\\host\share\x")]
    #[case::the_root_of_a_share(r"\\host\share", r"\\host\share")]
    fn a_windows_machine_takes_its_own_absolute_paths(#[case] path: &str, #[case] sent: &str) {
        for windows in [false, true] {
            assert_eq!(on_windows(path, windows).expect("kept").to_str(), Some(sent));
        }
    }

    #[rstest]
    #[case::the_root_of_a_drive(r"\srv\x")]
    #[case::relative_to_a_drive("c:x")]
    #[case::a_share_without_its_name(r"\\host")]
    #[case::a_share_with_an_empty_name(r"\\host\\x")]
    fn a_windows_machine_refuses_a_path_relative_to_something_here(#[case] path: &str) {
        let error = on_windows(path, true).expect_err("refused");

        assert!(matches!(&error, Error::WrongUsage(_)), "{error:?}");
        assert!(error.to_string().contains("devbox is a Windows machine"), "{error}");
    }

    #[test]
    fn a_unix_path_goes_without_asking_what_the_machine_runs() {
        let asked = std::cell::Cell::new(false);
        let ask = || {
            asked.set(true);
            true
        };

        let sent = path_on(Path::new("/srv/x"), Some(&devbox()), false, &ask);

        assert_eq!(sent.expect("kept"), PathBuf::from("/srv/x"));
        assert!(!asked.get());
    }

    #[test]
    fn here_it_becomes_absolute() {
        assert!(path_on(Path::new("src"), None, cfg!(windows), &|| false).expect("absolute").is_absolute());
    }
}
