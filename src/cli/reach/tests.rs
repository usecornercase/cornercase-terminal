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
    fn the_lifecycle_commands_go_there(#[case] args: &[&str]) {
        let mut all = vec!["--remote", "devbox"];
        all.extend(args);

        let flag = chosen(&parsed(&all), &nothing).expect("allowed");
        let variable = chosen(&parsed(args), &set("devbox")).expect("allowed");

        assert_eq!(flag.map(|remote| remote.destination), Some("devbox".to_string()));
        assert_eq!(variable.map(|remote| remote.destination), Some("devbox".to_string()));
    }

    #[test]
    fn the_commands_that_change_plugins_refuse_it() {
        let error = chosen(&parsed(&["--remote", "devbox", "plugin", "install", "x"]), &nothing).expect_err("refused");

        assert!(matches!(&error, Error::WrongUsage(_)), "{error:?}");
        let text = error.to_string();
        assert!(text.contains("plugin") && text.contains("ssh"), "{text}");
    }

    #[test]
    fn the_variable_is_named_when_it_is_the_cause() {
        let error = chosen(&parsed(&["plugin", "install", "x"]), &set("devbox")).expect_err("refused");

        assert!(error.to_string().contains("CORNERCASE_REMOTE=devbox"), "{error}");
    }

    #[rstest]
    #[case::remote(&["remote", "otherbox"])]
    #[case::proxy(&["proxy"])]
    #[case::server(&["server"])]
    #[case::skill(&["skill"])]
    fn ignore_the_variable(#[case] args: &[&str]) {
        assert_eq!(chosen(&parsed(args), &set("devbox")).expect("allowed"), None);
    }

    #[test]
    fn the_window_command_refuses_the_flag() {
        let error = chosen(&parsed(&["--remote", "x", "remote", "y"]), &nothing).expect_err("refused");

        assert!(matches!(error, Error::WrongUsage(_)), "{error:?}");
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

    fn there_of(args: &[&str]) -> Vec<String> {
        there(&parsed(args).command.expect("a command")).expect("goes there")
    }

    #[rstest]
    #[case::kill_server(&["kill-server", "--yes"])]
    #[case::restart(&["restart", "--yes", "--when-idle", "--timeout", "1.5"])]
    #[case::restart_now(&["restart"])]
    #[case::update_check(&["update", "--check"])]
    #[case::update_when_idle(&["update", "--yes", "--when-idle"])]
    #[case::logs_follow(&["logs", "-n", "50", "--follow"])]
    #[case::logs_path(&["logs", "--path"])]
    #[case::logs_default(&["logs"])]
    fn what_goes_there_parses_to_the_same_command(#[case] args: &[&str]) {
        let sent = there_of(args);

        let again = parsed(&sent.iter().map(String::as_str).collect::<Vec<_>>());

        assert_eq!(format!("{:?}", again.command), format!("{:?}", parsed(args).command), "{sent:?}");
    }

    #[test]
    fn a_whole_number_of_seconds_has_no_decimals() {
        assert_eq!(
            there_of(&["update", "--when-idle", "--timeout", "3600"]),
            ["update", "--when-idle", "--timeout", "3600"]
        );
    }

    #[rstest]
    #[case::status(&["status"])]
    #[case::plugin_install(&["plugin", "install", "x"])]
    #[case::start_server(&["start-server"])]
    fn the_others_do_not_run_as_processes(#[case] args: &[&str]) {
        assert_eq!(there(&parsed(args).command.expect("a command")), None);
    }
}

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

    #[test]
    fn a_relative_path_is_refused_there() {
        let error = path_on(Path::new("src"), Some(&devbox())).expect_err("refused");

        assert!(matches!(&error, Error::WrongUsage(_)), "{error:?}");
        assert!(error.to_string().contains("absolute path on devbox"), "{error}");
    }

    #[test]
    fn an_absolute_one_goes_as_it_is() {
        assert_eq!(path_on(Path::new("/a/../b"), Some(&devbox())).expect("kept"), PathBuf::from("/a/../b"));
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

    #[test]
    fn here_it_becomes_absolute() {
        assert!(path_on(Path::new("src"), None).expect("absolute").is_absolute());
    }
}
