use rstest::rstest;

use super::*;
use crate::test_util::TempDir;

mod file {
    use super::*;

    fn matches_fixture(fixture: &str, expected: &Config) {
        let dir = TempDir::new();
        let path = dir.path().join("config.json");
        std::fs::write(&path, fixture).expect("write the config fixture");

        assert_eq!(load(&path), *expected);

        save(&path, expected).expect("save the config");
        assert_eq!(
            std::fs::read_to_string(&path).expect("read the saved config"),
            fixture.strip_suffix('\n').expect("fixture newline")
        );
    }

    #[test]
    fn every_key_matches_the_saved_format() {
        let expected = Config {
            worktrees_dir: "worktrees".into(),
            fetch_minutes: 12,
            pull_requests: false,
            merged_pull_requests: false,
            issue_tabs: vec!["shortcut".into(), "github".into(), "gitlab".into()],
            gitlab_url: "https://gitlab.acme.dev".into(),
            jira_site: "acme.atlassian.net".into(),
            jira_email: "ana@acme.dev".into(),
            jira_jql: "project = SHOP".into(),
            plane_url: "https://plane.example".into(),
            plane_workspace: "shop".into(),
            plane_filter: "state:todo".into(),
            agent: "codex".into(),
            agent_args: [("codex".into(), vec!["--sandbox".into(), "workspace-write".into()])].into(),
            agent_modes: [("claude".into(), [("plan".into(), vec!["--permission-mode".into(), "plan".into()])].into())]
                .into(),
            agent_commands: [("codex".into(), "fake-codex".into())].into(),
            prompt: "Fix {url}".into(),
            submit: true,
            accept_trust_prompts: true,
            resume_agents: false,
            keep_awake: true,
            trust_prompt_pattern: "Allow this folder".into(),
            gh: "fake-gh".into(),
            glab: "fake-glab".into(),
            sidebar: "tree".into(),
            tabs: "top".into(),
            icon_bar: true,
            agents_section: true,
            working_icon: "spinner".into(),
            counts: false,
            todo_dot: false,
            dim_inactive_panes: false,
            prefix_key: "ctrl+]".into(),
            model: false,
            context: false,
            memory: true,
            desktop_notifications: "off".into(),
            check_updates: false,
            star_prompt: false,
            remote: true,
            remote_port: 47682,
        };
        matches_fixture(crate::test_util::fixture!("config/config.json"), &expected);
    }

    #[test]
    fn defaults_match_the_saved_format() {
        matches_fixture(crate::test_util::fixture!("config/default.json"), &Config::default());
    }

    #[test]
    fn round_trips() {
        let tmp = TempDir::new();
        let path = tmp.path().join("nested").join("config.json");
        let config = Config { worktrees_dir: "/srv/worktrees".into(), ..Config::default() };

        save(&path, &config).expect("save");

        assert_eq!(load(&path), config);
    }

    #[test]
    fn missing_file_loads_the_defaults() {
        let tmp = TempDir::new();
        assert_eq!(load(&tmp.path().join("config.json")), Config::default());
    }

    #[test]
    fn corrupt_file_loads_the_defaults() {
        let tmp = TempDir::new();
        let path = tmp.path().join("config.json");
        std::fs::write(&path, "{ not json").expect("write");

        assert_eq!(load(&path), Config::default());
    }

    #[test]
    fn missing_keys_take_their_defaults() {
        let tmp = TempDir::new();
        let path = tmp.path().join("config.json");
        std::fs::write(&path, r#"{"worktrees_dir": "/srv/w"}"#).expect("write");

        assert_eq!(load(&path), Config { worktrees_dir: "/srv/w".into(), ..Config::default() });
    }

    #[test]
    fn the_sidebar_starts_with_projects_on_top() {
        assert_eq!(Sidebar::from_setting(&Config::default().sidebar), Sidebar::ProjectsOnTop);
    }

    #[test]
    fn an_agent_tab_starts_with_its_model_and_context_but_not_its_memory() {
        let c = Config::default();
        assert_eq!((c.model, c.context, c.memory), (true, true, false));
    }

    #[rstest]
    #[case::nothing_set("{}")]
    #[case::accepted_before_the_rename(r#"{"auto_accept_trust_prompt": true}"#)]
    fn trust_prompts_are_left_to_you(#[case] file: &str) {
        let tmp = TempDir::new();
        let path = tmp.path().join("config.json");
        std::fs::write(&path, file).expect("write");

        assert!(!load(&path).accept_trust_prompts);
    }

    #[rstest]
    #[case::nothing_set("{}", (true, true))]
    #[case::the_switch_of_0_6_off(r#"{"context_line": false}"#, (false, false))]
    #[case::the_context_off(r#"{"context": false}"#, (false, false))]
    #[case::the_model_set_apart(r#"{"context": false, "model": true}"#, (true, false))]
    #[case::the_context_set_apart(r#"{"model": false}"#, (false, true))]
    #[case::the_new_key_wins(r#"{"context_line": false, "context": true}"#, (true, true))]
    fn a_missing_model_follows_the_context(#[case] file: &str, #[case] expected: (bool, bool)) {
        let tmp = TempDir::new();
        let path = tmp.path().join("config.json");
        std::fs::write(&path, file).expect("write");

        let config = load(&path);

        assert_eq!((config.model, config.context), expected);
    }
}

mod added_tabs {
    use super::*;

    fn tabs(file: &str) -> Vec<String> {
        let tmp = TempDir::new();
        let path = tmp.path().join("config.json");
        std::fs::write(&path, file).expect("write");
        load(&path).issue_tabs
    }

    #[rstest]
    #[case::a_list_from_before_jira(
        r#"{"issue_tabs": ["linear", "all"]}"#,
        &["linear", "all", "jira", "plane", "gitlab"]
    )]
    #[case::a_list_from_before_plane(
        r#"{"issue_tabs": ["linear", "all"], "jira_site": ""}"#,
        &["linear", "all", "plane", "gitlab"]
    )]
    #[case::jira_already_there(r#"{"issue_tabs": ["Jira", "all"]}"#, &["Jira", "all", "plane", "gitlab"])]
    #[case::plane_already_there(
        r#"{"issue_tabs": ["Plane", "all"], "jira_site": ""}"#,
        &["Plane", "all", "gitlab"]
    )]
    #[case::gitlab_goes_after_github(
        r#"{"issue_tabs": ["all", "GitHub", "linear"], "jira_site": "", "plane_workspace": ""}"#,
        &["all", "GitHub", "gitlab", "linear"]
    )]
    #[case::gitlab_already_there(
        r#"{"issue_tabs": ["gitlab", "all"], "jira_site": "", "plane_workspace": ""}"#,
        &["gitlab", "all"]
    )]
    #[case::hidden_since(
        r#"{"issue_tabs": ["all"], "jira_site": "", "plane_workspace": "", "gitlab_url": ""}"#,
        &["all"]
    )]
    #[case::no_list("{}", &DEFAULT_ISSUE_TABS)]
    fn are_added_to_lists_saved_before_they_existed(#[case] file: &str, #[case] expected: &[&str]) {
        assert_eq!(tabs(file), expected);
    }
}

mod expand_home {
    use super::*;

    #[rstest]
    #[case::tilde_alone("~", "/home/ana")]
    #[case::tilde_slash("~/.cornercase/worktrees", "/home/ana/.cornercase/worktrees")]
    #[case::absolute("/srv/worktrees", "/srv/worktrees")]
    #[case::other_users_home("~bob/x", "~bob/x")]
    fn replaces_a_leading_tilde(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(expand_home(input, Some(Path::new("/home/ana"))), PathBuf::from(expected));
    }

    #[test]
    fn keeps_the_tilde_without_a_home() {
        assert_eq!(expand_home("~/x", None), PathBuf::from("~/x"));
    }
}

mod check_worktrees_dir {
    use super::*;

    #[rstest]
    #[case::tilde(" ~/worktrees ", Ok("~/worktrees".to_string()))]
    #[case::absolute("/srv/worktrees", Ok("/srv/worktrees".to_string()))]
    #[case::empty("  ", Err("the folder is required"))]
    #[case::relative("worktrees", Err("use an absolute path or one starting with ~/"))]
    fn accepts_only_absolute_folders(#[case] input: &str, #[case] expected: Result<String, &'static str>) {
        assert_eq!(check_worktrees_dir(input, Some(Path::new("/home/ana"))), expected);
    }
}

mod check_fetch_minutes {
    use super::*;

    #[rstest]
    #[case::minutes(" 15 ", Ok(15))]
    #[case::off("0", Ok(0))]
    #[case::empty("", Err("use a whole number of minutes, 0 turns it off"))]
    #[case::negative("-1", Err("use a whole number of minutes, 0 turns it off"))]
    #[case::fraction("1.5", Err("use a whole number of minutes, 0 turns it off"))]
    fn accepts_whole_minutes(#[case] input: &str, #[case] expected: Result<u64, &'static str>) {
        assert_eq!(check_fetch_minutes(input), expected);
    }
}

mod fetch_every {
    use super::*;

    #[rstest]
    #[case::default(DEFAULT_FETCH_MINUTES, Some(Duration::from_secs(300)))]
    #[case::off(0, None)]
    fn is_the_interval_in_minutes(#[case] fetch_minutes: u64, #[case] expected: Option<Duration>) {
        assert_eq!(Config { fetch_minutes, ..Config::default() }.fetch_every(), expected);
    }
}

mod sidebar_setting;
