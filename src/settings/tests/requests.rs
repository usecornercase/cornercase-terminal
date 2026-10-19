use serde_json::{Value, json};

use super::*;
use crate::control::config::{Kind, State};

fn full() -> Settings {
    let tokens = Source::REMOTE.into_iter().map(|source| (source, Status::Missing)).collect();
    let mut s = Settings::new(Config::default(), Some(PathBuf::from("/home/ana")), tokens, Page::default());
    s.browsers = 1;
    s.messenger_mut(Via::Telegram).bot = Some("@ana_bot".into());
    s.messenger_mut(Via::Telegram).chat = Some("Ana".into());
    s.plugins.push(PluginRow { id: "x".into(), label: "x".into(), on: false, note: String::new(), broken: false });
    s
}

fn value_of(s: &Settings, key: &str) -> Value {
    s.describe().into_iter().find(|setting| setting.key == key).map(|setting| setting.value).expect("a setting")
}

mod describing {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn every_key_is_unique() {
        let keys: Vec<String> = full().describe().into_iter().map(|s| s.key).collect();
        assert_eq!(keys.iter().collect::<HashSet<_>>().len(), keys.len(), "{keys:?}");
    }

    #[test]
    fn every_row_of_the_modal_is_a_setting_or_a_known_connection() {
        let s = full();
        let rows: Vec<Row> = s
            .entries()
            .into_iter()
            .filter_map(|(_, target)| match target {
                describe::Target::Row(row) => Some(row),
                _ => None,
            })
            .collect();
        for row in s.every_row() {
            let skipped = matches!(
                row,
                Row::Token(_)
                    | Row::Bot(_)
                    | Row::Chat(_)
                    | Row::Browsers
                    | Row::Kind(_)
                    | Row::Tab(_)
                    | Row::AddAgent
                    | Row::Plugin(_)
                    | Row::NoPlugins
                    | Row::TourStart
                    | Row::TourReset
            ) || row.page() == Page::Accounts;
            assert!(skipped || rows.contains(&row), "{row:?} has no key");
        }
    }

    #[test]
    fn a_default_config_is_every_default() {
        for setting in full().describe() {
            assert_eq!(setting.value, setting.default, "{}", setting.key);
        }
    }

    #[test]
    fn issue_tabs_are_one_list_in_order() {
        let setting = full().describe().into_iter().find(|s| s.key == "issue_tabs").expect("issue_tabs");
        assert_eq!((setting.kind, setting.choices.len()), (Kind::List, 8));
        assert_eq!(setting.value, json!(["all", "github", "gitlab", "shortcut", "linear", "jira", "plane", "asana"]));
    }

    #[test]
    fn every_agent_has_extra_arguments_and_those_with_modes_a_mode() {
        let keys: Vec<String> = full().describe().into_iter().map(|s| s.key).collect();
        assert!(keys.contains(&"agents.claude.mode".to_string()), "{keys:?}");
        assert!(keys.contains(&"agents.claude.extra".to_string()), "{keys:?}");
        assert!(keys.contains(&"agents.codex.extra".to_string()), "{keys:?}");
    }

    #[test]
    fn a_choice_lists_what_the_pick_offers() {
        let setting = full().describe().into_iter().find(|s| s.key == "sidebar").expect("sidebar");
        let offered: Vec<&str> = setting.choices.iter().map(|c| c.value.as_str()).collect();
        assert_eq!(offered, ["side_by_side", "projects_on_top", "workspaces_on_top", "tree"]);
        assert!(setting.question.is_some());
    }

    #[test]
    fn connections_say_what_is_connected() {
        let mut s = full();
        s.tokens[0].1 = Status::Env;
        let found = s.connections();
        let by = |key: &str| found.iter().find(|c| c.key == key).expect("a connection").clone();
        assert_eq!((by("shortcut").state, by("shortcut").from.as_deref()), (State::Connected, Some("env")));
        assert_eq!((by("linear").state, by("linear").removable), (State::Missing, false));
        assert_eq!((by("telegram").state, by("telegram").account.as_deref()), (State::Connected, Some("@ana_bot")));
        assert_eq!((by("telegram.chat").account.as_deref(), by("telegram.chat").removable), (Some("Ana"), true));
        assert_eq!(by("browsers").count, Some(1));
        assert_eq!(by("discord").state, State::Missing);
    }

    #[test]
    fn a_refused_bot_says_so() {
        let mut s = full();
        s.messenger_mut(Via::Telegram).refused = true;
        let found = s.connections();
        assert_eq!(found.iter().find(|c| c.key == "telegram").map(|c| c.state), Some(State::Refused));
    }
}

mod applying {
    use super::*;

    fn apply(key: &str, value: &Value) -> (Settings, Result<Option<String>, Refused>) {
        let mut s = full();
        let result = s.apply(key, value);
        (s, result)
    }

    #[test]
    fn a_switch_flips_with_the_modals_notice() {
        let (s, result) = apply("counts", &json!(false));
        assert_eq!(result, Ok(Some("rows no longer show how many they hold".into())));
        assert!(!s.config.counts);
    }

    #[test]
    fn the_account_switches_are_on_the_ui_page_and_turn_off() {
        for (key, notice) in [
            ("account_mark", "agent tabs hide their account"),
            ("account_switcher", "the account switcher is hidden"),
            ("account_picker", "starting an agent uses its chosen account"),
        ] {
            let setting = full().describe().into_iter().find(|setting| setting.key == key).expect("a setting");
            assert_eq!(
                (setting.page.as_str(), setting.kind, setting.value),
                ("UI", Kind::Switch, json!(true)),
                "{key}"
            );
            assert!(setting.note.contains("only once you add an account"), "{key}");
            let (s, result) = apply(key, &json!(false));
            assert_eq!((result, value_of(&s, key)), (Ok(Some(notice.into())), json!(false)), "{key}");
        }
    }

    #[test]
    fn the_same_value_changes_nothing() {
        let (s, result) = apply("counts", &json!(true));
        assert_eq!((result, s.config), (Ok(None), Config::default()));
    }

    #[test]
    fn a_number_is_a_whole_number() {
        let (s, result) = apply("fetch_minutes", &json!(10));
        assert_eq!((result.is_ok(), s.config.fetch_minutes), (true, 10));
        for bad in [json!("x"), json!(-1), json!(1.5)] {
            assert!(matches!(apply("fetch_minutes", &bad).1, Err(Refused::Invalid(_))));
        }
    }

    #[test]
    fn the_modals_check_refuses_a_relative_folder() {
        let (s, result) = apply("worktrees_dir", &json!("rel"));
        assert_eq!(result, Err(Refused::Invalid("use an absolute path or one starting with ~/".into())));
        assert_eq!(s.config, Config::default());
    }

    #[test]
    fn a_wrong_type_is_refused() {
        assert_eq!(apply("counts", &json!("yes")).1, Err(Refused::Invalid("`counts` takes true or false".into())));
        assert!(matches!(apply("jira_jql", &json!(3)).1, Err(Refused::Invalid(_))));
    }

    #[test]
    fn the_sound_settings_are_on_the_ui_page() {
        let keys: Vec<(String, String, Kind)> = full()
            .describe()
            .into_iter()
            .map(|s| (s.key, s.page, s.kind))
            .filter(|(k, ..)| k.starts_with("sound"))
            .collect();
        let ui = |key: &str, kind| (key.to_string(), "UI".to_string(), kind);
        assert_eq!(
            keys,
            [
                ui("sound", Kind::Switch),
                ui("sound_waiting", Kind::Text),
                ui("sound_done", Kind::Text),
                ui("sound_subagents", Kind::Switch)
            ]
        );
    }

    #[test]
    fn the_sound_settings_are_set_through_the_modal() {
        let mut s = full();
        assert!(s.apply("sound", &json!(true)).is_ok());
        assert!(s.apply("sound_subagents", &json!(true)).is_ok());
        assert!(s.apply("sound_done", &json!("/tmp/done.wav")).is_ok());
        assert!(matches!(s.apply("sound_waiting", &json!("bad\u{7}.wav")), Err(Refused::Invalid(_))));
        let c = &s.config;
        assert_eq!(
            (c.sound, c.sound_subagents, c.sound_done.as_str(), c.sound_waiting.as_str()),
            (true, true, "/tmp/done.wav", "")
        );
        assert_eq!(value_of(&s, "sound_done"), json!("/tmp/done.wav"));
    }

    #[test]
    fn an_unknown_key_is_not_found() {
        assert!(matches!(apply("nonsense", &json!(1)).1, Err(Refused::Unknown(_))));
    }

    #[test]
    fn a_token_is_not_a_setting() {
        assert!(matches!(apply("linear", &json!("x")).1, Err(Refused::Invalid(why)) if why.contains("secret")));
        assert!(matches!(apply("browsers", &json!(0)).1, Err(Refused::Invalid(_))));
    }

    #[test]
    fn a_choice_must_be_one_that_is_offered() {
        let (s, result) = apply("sidebar", &json!("tree"));
        assert_eq!((result.is_ok(), s.config.sidebar.as_str()), (true, "tree"));
        assert!(matches!(apply("sidebar", &json!("diagonal")).1, Err(Refused::Invalid(_))));
        assert!(matches!(apply("agents.claude.mode", &json!("extra arguments…")).1, Err(Refused::Invalid(_))));
    }

    #[test]
    fn the_default_agent_is_a_choice() {
        let (s, result) = apply("agent", &json!("claude"));
        assert_eq!((result.is_ok(), s.config.agent.as_str()), (true, "claude"));
    }

    #[test]
    fn tabs_are_a_list_that_keeps_one() {
        let (s, result) = apply("issue_tabs", &json!(["github", "linear"]));
        assert_eq!((result.is_ok(), s.config.issue_tabs), (true, vec!["github".to_string(), "linear".to_string()]));
        assert_eq!(apply("issue_tabs", &json!([])).1, Err(Refused::Invalid("at least one tab stays".into())));
        assert!(matches!(apply("issue_tabs", &json!(["nope"])).1, Err(Refused::Invalid(_))));
        assert!(matches!(apply("issue_tabs", &json!(["all", "all"])).1, Err(Refused::Invalid(_))));
        assert!(matches!(apply("issue_tabs", &json!("all")).1, Err(Refused::Invalid(_))));
    }

    #[test]
    fn the_prefix_is_read_as_the_capture_reads_it() {
        let (s, result) = apply("prefix_key", &json!("ctrl+]"));
        assert_eq!((result.is_ok(), s.config.prefix_key.as_str()), (true, "ctrl+]"));
        assert!(
            matches!(apply("prefix_key", &json!("ctrl+c")).1, Err(Refused::Invalid(why)) if why.contains("ctrl+c"))
        );
        let mut s = full();
        s.config.prefix_key = "ctrl+]".into();
        assert!(s.apply("prefix_key", &json!("")).is_ok());
        assert_eq!(s.config.prefix_key, "");
    }

    #[test]
    fn an_agents_mode_and_extra_arguments_change_its_arguments() {
        let (s, result) = apply("agents.claude.mode", &json!("plan"));
        assert!(result.expect("applied").is_some_and(|notice| notice.starts_with("claude starts with: ")));
        assert!(s.config.agent_args.contains_key("claude"));
        assert_eq!(value_of(&s, "agents.claude.mode"), json!("plan"));

        let (s, result) = apply("agents.codex.extra", &json!("--search"));
        assert!(result.is_ok());
        assert_eq!(value_of(&s, "agents.codex.extra"), json!("--search"));
    }

    #[test]
    fn several_changes_build_on_each_other() {
        let mut s = full();
        s.apply("counts", &json!(false)).expect("first");
        s.apply("memory", &json!(true)).expect("second");
        assert_eq!((s.config.counts, s.config.memory), (false, true));
    }
}

mod secrets {
    use super::*;

    #[test]
    fn a_token_is_handed_to_its_check() {
        let mut s = full();
        let action = s.secret("linear", &Secret(" lin_x ".into()));
        assert_eq!(action, Ok(Action::CheckToken(Source::Linear, Secret("lin_x".into()))));
    }

    #[test]
    fn a_bot_token_is_handed_to_its_check() {
        let mut s = full();
        assert_eq!(
            s.secret("discord", &Secret("abc".into())),
            Ok(Action::CheckBot(Via::Discord, Secret("abc".into())))
        );
    }

    #[test]
    fn an_empty_token_is_refused() {
        let mut s = full();
        assert_eq!(s.secret("linear", &Secret("  ".into())), Err(Refused::Invalid("paste the API key first".into())));
    }

    #[test]
    fn a_token_from_the_environment_is_blocked() {
        let mut s = full();
        s.tokens.iter_mut().for_each(|(_, status)| *status = Status::Env);
        assert!(
            matches!(s.secret("linear", &Secret("x".into())), Err(Refused::Blocked(why)) if why.contains("is set"))
        );
    }

    #[test]
    fn jira_needs_its_site_and_email_first() {
        let mut s = full();
        assert_eq!(
            s.secret("jira", &Secret("x".into())),
            Err(Refused::Blocked("set the Jira site and email first".into()))
        );
    }

    #[test]
    fn what_is_not_a_secret_is_refused() {
        let mut s = full();
        assert!(matches!(s.secret("browsers", &Secret("x".into())), Err(Refused::Invalid(_))));
        assert!(matches!(s.secret("telegram.chat", &Secret("x".into())), Err(Refused::Invalid(_))));
        assert!(matches!(s.secret("nothing", &Secret("x".into())), Err(Refused::Unknown(_))));
    }

    #[test]
    fn removing_follows_the_modals_removal() {
        let mut s = full();
        s.tokens[0].1 = Status::Saved(None);
        assert_eq!(s.removal_of("shortcut"), Ok(Some(Action::RemoveToken(Source::Shortcut))));
        assert_eq!(s.removal_of("linear"), Ok(None));
        assert_eq!(s.removal_of("telegram"), Ok(Some(Action::RemoveBot(Via::Telegram))));
        assert_eq!(s.removal_of("telegram.chat"), Ok(Some(Action::ForgetChat(Via::Telegram))));
        assert_eq!(s.removal_of("discord"), Ok(None));
        assert_eq!(s.removal_of("browsers"), Ok(Some(Action::ForgetBrowsers)));
        assert!(matches!(s.removal_of("nothing"), Err(Refused::Unknown(_))));
    }

    #[test]
    fn a_token_from_the_environment_cannot_be_removed() {
        let mut s = full();
        s.tokens[0].1 = Status::Env;
        assert!(matches!(s.removal_of("shortcut"), Err(Refused::Blocked(_))));
    }
}
