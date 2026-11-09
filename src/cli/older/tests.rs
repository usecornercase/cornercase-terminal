use rstest::rstest;
use serde_json::json;

use super::*;
use crate::error::Behind;

fn remote() -> Remote {
    Remote {
        destination: "ana@vps.example.com".into(),
        command: Some("~/bin/cornercase".into()),
        option: "--remote-command",
    }
}

mod the_refusal {
    use super::*;

    #[test]
    fn gives_both_versions_and_the_update_words() {
        let remote = remote();
        let behind = Behind { host: remote.destination.clone(), there: "0.0.1".into(), update: remote.update_line() };
        let error = Error::RemoteBehind(Box::new(behind));
        let value = serde_json::to_value(refusal(&error, &remote).expect("a refusal")).expect("json");
        assert_eq!(
            value,
            json!({"error": {
                "code": "older",
                "message": error.to_string(),
                "destination": "ana@vps.example.com",
                "there": "0.0.1",
                "here": version::CURRENT,
                "update": ["cornercase", "--remote", "ana@vps.example.com", "--remote-command", "~/bin/cornercase", "update"],
            }})
        );
    }

    #[test]
    fn has_a_null_release_from_before_the_proxy() {
        let remote = remote();
        let error = Error::RemoteTooOld("vps".into());
        let value = serde_json::to_value(refusal(&error, &remote).expect("a refusal")).expect("json");
        assert_eq!(value["error"]["there"], json!(null));
        assert!(value["error"].as_object().expect("object").contains_key("there"));
    }

    #[rstest]
    #[case::ssh(Error::Ssh { host: "vps".into(), reason: "x".into() })]
    #[case::missing(Error::RemoteMissing { host: "vps".into(), option: "--remote-command" })]
    #[case::no_server(Error::RemoteNoServer("vps".into()))]
    #[case::window(Error::RemoteOlder { host: "vps".into(), there: "0.0.1".into() })]
    #[case::stale(Error::StaleBoot("x".into()))]
    fn prints_nothing_for(#[case] error: Error) {
        assert!(refusal(&error, &remote()).is_none());
    }
}

mod which_refusals_exit_with_5 {
    use super::*;

    fn behind() -> Error {
        Error::RemoteBehind(Box::new(Behind { host: "vps".into(), there: "0.0.1".into(), update: "u".into() }))
    }

    fn older_window() -> Error {
        Error::RemoteOlder { host: "vps".into(), there: "0.0.1".into() }
    }

    #[rstest]
    #[case::behind(behind(), true)]
    #[case::too_old(Error::RemoteTooOld("vps".into()), true)]
    #[case::window(older_window(), true)]
    #[case::newer(Error::RemoteNewer { host: "vps".into(), there: "9.0.0".into() }, false)]
    #[case::ssh(Error::Ssh { host: "vps".into(), reason: "x".into() }, false)]
    #[case::missing(Error::RemoteMissing { host: "vps".into(), option: "--remote-command" }, false)]
    #[case::no_server(Error::RemoteNoServer("vps".into()), false)]
    #[case::stale(Error::StaleBoot("x".into()), false)]
    #[case::elsewhere(Error::Elsewhere(Box::new(older_window())), false)]
    fn is_decided_by(#[case] error: Error, #[case] expected: bool) {
        assert_eq!(older(&error), expected);
    }
}

mod asking_for_json {
    use super::*;

    #[rstest]
    #[case::status(&["cornercase", "status", "--json"], true)]
    #[case::changes(&["cornercase", "changes", "list", "--json"], true)]
    #[case::events(&["cornercase", "events", "--json"], true)]
    #[case::machines(&["cornercase", "machines", "--json"], true)]
    #[case::remote(&["cornercase", "--remote", "vps", "status", "--json"], true)]
    #[case::config(&["cornercase", "config", "--json"], true)]
    #[case::plain(&["cornercase", "status"], false)]
    #[case::view(&["cornercase", "view", "--cols", "80", "--rows", "24"], false)]
    #[case::requests(&["cornercase", "requests", "--follow", "-"], false)]
    fn is_found(#[case] args: &[&str], #[case] expected: bool) {
        let matches = Cli::command().try_get_matches_from(args).expect("parse");
        assert_eq!(wants_json(&matches), expected);
    }

    fn flags_named_json(command: &clap::Command, found: &mut Vec<String>) {
        for arg in command.get_arguments().filter(|arg| arg.get_long() == Some("json")) {
            found.push(arg.get_id().to_string());
        }
        for sub in command.get_subcommands() {
            flags_named_json(sub, found);
        }
    }

    #[test]
    fn every_json_flag_is_found() {
        let mut found = Vec::new();
        flags_named_json(&Cli::command(), &mut found);
        assert!(found.len() > 10, "{found:?}");
        assert!(found.iter().all(|id| id == "json"), "{found:?}");
    }
}
