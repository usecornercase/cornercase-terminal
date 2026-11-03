use std::path::PathBuf;

use rstest::rstest;

use super::*;
use crate::control::accounts::{AccountInfo, AgentInstalled, LoginInfo};
use crate::control::issues::SourceInfo;
use crate::control::{BrokenFlow, FlowSpec};

fn login(state: State) -> LoginInfo {
    LoginInfo { state, ..LoginInfo::default() }
}

fn account(name: &str, label: &str, state: State) -> AccountInfo {
    AccountInfo { name: name.into(), label: label.into(), login: Some(login(state)), ..AccountInfo::default() }
}

fn claude(own: State, work: State) -> ProviderInfo {
    ProviderInfo {
        provider: "claude".into(),
        accounts: vec![account("work", "Work", work), account("home", "", State::SignedIn)],
        active: "work".into(),
        system_login: Some(login(own)),
        ..ProviderInfo::default()
    }
}

fn installed(agent: &str, installed: bool) -> AgentInstalled {
    AgentInstalled { agent: agent.into(), command: agent.into(), installed }
}

fn info(own: State, work: State) -> AccountsInfo {
    let codex = ProviderInfo { provider: "codex".into(), ..ProviderInfo::default() };
    AccountsInfo {
        providers: vec![claude(own, work), codex],
        agents: vec![installed("claude", true), installed("codex", false)],
        ..AccountsInfo::default()
    }
}

mod its_accounts {
    use super::*;

    #[test]
    fn are_the_ones_listed_there_with_their_labels_and_choice() {
        let read = accounts(&info(State::SignedIn, State::SignedIn)).expect("accounts");

        let names = vec!["work".to_string(), "home".to_string()];
        let labels = vec!["System".to_string(), "Work".to_string(), "home".to_string()];
        let claude = AgentAccounts { names, labels, active: "work".into() };
        assert_eq!(read, Accounts { agents: HashMap::from([("claude".to_string(), claude)]), chosen: None });
    }

    #[test]
    fn choose_its_own_login_when_the_active_one_is_gone() {
        let mut info = info(State::SignedIn, State::SignedIn);
        info.providers[0].active = "gone".into();
        info.providers[0].system_label = "Mine".into();

        let read = accounts(&info).expect("accounts");

        let claude = read.of("claude").expect("claude");
        assert_eq!((claude.active.as_str(), claude.labels[0].as_str()), ("", "Mine"));
    }

    #[test]
    fn are_none_when_no_agent_has_one() {
        let info = AccountsInfo { providers: vec![ProviderInfo::default()], ..AccountsInfo::default() };

        assert_eq!(accounts(&info), None);
    }
}

mod the_agent {
    use super::*;

    #[rstest]
    #[case::ready("claude", "", State::SignedIn, AgentThere::Ready)]
    #[case::missing_its_command("codex", "", State::SignedIn, AgentThere::NotInstalled)]
    #[case::not_listed("gemini", "", State::SignedIn, AgentThere::Unknown)]
    #[case::signed_out_of_its_own_login("claude", "", State::SignedOut, AgentThere::SignedOut)]
    #[case::signed_in_to_the_chosen_account("claude", "home", State::SignedOut, AgentThere::Ready)]
    #[case::of_an_unknown_login("claude", "", State::Unknown, AgentThere::Ready)]
    fn is_told_from_what_the_machine_checked(
        #[case] agent: &str,
        #[case] account: &str,
        #[case] own: State,
        #[case] expected: AgentThere,
    ) {
        assert_eq!(agent_state(&info(own, State::SignedIn), agent, account), expected);
    }

    #[test]
    fn is_signed_out_of_the_chosen_account_even_with_its_own_login() {
        assert_eq!(agent_state(&info(State::SignedIn, State::SignedOut), "claude", "work"), AgentThere::SignedOut);
    }

    #[test]
    fn signed_out_works_with_a_token_in_the_environment_there() {
        let info = AccountsInfo { tokens: vec!["ANTHROPIC_API_KEY".into()], ..info(State::SignedOut, State::SignedIn) };

        assert_eq!(agent_state(&info, "claude", ""), AgentThere::Ready);
    }

    #[test]
    fn is_unknown_to_a_machine_that_did_not_check_the_agents() {
        let info = AccountsInfo { agents: Vec::new(), ..info(State::SignedOut, State::SignedIn) };

        assert_eq!(agent_state(&info, "claude", ""), AgentThere::Unknown);
    }
}

mod the_tracker {
    use super::*;

    fn source(source: &str, name: &str, state: SourceState) -> SourceInfo {
        SourceInfo { source: source.into(), name: name.into(), state, ..SourceInfo::default() }
    }

    fn lacks(found: SourceInfo, source: &str) -> Option<String> {
        let sources = Sources { sources: vec![found], ..Sources::default() };
        Tracked { machine: "vps", project: "web-shop", source }.lacks(&sources)
    }

    #[test]
    fn says_nothing_when_connected() {
        assert_eq!(lacks(source("github", "GitHub", SourceState::Connected), "github"), None);
    }

    #[rstest]
    #[case::a_token(&["token", "workspace"], "vps has no Plane token for web-shop")]
    #[case::settings(&["site", "email"], "vps lacks site, email for Plane in web-shop")]
    fn says_what_is_missing_there(#[case] missing: &[&str], #[case] said: &str) {
        let missing = missing.iter().map(ToString::to_string).collect();
        let found = SourceInfo { missing, ..source("plane", "Plane", SourceState::Missing) };

        assert_eq!(lacks(found, "plane").as_deref(), Some(said));
    }

    #[test]
    fn says_why_it_fails_there() {
        let found = SourceInfo {
            reason: Some("gh is not logged in".into()),
            ..source("github", "GitHub", SourceState::Failing)
        };

        assert_eq!(lacks(found, "github").as_deref(), Some("GitHub on vps: gh is not logged in"));
    }

    #[test]
    fn says_a_few_of_many_missing_settings_and_how_many_more() {
        let missing = ["site", "email", "workspace", "project", "team"].map(String::from).to_vec();
        let found = SourceInfo { missing, ..source("plane", "Plane", SourceState::Missing) };

        assert_eq!(
            lacks(found, "plane").as_deref(),
            Some("vps lacks site, email, workspace … and 2 more for Plane in web-shop")
        );
    }

    #[test]
    fn cuts_a_long_reason_and_name() {
        let found =
            SourceInfo { reason: Some("x".repeat(500)), ..source("github", &"G".repeat(500), SourceState::Failing) };

        let said = lacks(found, "github").expect("a reason");

        assert_eq!(said, format!("{}… on vps: {}…", "G".repeat(59), "x".repeat(59)));
    }

    #[test]
    fn cuts_a_long_project_name_there() {
        let project = "w".repeat(500);
        let found = source("github", "GitHub", SourceState::Missing);
        let sources =
            Sources { sources: vec![SourceInfo { missing: vec!["token".into()], ..found }], ..Sources::default() };

        let said = Tracked { machine: "vps", project: &project, source: "github" }.lacks(&sources);

        assert_eq!(said, Some(format!("vps has no GitHub token for {}…", "w".repeat(59))));
    }

    #[rstest]
    #[case::nothing_missing(SourceInfo { missing: vec![" ".into(), "\u{7}".into()], ..source("plane", "Plane", SourceState::Missing) }, "vps lacks settings for Plane in web-shop")]
    #[case::no_reason(SourceInfo { reason: Some("\n ".into()), ..source("github", "GitHub", SourceState::Failing) }, "GitHub on vps: failing")]
    #[case::no_name(SourceInfo { missing: vec!["token".into()], ..source("plane", " \t", SourceState::Missing) }, "vps has no plane token for web-shop")]
    fn never_leaves_a_blank_where_the_machine_said_nothing(#[case] found: SourceInfo, #[case] said: &str) {
        let source = found.source.clone();

        assert_eq!(lacks(found, &source).as_deref(), Some(said));
    }

    #[test]
    fn names_a_project_with_a_blank_name_there_the_project() {
        let found = SourceInfo { missing: vec!["token".into()], ..source("github", "GitHub", SourceState::Missing) };
        let sources = Sources { sources: vec![found], ..Sources::default() };

        let said = Tracked { machine: "vps", project: "\u{1b} ", source: "github" }.lacks(&sources);

        assert_eq!(said.as_deref(), Some("vps has no GitHub token for the project"));
    }

    #[test]
    fn keeps_a_blank_refusal_a_visible_error() {
        let mut cached = Cached::<Sources>::default();

        cached.heard(Err(Refusal::new(crate::control::contract::Code::Failed, " \r\n")), "vps");

        assert_eq!(cached.error.as_deref(), Some("vps gave no reason"));
    }

    #[test]
    fn says_a_machine_that_does_not_list_it_cannot_read_it() {
        assert_eq!(
            lacks(source("github", "GitHub", SourceState::Connected), "asana").as_deref(),
            Some("vps cannot read asana issues")
        );
    }
}

mod its_flows {
    use super::*;

    fn flow(name: &str, origin: &str, path: Option<&str>, default: bool) -> FlowInfo {
        let spec = FlowSpec { name: name.into(), ..FlowSpec::default() };
        let path = path.map(PathBuf::from);
        FlowInfo { name: name.into(), origin: origin.into(), path, flow: spec, default, ..FlowInfo::default() }
    }

    #[test]
    fn are_listed_as_the_dialog_lists_its_own() {
        let flows = Flows {
            flows: vec![
                flow("Quick", "built-in", None, false),
                flow("Pair", "user", Some("/home/ana/pair.toml"), true),
            ],
            broken: vec![BrokenFlow { path: "/srv/web/bad.toml".into(), error: "no roles".into() }],
            hidden: vec![flow("Full", "built-in", None, false)],
            builtins: vec!["Quick".into(), "Full".into()],
        };

        let read = listing(&flows);

        let origins: Vec<(&str, &Origin)> =
            read.flows.iter().map(|saved| (saved.flow.name.as_str(), &saved.origin)).collect();
        assert_eq!(origins, [("Quick", &Origin::BuiltIn), ("Pair", &Origin::User("/home/ana/pair.toml".into()))]);
        let hidden: Vec<&str> = read.hidden.iter().map(|flow| flow.name.as_str()).collect();
        assert_eq!((hidden, read.default.as_deref()), (vec!["Full"], Some("Pair")));
        assert_eq!(
            (read.broken, read.builtins),
            (vec![("/srv/web/bad.toml".into(), "no roles".into())], flows.builtins)
        );
    }

    #[test]
    fn of_a_project_keep_their_file() {
        let flows =
            Flows { flows: vec![flow("Ship", "project", Some("/srv/web/ship.toml"), false)], ..Flows::default() };

        assert_eq!(listing(&flows).flows[0].origin, Origin::Project("/srv/web/ship.toml".into()));
    }
}
