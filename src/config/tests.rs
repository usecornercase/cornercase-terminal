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

    fn shop_projects() -> BTreeMap<String, ProjectSettings> {
        let shop = ProjectSettings {
            setup: "npm ci\ncp ../.env .env".into(),
            teardown: "docker compose down".into(),
            builds: Some(vec!["node_modules".into(), ".next".into()]),
        };
        [("~/code/web-shop".to_string(), shop)].into()
    }

    fn interface_keys() -> Config {
        Config {
            icon_bar: true,
            icons: "on".into(),
            agents_section: true,
            working_icon: "spinner".into(),
            counts: false,
            todo_dot: false,
            dim_inactive_panes: false,
            prefix_key: "ctrl+]".into(),
            model: false,
            context: false,
            memory: true,
            account_mark: false,
            account_switcher: false,
            account_picker: false,
            desktop_notifications: "off".into(),
            sound: true,
            sound_waiting: "~/sounds/ping.wav".into(),
            sound_done: "/usr/share/sounds/done.oga".into(),
            sound_subagents: true,
            check_updates: false,
            star_prompt: false,
            tour: false,
            remote: true,
            remote_port: 47682,
            discord_forum: true,
            discord_forum_close: "delete".into(),
            projects: shop_projects(),
            account_names: [(
                "claude".into(),
                [(String::new(), "work".into()), ("home".into(), "Personal".into())].into(),
            )]
            .into(),
            active_accounts: [("claude".into(), "home".into())].into(),
            ..Config::default()
        }
    }

    #[test]
    fn every_key_matches_the_saved_format() {
        let expected = Config {
            worktrees_dir: "worktrees".into(),
            fetch_minutes: 12,
            pull_requests: false,
            merged_pull_requests: false,
            shared_files: true,
            issue_branch: "feat/{key}-{slug}".into(),
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
            default_flow: "Quick".into(),
            flow_order: vec!["Quick".into(), "Arena".into()],
            trust_prompt_pattern: "Allow this folder".into(),
            gh: "fake-gh".into(),
            glab: "fake-glab".into(),
            sidebar: "tree".into(),
            tabs: "top".into(),
            ..interface_keys()
        };
        matches_fixture(crate::test_util::fixture!("config/config.json"), &expected);
    }

    #[test]
    fn a_home_relative_key_names_the_absolute_folder() {
        let home = Path::new("/home/ana");
        let mut config = Config::default();
        let settings = ProjectSettings { setup: "npm ci".into(), teardown: String::new(), ..Default::default() };
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
        let settings = ProjectSettings { setup: "make".into(), teardown: String::new(), ..Default::default() };
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
        config.set_project(
            folder,
            Some(home),
            ProjectSettings { setup: "new\r\n\n".into(), teardown: String::new(), ..Default::default() },
        );

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
    fn a_config_from_before_the_discord_forum_keeps_it_off_and_archives_closed_threads() {
        let tmp = TempDir::new();
        let path = tmp.path().join("config.json");
        std::fs::write(&path, r#"{"remote": true, "remote_port": 47682}"#).expect("write");

        let c = load(&path);
        assert_eq!((c.remote, c.discord_forum), (true, false));
        assert_eq!(ForumClose::from_setting(&c.discord_forum_close), ForumClose::Archive);
    }

    #[rstest]
    #[case::archive("archive", ForumClose::Archive)]
    #[case::delete(" Delete ", ForumClose::Delete)]
    #[case::unknown("burn", ForumClose::Archive)]
    fn a_closed_workspace_thread_is_archived_unless_told_to_delete(#[case] setting: &str, #[case] close: ForumClose) {
        assert_eq!(ForumClose::from_setting(setting), close);
    }

    #[test]
    fn a_config_from_before_sounds_plays_them_for_the_agents_you_started() {
        let tmp = TempDir::new();
        let path = tmp.path().join("config.json");
        std::fs::write(&path, r#"{"desktop_notifications": "osc9"}"#).expect("write");

        let c = load(&path);
        assert_eq!(
            (c.sound, c.sound_waiting.as_str(), c.sound_done.as_str(), c.sound_subagents),
            (true, "", "", false)
        );
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

mod check_sound_file {
    use super::*;

    const ABSOLUTE: &str = "use an absolute path or one starting with ~/";

    #[rstest]
    #[case::empty("", Ok(String::new()))]
    #[case::tilde("~/ding.wav", Ok("~/ding.wav".to_string()))]
    #[case::absolute("/usr/share/sounds/done.oga", Ok("/usr/share/sounds/done.oga".to_string()))]
    #[case::relative("ding.wav", Err(ABSOLUTE))]
    #[case::like_an_option("-q", Err(ABSOLUTE))]
    #[case::another_home("~ana/ding.wav", Err(ABSOLUTE))]
    #[case::control("bad\u{7}.wav", Err("a file's path cannot hold control or invisible characters"))]
    fn accepts_only_absolute_files(#[case] input: &str, #[case] expected: Result<String, &'static str>) {
        assert_eq!(check_sound_file(input), expected);
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
        new.set_project(
            Path::new("/srv/a"),
            None,
            ProjectSettings { setup: "make".into(), teardown: String::new(), ..Default::default() },
        );
        assert_eq!(Changes::between(&Config::default(), &new).keys(), ["projects"]);
    }
}

mod profiles {
    use super::*;

    fn work() -> Config {
        let accounts = BTreeMap::from([("claude".to_string(), "work".to_string())]);
        Config {
            profiles: BTreeMap::from([("Work".to_string(), accounts)]),
            profile: "Work".into(),
            ..Config::default()
        }
    }

    #[test]
    fn are_left_out_of_the_file_while_there_are_none() {
        let dir = TempDir::new();
        let path = dir.path().join("config.json");
        save(&path, &Config::default()).expect("save the config");
        let text = std::fs::read_to_string(&path).expect("read the saved config");
        assert!(!text.contains("\"profile\":") && !text.contains("\"profiles\":"), "{text}");
    }

    #[test]
    fn round_trip_through_the_file() {
        let dir = TempDir::new();
        let path = dir.path().join("config.json");
        save(&path, &work()).expect("save the config");
        let text = std::fs::read_to_string(&path).expect("read the saved config");
        assert!(text.contains("\"profile\": \"Work\""), "{text}");
        assert_eq!(load(&path), work());
    }

    #[test]
    fn the_active_one_gives_its_accounts() {
        let config = work();
        let (name, accounts) = config.active_profile().expect("an active profile");
        assert_eq!((name, accounts.get("claude").map(String::as_str)), ("Work", Some("work")));
        assert_eq!(config.unknown_profile(), None);
    }

    #[test]
    fn an_empty_one_is_the_system() {
        let config = Config { profile: String::new(), ..work() };
        assert_eq!(config.active_profile(), None);
        assert_eq!(config.unknown_profile(), None);
    }

    #[test]
    fn one_that_does_not_exist_counts_as_the_system() {
        let config = Config { profile: "Gone".into(), ..work() };
        assert_eq!(config.active_profile(), None);
        assert_eq!(config.unknown_profile(), Some("Gone"));
    }

    #[test]
    fn going_back_to_the_system_is_a_change_that_applies() {
        let changes = Changes::between(&work(), &Config::default());
        assert_eq!(changes.keys(), ["profile", "profiles"]);
        let merged = changes.apply(&Config { memory: true, ..work() });
        assert_eq!((merged.profile.as_str(), merged.profiles.is_empty(), merged.memory), ("", true, true));
    }

    #[test]
    fn the_selection_is_the_active_profile_s_else_each_agent_s_own() {
        let own = BTreeMap::from([("claude".to_string(), "home".to_string())]);
        let config = Config { active_accounts: own.clone(), ..work() };
        assert_eq!(config.selected_accounts().get("claude").map(String::as_str), Some("work"));
        let gone = Config { profile: "Gone".into(), ..config.clone() };
        assert_eq!(gone.selected_accounts(), &own);
        let system = Config { profile: String::new(), ..config };
        assert_eq!(system.selected_accounts(), &own);
        assert!(Config::default().selected_accounts().is_empty());
    }

    #[test]
    fn an_account_is_named_by_its_label_else_its_folder_and_the_system_by_system() {
        let labels =
            BTreeMap::from([(String::new(), "work".to_string()), ("home".to_string(), "Personal".to_string())]);
        let config = Config { account_names: BTreeMap::from([("claude".to_string(), labels)]), ..Config::default() };
        assert_eq!(config.account_label("claude", ""), "work");
        assert_eq!(config.account_label("claude", "home"), "Personal");
        assert_eq!(config.account_label("claude", "side"), "side");
        assert_eq!(config.account_label("codex", ""), SYSTEM_ACCOUNT);
    }

    #[test]
    fn forgetting_an_account_drops_it_from_profiles_selection_and_labels() {
        let labels = BTreeMap::from([("work".to_string(), "Work".to_string())]);
        let mut config = Config {
            account_names: BTreeMap::from([("claude".to_string(), labels)]),
            active_accounts: BTreeMap::from([("claude".to_string(), "work".to_string())]),
            ..work()
        };
        config.forget_account("claude", "work");
        assert!(config.profiles["Work"].is_empty());
        assert!(config.active_accounts.is_empty() && config.account_names.is_empty());
    }

    #[test]
    fn labels_and_the_selection_are_left_out_of_the_file_while_empty() {
        let dir = TempDir::new();
        let path = dir.path().join("config.json");
        save(&path, &Config::default()).expect("save the config");
        let text = std::fs::read_to_string(&path).expect("read the saved config");
        assert!(!text.contains("account_names") && !text.contains("active_accounts"), "{text}");
        let own = BTreeMap::from([("claude".to_string(), "home".to_string())]);
        let chosen = Config { active_accounts: own, ..Config::default() };
        assert_eq!(Changes::between(&Config::default(), &chosen).keys(), ["active_accounts"]);
        save(&path, &chosen).expect("save the config");
        assert_eq!(load(&path), chosen);
    }

    #[test]
    fn choosing_one_is_a_change_of_its_key_alone() {
        let changes = Changes::between(&Config::default(), &Config { profile: "Work".into(), ..Config::default() });
        assert_eq!(changes.keys(), ["profile"]);
    }
}

mod project_settings {
    use super::*;

    #[test]
    fn control_characters_become_spaces_and_line_breaks_stay() {
        let mut config = Config::default();
        let settings =
            ProjectSettings { setup: "a\u{7}b\r\nc\n\n".into(), teardown: String::new(), ..Default::default() };
        config.set_project(Path::new("/srv/a"), None, settings);
        assert_eq!(config.project(Path::new("/srv/a"), None).setup, "a b\nc");
    }
}

mod project_builds {
    use rstest::rstest;

    use super::*;
    use crate::config::{DEFAULT_BUILDS, parse_builds};

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn no_list_means_the_defaults() {
        assert_eq!(ProjectSettings::default().builds(), names(&DEFAULT_BUILDS));
        let custom = ProjectSettings { builds: Some(names(&["out"])), ..Default::default() };
        assert_eq!(custom.builds(), ["out"]);
        let none = ProjectSettings { builds: Some(Vec::new()), ..Default::default() };
        assert_eq!(none.builds(), Vec::<String>::new());
    }

    #[test]
    fn saving_the_defaults_stores_nothing_and_a_custom_list_alone_keeps_its_key() {
        let mut config = Config::default();
        let folder = Path::new("/srv/shop");
        let defaults = ProjectSettings { builds: Some(names(&DEFAULT_BUILDS)), ..Default::default() };
        config.set_project(folder, None, defaults);
        assert!(config.projects.is_empty());
        let reordered = ProjectSettings {
            builds: Some(names(&[
                "target",
                "node_modules",
                ".next",
                ".turbo",
                "dist",
                "build",
                ".venv",
                "__pycache__",
            ])),
            ..Default::default()
        };
        config.set_project(folder, None, reordered);
        assert!(config.projects.is_empty());
        let custom = ProjectSettings { builds: Some(names(&["out", "out", " cache "])), ..Default::default() };
        config.set_project(folder, None, custom);
        assert_eq!(config.project(folder, None).builds, Some(names(&["out", "cache"])));
        let json = serde_json::to_value(&config.projects).expect("json");
        assert_eq!(json["/srv/shop"], serde_json::json!({"builds": ["out", "cache"]}));
    }

    #[rstest]
    #[case::spaces("node_modules  target", Some(&["node_modules", "target"][..]))]
    #[case::commas("a,b, c", Some(&["a", "b", "c"][..]))]
    #[case::repeats("a a", Some(&["a"][..]))]
    #[case::empty("  ", Some(&[][..]))]
    fn a_text_is_split_into_names(#[case] text: &str, #[case] expected: Option<&[&str]>) {
        assert_eq!(parse_builds(text).expect("parsed"), expected.map(names));
    }

    #[rstest]
    #[case::a_path("a/b")]
    #[case::the_folder_itself(".")]
    #[case::the_parent("..")]
    #[case::git(".git")]
    #[case::a_control("a\u{7}b")]
    fn a_bad_name_is_refused(#[case] text: &str) {
        let error = parse_builds(text).expect_err("refused");
        assert!(error.contains("is not a folder name"), "{error}");
    }
}
