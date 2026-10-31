use std::collections::BTreeMap;

use rstest::rstest;
use serde_json::{Value, json};

use super::*;
use crate::config::ProjectSettings;
use crate::machines::Machine;

fn changed_config() -> Config {
    Config {
        sidebar: "tree".into(),
        prompt: "Work on {url}\nand open a pull request".into(),
        agent_args: BTreeMap::from([("claude".into(), vec!["--model".into(), "opus".into()])]),
        agent_commands: BTreeMap::from([("claude".into(), "~/bin/claude".into())]),
        issue_tabs: vec!["jira".into(), "all".into()],
        fetch_minutes: 9,
        ..Config::default()
    }
}

fn shared() -> Shared {
    Shared {
        settings: settings_of(&changed_config()),
        repositories: vec![Repository {
            remotes: vec!["github.com/alice/web-shop".into(), "github.com/acme/web-shop".into()],
            setup: "npm ci\ncp .env.example .env".into(),
            teardown: String::new(),
        }],
        connect: vec!["jira".into()],
        unknown: Vec::new(),
    }
}

mod the_sections {
    use super::*;

    #[test]
    fn place_every_key_of_the_config_or_leave_it_out_on_purpose() {
        let mut config = Config::default();
        config.profiles.insert("p".into(), BTreeMap::new());
        config.profile = "p".into();
        config.account_names.insert("claude".into(), BTreeMap::new());
        config.active_accounts.insert("claude".into(), "x".into());
        config.projects.insert("~/x".into(), ProjectSettings::default());
        config.machines.push(Machine { name: "vps".into(), ..Machine::default() });
        let Value::Object(keys) = serde_json::to_value(&config).expect("json") else { panic!("an object") };
        for key in keys.keys() {
            assert!(section_of(key).is_some() != LEFT_OUT.contains(&key.as_str()), "{key} is placed nowhere or twice");
        }
    }

    #[test]
    fn are_found_by_their_id() {
        for section in Section::ALL {
            assert_eq!(Section::of(section.id()), Some(section));
        }
        assert_eq!(Section::of("colours"), None);
    }

    #[test]
    fn hold_every_shared_key_with_its_current_value() {
        let settings = settings_of(&changed_config());
        assert_eq!(settings.get("sidebar"), Some(&json!("tree")));
        assert_eq!(settings.get("fetch_minutes"), Some(&json!(9)));
        assert_eq!(settings.get("check_updates"), Some(&json!(true)));
        assert_eq!(settings.len(), Section::ALL.iter().map(|s| s.keys().len()).sum::<usize>());
    }
}

mod a_round_trip {
    use super::*;

    #[test]
    fn gives_back_every_shared_setting_and_repository() {
        let shared = shared();
        let text = to_text(&shared).expect("export");
        assert_eq!(parse(&text).expect("import"), shared, "{text}");
    }

    #[test]
    fn starts_with_the_header_and_the_version() {
        let text = to_text(&shared()).expect("export");
        assert!(text.starts_with(HEADER), "{text}");
        assert!(text.contains("version = 1"), "{text}");
    }

    #[test]
    fn writes_the_sections_in_their_order_and_the_repositories_last() {
        let text = to_text(&shared()).expect("export");
        let at = |part: &str| text.find(part).unwrap_or_else(|| panic!("{part} in {text}"));
        let order = ["connect = ", "[agents]", "[issues]", "[worktrees]", "[interface]", "[[repositories]]"];
        assert!(order.windows(2).all(|pair| at(pair[0]) < at(pair[1])), "{text}");
        assert!(text.contains("teardown = \"\""), "{text}");
    }

    #[test]
    fn writes_a_text_with_a_newline_as_a_multi_line_string() {
        let text = to_text(&shared()).expect("export");
        assert!(text.contains("\"\"\""), "{text}");
    }

    #[test]
    fn reads_a_text_with_a_bom_and_windows_line_ends() {
        let shared = shared();
        let text = to_text(&shared).expect("export");
        let windows = format!("\u{feff}{}", text.replace('\n', "\r\n"));
        assert_eq!(parse(&windows).expect("import"), shared);
    }

    #[test]
    fn leaves_out_connect_when_there_is_nothing_to_connect() {
        let shared = Shared { connect: Vec::new(), ..shared() };
        let text = to_text(&shared).expect("export");
        assert!(!text.contains("connect"), "{text}");
        assert_eq!(parse(&text).expect("import"), shared);
    }
}

mod what_stays_out {
    use super::*;

    #[test]
    fn personal_and_machine_keys_never_reach_the_text() {
        let mut config = changed_config();
        config.jira_email = "me@acme.com".into();
        config.remote = true;
        config.keep_awake = true;
        config.profiles.insert("work".into(), BTreeMap::new());
        let shared = Shared { settings: settings_of(&config), ..Shared::default() };
        let text = to_text(&shared).expect("export");
        for word in ["me@acme.com", "remote_port", "keep_awake", "profiles", "jira_email"] {
            assert!(!text.contains(word), "{word} in {text}");
        }
    }

    #[test]
    fn a_key_of_no_section_is_not_written() {
        let mut settings = settings_of(&Config::default());
        settings.insert("jira_email".into(), json!("me@acme.com"));
        let text = to_text(&Shared { settings, ..Shared::default() }).expect("export");
        assert!(!text.contains("me@acme.com"), "{text}");
    }
}

mod refusals {
    use super::*;

    #[rstest]
    #[case::newer_version(
        "version = 2\n",
        "made by a newer cornercase (setup version 2); update cornercase to import it"
    )]
    #[case::no_version("[interface]\nsidebar = \"tree\"\n", "this is not a cornercase setup: it has no version")]
    #[case::not_toml("version = \n", "line 1:")]
    #[case::version_not_a_number("version = \"1\"\n", "version must be")]
    #[case::connect_not_a_list("version = 1\nconnect = \"jira\"\n", "connect must be")]
    #[case::section_not_a_table("version = 1\ninterface = 3\n", "interface must be")]
    #[case::repositories_not_tables("version = 1\nrepositories = [1]\n", "repositories must be")]
    #[case::remotes_not_strings("version = 1\n[[repositories]]\nremotes = [1]\n", "remotes of a repository must be")]
    #[case::no_remotes("version = 1\n[[repositories]]\nsetup = \"x\"\n", "remotes of a repository must be")]
    #[case::setup_not_a_string(
        "version = 1\n[[repositories]]\nremotes = [\"a/b\"]\nsetup = 1\n",
        "setup of a repository must be"
    )]
    fn are_explained(#[case] text: &str, #[case] start: &str) {
        let error = parse(text).expect_err("a refusal");
        assert!(error.starts_with(start), "{error}");
    }

    #[test]
    fn a_text_past_the_limit_is_too_large() {
        assert_eq!(parse(&"#".repeat(MAX_BYTES + 1)), Err(too_large()));
        assert_eq!(too_large(), "too large to share (at most 256 KiB)");
    }

    #[test]
    fn an_export_past_the_limit_is_too_large() {
        let mut settings = settings_of(&Config::default());
        settings.insert("prompt".into(), json!("x".repeat(MAX_BYTES)));
        assert_eq!(to_text(&Shared { settings, ..Shared::default() }), Err(too_large()));
    }
}

mod unknown_keys {
    use super::*;

    #[test]
    fn are_named_and_skipped() {
        let text =
            "version = 1\nfuture = 1\n[interface]\nsidebar = \"tree\"\ncolours = 3\n[agents]\nsidebar = \"top\"\n";
        let shared = parse(text).expect("import");
        assert_eq!(shared.unknown, ["future", "interface.colours", "agents.sidebar"]);
        assert_eq!(shared.settings, BTreeMap::from([("sidebar".to_string(), json!("tree"))]));
    }

    #[test]
    fn a_repository_key_is_unknown_too() {
        let text = "version = 1\n[[repositories]]\nremotes = [\"github.com/acme/web-shop\"]\ncolour = 1\n";
        let shared = parse(text).expect("import");
        assert_eq!(shared.unknown, ["repositories.colour"]);
        assert_eq!(
            shared.repositories,
            [Repository { remotes: vec!["github.com/acme/web-shop".into()], ..Repository::default() }]
        );
    }
}
