use std::collections::BTreeMap;
use std::path::PathBuf;

use super::*;
use crate::accounts::login::State;
use crate::cli::args::{Command as Parsed, Control};
use crate::cli::tests::parse;
use crate::control::accounts::{AccountInfo, LoginInfo, ProfileInfo, ProviderInfo};

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
    fn account_list_takes_check() {
        let Control::Account(args) = control(&["account", "list", "--check", "--json"]) else { panic!("account") };
        let AccountAction::List(list) = args.action else { panic!("list") };

        assert!(list.check && list.print.json);
    }

    #[test]
    fn without_a_profile_none_is_sent() {
        let Control::Start(start) = control(&["start", "claude"]) else { panic!("start") };

        assert_eq!(profile_arg(start.profile).expect("no check needed"), (None, BTreeMap::new()));
    }

    #[test]
    fn account_takes_an_agent_and_its_account_once_per_agent() {
        let Control::Start(start) = control(&["start", "claude", "--account", "claude=home", "--account", "codex="])
        else {
            panic!("start")
        };
        let Control::NewTab(twice) = control(&["new-tab", "--account", "claude=a", "--account", "claude=b"]) else {
            panic!("new-tab")
        };

        assert_eq!(
            start.profile.accounts,
            [("claude".to_string(), "home".to_string()), ("codex".to_string(), String::new())]
        );
        assert!(matches!(profile_arg(twice.profile), Err(Error::WrongUsage(_))));
        assert!(parse(&["start", "--account", "=home"]).is_err());
        assert!(parse(&["start", "--account", "claude"]).is_err());
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

    fn rename(args: &[&str]) -> Result<(String, String)> {
        let Control::Account(parsed) = control(args) else { panic!("account") };
        let AccountAction::Rename(rename) = parsed.action else { panic!("rename") };
        rename_target(&rename)
    }

    #[rstest::rstest]
    #[case::an_account(&["account", "rename", "claude", "home", "Personal"], ("home", "Personal"))]
    #[case::the_system(&["account", "rename", "claude", "--system", "work"], ("", "work"))]
    #[case::clearing_an_account(&["account", "rename", "claude", "home", "--clear"], ("home", ""))]
    #[case::clearing_the_system(&["account", "rename", "claude", "--system", "--clear"], ("", ""))]
    fn rename_names_an_account_or_the_system(#[case] args: &[&str], #[case] wanted: (&str, &str)) {
        let (account, label) = rename(args).expect("a target");

        assert_eq!((account.as_str(), label.as_str()), wanted);
    }

    #[rstest::rstest]
    #[case::only_the_account(&["account", "rename", "claude", "home"])]
    #[case::the_system_and_an_account(&["account", "rename", "claude", "home", "--system", "x"])]
    #[case::clearing_with_a_name(&["account", "rename", "claude", "home", "x", "--clear"])]
    #[case::nothing(&["account", "rename", "claude"])]
    fn a_rename_without_a_clear_target_is_wrong_usage(#[case] args: &[&str]) {
        assert!(matches!(rename(args), Err(Error::WrongUsage(_))), "{args:?}");
    }

    #[test]
    fn use_takes_an_account_or_the_system() {
        let Control::Account(args) = control(&["account", "use", "claude", "--system"]) else { panic!("account") };
        let AccountAction::Use(chosen) = args.action else { panic!("use") };
        assert!(chosen.system && chosen.account.is_none());
        let Control::Account(args) = control(&["account", "use", "claude", "home", "--json"]) else {
            panic!("account")
        };
        let AccountAction::Use(chosen) = args.action else { panic!("use") };
        assert_eq!(chosen.account.as_deref(), Some("home"));
        assert!(parse(&["account", "use", "claude"]).is_err());
        assert!(parse(&["account", "use", "claude", "home", "--system"]).is_err());
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
            login: None,
            ..AccountInfo::default()
        };
        AccountsInfo {
            providers: vec![
                ProviderInfo {
                    provider: "claude".into(),
                    title: "Claude Code".into(),
                    env: "CLAUDE_CONFIG_DIR".into(),
                    accounts: vec![account("home", &["settings.json"]), account("work", &[])],
                    ..ProviderInfo::default()
                },
                ProviderInfo {
                    provider: "codex".into(),
                    title: "Codex".into(),
                    env: "CODEX_HOME".into(),
                    accounts: Vec::new(),
                    ..ProviderInfo::default()
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
            tokens: Vec::new(),
        }
    }

    #[test]
    fn accounts_are_listed_per_agent_with_what_stopped_being_shared() {
        assert_eq!(
            accounts_text(&info()),
            "Claude Code  CLAUDE_CONFIG_DIR\n  system  (in use)\n  home  /state/accounts/claude/home  (not shared: settings.json)\n  \
             work  /state/accounts/claude/work\nCodex  CODEX_HOME\n  no accounts\n"
        );
    }

    #[test]
    fn names_show_after_each_account_and_the_one_in_use_is_marked() {
        let mut info = info();
        info.providers[0].system_label = "work".into();
        info.providers[0].active = "home".into();
        info.providers[0].accounts[0].label = "Personal".into();
        info.providers[0].accounts[0].unshared.clear();

        assert_eq!(
            accounts_text(&info),
            "Claude Code  CLAUDE_CONFIG_DIR\n  system  called work\n  home  called Personal  \
             /state/accounts/claude/home  (in use)\n  work  /state/accounts/claude/work\nCodex  CODEX_HOME\n  no \
             accounts\n"
        );
    }

    #[test]
    fn a_checked_login_follows_its_account_and_tokens_in_the_server_come_first() {
        let mut info = info();
        info.tokens = vec!["ANTHROPIC_API_KEY".into()];
        info.providers[0].accounts[1].login = Some(LoginInfo {
            state: State::SignedIn,
            email: Some("me@work.dev".into()),
            plan: Some("max".into()),
            checked: "2026-10-19T08:00:00.000Z".into(),
        });
        info.providers[0].accounts[0].login = Some(LoginInfo { state: State::SignedOut, ..LoginInfo::default() });

        assert_eq!(
            accounts_text(&info),
            "set in the server, and they win over the accounts: ANTHROPIC_API_KEY\nClaude Code  CLAUDE_CONFIG_DIR\n  \
             system  (in use)\n  home  /state/accounts/claude/home  signed out  (not shared: settings.json)\n  work  \
             /state/accounts/claude/work  signed in as me@work.dev (max)\nCodex  CODEX_HOME\n  no accounts\n"
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
