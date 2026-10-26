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
    #[case::kill_server(&["kill-server"], "kill-server")]
    #[case::restart(&["restart"], "restart")]
    #[case::update(&["update"], "update")]
    #[case::logs(&["logs"], "logs")]
    #[case::plugin_install(&["plugin", "install", "x"], "plugin")]
    fn commands_of_this_machine_refuse_it(#[case] args: &[&str], #[case] name: &str) {
        let mut all = vec!["--remote", "devbox"];
        all.extend(args);

        let error = chosen(&parsed(&all), &nothing).expect_err("refused");

        assert!(matches!(&error, Error::WrongUsage(_)), "{error:?}");
        let text = error.to_string();
        assert!(text.contains(name) && text.contains("ssh"), "{text}");
    }

    #[test]
    fn the_variable_is_named_when_it_is_the_cause() {
        let error = chosen(&parsed(&["kill-server"]), &set("devbox")).expect_err("refused");

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
