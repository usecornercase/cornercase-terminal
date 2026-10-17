use serde_json::json;

use super::*;
use crate::control::config::{ConnectionKind, ProjectEntry};

fn setting(kind: Kind) -> Setting {
    Setting { key: "x".into(), kind, ..Setting::default() }
}

mod conversion {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::true_word("true", true)]
    #[case::on("ON", true)]
    #[case::yes("yes", true)]
    #[case::false_word("false", false)]
    #[case::off("off", false)]
    #[case::zero("0", false)]
    fn a_switch_reads_the_words_people_type(#[case] text: &str, #[case] expected: bool) {
        assert_eq!(convert(&setting(Kind::Switch), text).expect("a switch"), json!(expected));
    }

    #[test]
    fn a_switch_refuses_other_words() {
        assert!(matches!(convert(&setting(Kind::Switch), "maybe"), Err(Error::WrongUsage(_))));
    }

    #[test]
    fn a_number_is_a_whole_number() {
        assert_eq!(convert(&setting(Kind::Number), " 10 ").expect("a number"), json!(10));
        assert!(convert(&setting(Kind::Number), "-1").is_err());
        assert!(convert(&setting(Kind::Number), "x").is_err());
    }

    #[test]
    fn a_list_is_comma_separated_or_json() {
        assert_eq!(convert(&setting(Kind::List), "github, linear").expect("a list"), json!(["github", "linear"]));
        assert_eq!(convert(&setting(Kind::List), r#"["jira"]"#).expect("a list"), json!(["jira"]));
        assert!(convert(&setting(Kind::List), "[oops").is_err());
    }

    #[test]
    fn text_stays_text() {
        assert_eq!(convert(&setting(Kind::Text), "  a b ").expect("text"), json!("  a b "));
        assert_eq!(convert(&setting(Kind::Choice), "tree").expect("choice"), json!("tree"));
    }
}

#[test]
fn an_odd_number_of_words_is_wrong_usage_before_the_server_is_asked() {
    let pairs = ["counts".to_string(), "false".into(), "memory".into()];
    assert!(matches!(converted(&pairs), Err(Error::WrongUsage(why)) if why.contains("KEY VALUE")));
}

#[test]
fn a_relative_folder_is_made_absolute() {
    assert_eq!(absolute("~/code").expect("a path"), PathBuf::from("~/code"));
    assert_eq!(absolute("/srv/a").expect("a path"), PathBuf::from("/srv/a"));
    assert!(absolute("web").expect("a path").is_absolute());
}

fn described() -> Described {
    Described {
        revision: 3,
        path: "/home/ana/.config/cornercase/config.json".into(),
        settings: vec![
            Setting {
                key: "counts".into(),
                page: "UI".into(),
                kind: Kind::Switch,
                value: json!(true),
                note: "what a project holds".into(),
                ..Setting::default()
            },
            Setting {
                key: "jira_jql".into(),
                page: "Issues".into(),
                kind: Kind::Text,
                value: json!(""),
                note: "JQL".into(),
                ..Setting::default()
            },
        ],
        connections: vec![Connection {
            key: "linear".into(),
            kind: ConnectionKind::Tracker,
            state: State::Connected,
            account: Some("@ana".into()),
            from: Some("saved".into()),
            ..Connection::default()
        }],
        projects: vec![ProjectEntry {
            folder: "/home/ana/web".into(),
            repository: true,
            setup: "npm ci\nnpm run build".into(),
            ..ProjectEntry::default()
        }],
        ..Described::default()
    }
}

#[test]
fn the_text_groups_settings_by_page_then_connections_and_projects() {
    let text = render(&described(), false);
    assert!(
        text.starts_with("UI\n  counts    true  what a project holds\n\nIssues\n  jira_jql  -     JQL\n"),
        "{text}"
    );
    assert!(text.contains("\nConnections\n  linear    connected as @ana (saved)\n"), "{text}");
    assert!(text.ends_with("\nProjects\n  /home/ana/web  setup: npm ci; npm run build  teardown: -\n"), "{text}");
}

#[test]
fn a_change_prints_its_notices_or_says_nothing_changed() {
    let mut got = described();
    assert_eq!(render(&got, true), "nothing changed\n");
    got.notices = vec!["sidebar: tree".into()];
    assert_eq!(render(&got, true), "sidebar: tree\n");
}

#[test]
fn a_file_that_cannot_be_read_is_said_first() {
    let mut got = described();
    got.problem = Some("expected value at line 1".into());
    assert!(render(&got, false).starts_with("/home/ana/.config/cornercase/config.json cannot be read: expected"));
}
