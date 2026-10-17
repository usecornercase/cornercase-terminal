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
            asana_workspace: "Acme".into(),
            asana_projects: "1200000000000001 1200000000000002".into(),
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
            hidden_flows: vec!["Arena".into()],
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
            projects: [(
                "~/code/web-shop".into(),
                ProjectSettings { setup: "npm ci\ncp ../.env .env".into(), teardown: "docker compose down".into() },
            )]
            .into(),
        };
        matches_fixture(crate::test_util::fixture!("config/config.json"), &expected);
    }

    #[test]
    fn a_home_relative_key_names_the_absolute_folder() {
        let home = Path::new("/home/ana");
        let mut config = Config::default();
        let settings = ProjectSettings { setup: "npm ci".into(), teardown: String::new() };
        config.set_project(Path::new("/home/ana/code/shop"), Some(home), settings.clone());

        assert_eq!(config.projects.keys().collect::<Vec<_>>(), ["~/code/shop"]);
        assert_eq!(config.project(Path::new("/home/ana/code/shop"), Some(home)), settings);
        assert_eq!(config.project(Path::new("/home/ana/code/other"), Some(home)), ProjectSettings::default());
    }

    #[test]
    fn a_key_through_a_symlink_names_the_same_folder() {
        let dir = TempDir::new();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).expect("dir");
        std::os::unix::fs::symlink(&real, dir.path().join("link")).expect("symlink");
        let mut config = Config::default();
        let settings = ProjectSettings { setup: "make".into(), teardown: String::new() };
        config.projects.insert(format!("{}/link/", dir.path().display()), settings.clone());

        assert_eq!(config.project(&real, None), settings);
        config.set_project(&real, None, ProjectSettings::default());
        assert!(config.projects.is_empty());
    }

    #[test]
    fn saving_a_project_replaces_its_key_and_empty_settings_remove_it() {
        let home = Path::new("/home/ana");
        let folder = Path::new("/home/ana/code/shop");
        let mut config = Config::default();
        config
            .projects
            .insert("/home/ana/code/shop".into(), ProjectSettings { setup: "old".into(), ..Default::default() });
        config.set_project(folder, Some(home), ProjectSettings { setup: "new\r\n\n".into(), teardown: String::new() });

        assert_eq!(config.projects.len(), 1);
        assert_eq!(config.project(folder, Some(home)).setup, "new");

        config.set_project(folder, Some(home), ProjectSettings::default());
        assert!(config.projects.is_empty());
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
        &["linear", "all", "jira", "plane", "asana", "gitlab"]
    )]
    #[case::a_list_from_before_plane(
        r#"{"issue_tabs": ["linear", "all"], "jira_site": ""}"#,
        &["linear", "all", "plane", "asana", "gitlab"]
    )]
    #[case::a_list_from_before_asana(
        r#"{"issue_tabs": ["linear", "all"], "jira_site": "", "plane_workspace": "", "gitlab_url": ""}"#,
        &["linear", "all", "asana"]
    )]
    #[case::asana_already_there(
        r#"{"issue_tabs": ["Asana", "all"], "jira_site": "", "plane_workspace": "", "gitlab_url": ""}"#,
        &["Asana", "all"]
    )]
    #[case::jira_already_there(r#"{"issue_tabs": ["Jira", "all"]}"#, &["Jira", "all", "plane", "asana", "gitlab"])]
    #[case::plane_already_there(
        r#"{"issue_tabs": ["Plane", "all"], "jira_site": ""}"#,
        &["Plane", "all", "asana", "gitlab"]
    )]
    #[case::gitlab_goes_after_github(
        r#"{"issue_tabs": ["all", "GitHub", "linear"], "jira_site": "", "plane_workspace": ""}"#,
        &["all", "GitHub", "gitlab", "linear", "asana"]
    )]
    #[case::gitlab_already_there(
        r#"{"issue_tabs": ["gitlab", "all"], "jira_site": "", "plane_workspace": ""}"#,
        &["gitlab", "all", "asana"]
    )]
    #[case::hidden_since(
        r#"{"issue_tabs": ["all"], "jira_site": "", "plane_workspace": "", "asana_workspace": "", "gitlab_url": ""}"#,
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

mod read {
    use super::*;

    #[test]
    fn tells_a_missing_file_from_a_good_one_and_a_broken_one() {
        let dir = TempDir::new();
        let path = dir.path().join("config.json");
        assert_eq!(read(&path), Ok(None));

        std::fs::write(&path, r#"{"counts": false}"#).expect("write");
        assert_eq!(read(&path).map(|c| c.map(|c| c.counts)), Ok(Some(false)));

        std::fs::write(&path, "{\n  \"counts\": }").expect("write");
        assert!(read(&path).is_err_and(|why| why.contains("line 2")));

        std::fs::write(&path, r#"{"counts": 3}"#).expect("write");
        assert!(read(&path).is_err());
    }

    #[test]
    fn applies_the_migrations() {
        let dir = TempDir::new();
        let path = dir.path().join("config.json");
        std::fs::write(&path, r#"{"context_line": false}"#).expect("write");
        assert_eq!(read(&path).map(|c| c.map(|c| (c.context, c.model))), Ok(Some((false, false))));
    }

    #[test]
    fn refuses_what_is_not_a_regular_file() {
        let dir = TempDir::new();
        assert!(read(dir.path()).is_err());
    }
}

mod changes {
    use super::*;

    #[test]
    fn name_the_keys_that_differ_and_carry_the_new_values() {
        let new = Config { counts: false, fetch_minutes: 9, ..Config::default() };
        let changes = Changes::between(&Config::default(), &new);
        assert_eq!(changes.keys(), ["counts", "fetch_minutes"]);
        assert!(Changes::between(&new, &new).is_empty());
    }

    #[test]
    fn leave_the_other_keys_of_the_target_alone() {
        let changes = Changes::between(&Config::default(), &Config { counts: false, ..Config::default() });
        let target = Config { memory: true, ..Config::default() };
        let merged = changes.apply(&target);
        assert_eq!((merged.counts, merged.memory), (false, true));
    }

    #[test]
    fn take_the_projects_map_as_one_key() {
        let mut new = Config::default();
        new.set_project(Path::new("/srv/a"), None, ProjectSettings { setup: "make".into(), teardown: String::new() });
        assert_eq!(Changes::between(&Config::default(), &new).keys(), ["projects"]);
    }
}

mod project_settings {
    use super::*;

    #[test]
    fn control_characters_become_spaces_and_line_breaks_stay() {
        let mut config = Config::default();
        let settings = ProjectSettings { setup: "a\u{7}b\r\nc\n\n".into(), teardown: String::new() };
        config.set_project(Path::new("/srv/a"), None, settings);
        assert_eq!(config.project(Path::new("/srv/a"), None).setup, "a b\nc");
    }
}
