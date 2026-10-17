use serde_json::json;

use super::*;
use crate::test_util::fixture;

fn answer() -> Described {
    Described {
        revision: 4,
        path: PathBuf::from("/home/me/.config/cornercase/config.json"),
        problem: None,
        settings: vec![
            Setting {
                key: "counts".into(),
                page: "UI".into(),
                label: "counts".into(),
                kind: Kind::Switch,
                value: json!(true),
                default: json!(true),
                shown: "[x] shown".into(),
                note: "what a project or collapsed group holds, such as (3)".into(),
                ..Setting::default()
            },
            Setting {
                key: "sidebar".into(),
                page: "UI".into(),
                label: "sidebar".into(),
                kind: Kind::Choice,
                value: json!("tree"),
                default: json!("projects_on_top"),
                shown: "tree".into(),
                note: "how projects, workspaces and tabs are laid out".into(),
                choices: vec![
                    Choice { value: "projects_on_top".into(), note: "one column".into(), dangerous: false },
                    Choice { value: "tree".into(), note: "one list".into(), dangerous: false },
                ],
                question: Some("How should projects, workspaces and tabs be laid out?".into()),
                ..Setting::default()
            },
            Setting {
                key: "worktrees_dir".into(),
                page: "Worktrees".into(),
                label: "worktrees folder".into(),
                kind: Kind::Text,
                value: json!("~/.cornercase/worktrees"),
                default: json!("~/.cornercase/worktrees"),
                shown: "~/.cornercase/worktrees".into(),
                field: Some("worktrees folder".into()),
                ..Setting::default()
            },
        ],
        connections: vec![
            Connection {
                key: "linear".into(),
                kind: ConnectionKind::Tracker,
                page: "Issues".into(),
                section: "Accounts".into(),
                label: "Linear API key".into(),
                note: "saved".into(),
                state: State::Connected,
                account: Some("@ana".into()),
                from: Some("saved".into()),
                env: None,
                field: Some("Linear API key".into()),
                removable: true,
                ..Connection::default()
            },
            Connection {
                key: "browsers".into(),
                kind: ConnectionKind::Browsers,
                page: "Remote".into(),
                section: "Browser".into(),
                label: "paired browsers".into(),
                note: "enter forgets them all".into(),
                state: State::Missing,
                count: Some(0),
                ..Connection::default()
            },
        ],
        projects: vec![ProjectEntry {
            folder: PathBuf::from("/home/me/code/web-shop"),
            key: "~/code/web-shop".into(),
            project: Some(1),
            repository: true,
            setup: "npm ci".into(),
            teardown: String::new(),
        }],
        notices: vec!["sidebar: tree".into()],
    }
}

mod request {
    use super::*;

    #[test]
    fn each_action_round_trips() {
        let changes = [
            Configure::Get,
            Configure::Set([("counts".to_string(), json!(false)), ("sidebar".to_string(), json!("tree"))].into()),
            Configure::Project(ProjectChange {
                path: Some("~/code/web-shop".into()),
                setup: Some("npm ci".into()),
                ..ProjectChange::default()
            }),
            Configure::Secret(SecretChange { key: "linear".into(), value: Secret("lin_api_x".into()) }),
            Configure::Remove("linear".into()),
        ];
        for change in changes {
            let text = serde_json::to_string(&change).expect("serialize");
            assert_eq!(serde_json::from_str::<Configure>(&text).expect("parse"), change, "{text}");
        }
    }

    #[test]
    fn it_travels_as_a_tagged_action() {
        let set = Configure::Set([("fetch_minutes".to_string(), json!(10))].into());
        assert_eq!(
            serde_json::to_value(&set).expect("json"),
            json!({"action": "set", "entry": {"fetch_minutes": 10}})
        );
        assert_eq!(serde_json::to_value(Configure::Get).expect("json"), json!({"action": "get"}));
    }

    #[test]
    fn an_action_this_server_does_not_know_is_not_read() {
        assert!(serde_json::from_value::<Configure>(json!({"action": "reset"})).is_err());
    }

    #[test]
    fn a_secret_prints_as_dots_in_debug() {
        let change = SecretChange { key: "linear".into(), value: Secret("lin_api_x".into()) };
        assert!(!format!("{change:?}").contains("lin_api_x"));
    }

    #[test]
    fn its_fields_name_keys_and_never_values() {
        let mut fields = Fields(Vec::new());
        Configure::Set([("jira_jql".to_string(), json!("project = SECRET"))].into()).fields(&mut fields);
        assert_eq!(fields.0, vec![("action", "set".to_string()), ("keys", "jira_jql".to_string())]);
    }
}

mod answer {
    use super::*;

    #[test]
    fn it_is_written_as_its_fixture() {
        let written = serde_json::to_string_pretty(&answer()).expect("json");
        assert_eq!(written, fixture!("control/config/answer.json").trim_end());
    }

    #[test]
    fn its_fixture_reads_back() {
        let read: Described = serde_json::from_str(fixture!("control/config/answer.json")).expect("an answer");
        assert_eq!(read, answer());
    }

    #[test]
    fn a_newer_server_may_add_keys_and_kinds() {
        let text = r#"{"revision": 9, "settings": [{"key": "x", "type": "colour", "extra": 1}], "later": true}"#;
        let read: Described = serde_json::from_str(text).expect("lenient");
        assert_eq!((read.revision, read.settings[0].kind), (9, Kind::Other));
    }
}
