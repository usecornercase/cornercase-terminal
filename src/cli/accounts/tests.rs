use std::collections::BTreeMap;
use std::path::PathBuf;

use super::*;
use crate::cli::args::{Command as Parsed, Control};
use crate::cli::tests::parse;
use crate::control::accounts::{AccountInfo, ProfileInfo, ProviderInfo};

fn control(args: &[&str]) -> Control {
    match parse(args).expect("parse").command {
        Some(Parsed::Control(control)) => control,
        other => panic!("not a command for scripts: {other:?}"),
    }
}

mod parsing {
    use super::*;

    #[test]
    fn start_new_tab_and_flow_run_take_a_profile() {
        let Control::Start(start) = control(&["start", "claude", "--profile", "Work"]) else { panic!("start") };
        let Control::NewTab(new) = control(&["new-tab", "--profile", ""]) else { panic!("new-tab") };
        let Control::Flow(flow) = control(&["flow", "run", "Quick", "--prompt", "x", "--profile", "Home"]) else {
            panic!("flow")
        };
        let crate::cli::args::FlowAction::Run(run) = flow.action else { panic!("flow run") };

        assert_eq!(
            (start.profile.profile.as_deref(), new.profile.profile.as_deref(), run.profile.profile.as_deref()),
            (Some("Work"), Some(""), Some("Home"))
        );
    }

    #[test]
    fn without_a_profile_none_is_sent() {
        let Control::Start(start) = control(&["start", "claude"]) else { panic!("start") };

        assert_eq!(profile_arg(start.profile).expect("no check needed"), None);
    }

    #[test]
    fn a_profile_takes_pairs_of_agent_and_account() {
        let Control::Profile(args) = control(&["profile", "new", "Work", "claude=work", "codex=me"]) else {
            panic!("profile")
        };
        let ProfileAction::New { name, accounts } = args.action else { panic!("new") };

        assert_eq!(name, "Work");
        assert_eq!(accounts, [("claude".to_string(), "work".to_string()), ("codex".to_string(), "me".to_string())]);
    }

    #[rstest::rstest]
    #[case::a_pair_without_an_account(&["profile", "new", "Work", "claude="])]
    #[case::a_pair_without_an_equals(&["profile", "new", "Work", "claude"])]
    #[case::use_with_neither_a_name_nor_system(&["profile", "use"])]
    #[case::use_with_both(&["profile", "use", "Work", "--system"])]
    #[case::set_with_nothing_to_change(&["profile", "set", "Work"])]
    #[case::set_an_account_without_its_agent(&["profile", "set", "Work", "--account", "me", "--rename", "W"])]
    #[case::a_login_workspace_without_login(&["account", "add", "claude", "work", "--workspace", "3"])]
    fn wrong_usage_is_refused(#[case] args: &[&str]) {
        assert!(parse(args).is_err(), "{args:?} parsed");
    }

    #[test]
    fn use_system_sends_the_empty_name() {
        let Control::Profile(args) = control(&["profile", "use", "--system"]) else { panic!("profile") };

        assert!(matches!(args.action, ProfileAction::Use { name: None, system: true }));
    }
}

mod text {
    use super::*;

    fn info() -> AccountsInfo {
        let account = |name: &str, unshared: &[&str]| AccountInfo {
            name: name.into(),
            path: PathBuf::from(format!("/state/accounts/claude/{name}")),
            unshared: unshared.iter().map(ToString::to_string).collect(),
        };
        AccountsInfo {
            providers: vec![
                ProviderInfo {
                    provider: "claude".into(),
                    title: "Claude Code".into(),
                    env: "CLAUDE_CONFIG_DIR".into(),
                    accounts: vec![account("home", &["settings.json"]), account("work", &[])],
                },
                ProviderInfo {
                    provider: "codex".into(),
                    title: "Codex".into(),
                    env: "CODEX_HOME".into(),
                    accounts: Vec::new(),
                },
            ],
            profiles: vec![
                ProfileInfo {
                    name: "Work".into(),
                    accounts: BTreeMap::from([("claude".into(), "work".into())]),
                    active: true,
                },
                ProfileInfo { name: "Empty".into(), accounts: BTreeMap::new(), active: false },
            ],
            profile: "Work".into(),
            login: None,
        }
    }

    #[test]
    fn accounts_are_listed_per_agent_with_what_stopped_being_shared() {
        assert_eq!(
            accounts_text(&info()),
            "Claude Code  CLAUDE_CONFIG_DIR\n  home  /state/accounts/claude/home  (not shared: settings.json)\n  \
             work  /state/accounts/claude/work\nCodex  CODEX_HOME\n  no accounts\n"
        );
    }

    #[test]
    fn profiles_mark_the_active_one_and_end_with_the_system() {
        assert_eq!(
            profiles_text(&info()),
            "* Work  claude=work\n  Empty  the system's logins\n  system  each agent's own login\n"
        );
    }

    #[test]
    fn the_system_is_marked_when_no_profile_is_active() {
        let info = AccountsInfo { profile: String::new(), profiles: Vec::new(), ..info() };

        assert_eq!(profiles_text(&info), "* system  each agent's own login\n");
    }
}
