use rstest::rstest;
use serde_json::{Value, json};

use super::*;
use crate::machines::State;
use crate::test_util::fixture;

fn answer() -> MachineList {
    MachineList {
        here: ThisComputer { icon: "▭".into(), version: "0.12.225".into() },
        machines: vec![
            Info {
                name: "vps".into(),
                icon: "☁".into(),
                destination: "miguel@vps.example.com".into(),
                folder: "~/code".into(),
                state: State::Linked,
                version: Some("0.12.225".into()),
                server_version: Some("0.12.225".into()),
                seen: Some("2026-10-29T10:00:00.000Z".into()),
                error: None,
                updating: false,
                problem: None,
            },
            Info {
                name: "pi".into(),
                icon: "◫".into(),
                destination: "pi.lan".into(),
                folder: "~/code".into(),
                state: State::Unreachable,
                error: Some("ssh: connect to host pi.lan port 22: Connection refused".into()),
                ..Info::default()
            },
        ],
    }
}

mod the_request {
    use super::*;

    #[rstest]
    #[case::list(json!({"action": "list"}), Machines::List)]
    #[case::remove(json!({"action": "remove", "entry": "vps"}), Machines::Remove("vps".into()))]
    #[case::update(json!({"action": "update", "entry": "vps"}), Machines::Update("vps".into()))]
    #[case::retry_every_machine(json!({"action": "retry"}), Machines::Retry(None))]
    #[case::retry_one(json!({"action": "retry", "entry": "vps"}), Machines::Retry(Some("vps".into())))]
    fn is_tagged_by_its_action(#[case] sent: Value, #[case] read: Machines) {
        assert_eq!(serde_json::from_value::<Machines>(sent).expect("a request"), read);
    }

    #[test]
    fn set_takes_the_name_and_whatever_changes() {
        let wire = json!({"action": "set", "entry": {"name": "vps", "new_name": "box", "folder": "/srv"}});
        let expected = MachineSet {
            name: "vps".into(),
            new_name: Some("box".into()),
            folder: Some("/srv".into()),
            ..MachineSet::default()
        };
        assert_eq!(serde_json::from_value::<Machines>(wire).expect("a request"), Machines::Set(expected));
    }

    #[test]
    fn an_action_this_server_does_not_know_is_not_read() {
        assert!(serde_json::from_value::<Machines>(json!({"action": "restart", "entry": "vps"})).is_err());
    }

    #[test]
    fn its_fields_name_the_machine_and_never_the_destination() {
        let mut fields = Fields(Vec::new());
        let set = MachineSet {
            name: "vps".into(),
            destination: Some("miguel@vps.example.com".into()),
            ..MachineSet::default()
        };
        Machines::Set(set).fields(&mut fields);
        assert_eq!(
            fields.0,
            vec![("action", "set".to_string()), ("name", "vps".to_string()), ("destination", "true".to_string())]
        );
    }
}

mod the_answer {
    use super::*;

    #[test]
    fn is_written_as_its_fixture() {
        let written = serde_json::to_string_pretty(&answer()).expect("json");
        assert_eq!(written, fixture!("control/machines/answer.json").trim_end());
    }

    #[test]
    fn its_fixture_reads_back() {
        let read: MachineList = serde_json::from_str(fixture!("control/machines/answer.json")).expect("an answer");
        assert_eq!(read, answer());
    }

    #[test]
    fn a_newer_server_may_add_keys() {
        let text = r#"{"here": {"icon": "▭", "version": "9.0.0", "os": "linux"}, "machines": [{"name": "vps", "later": 1}], "more": true}"#;
        let read: MachineList = serde_json::from_str(text).expect("an answer");
        assert_eq!((read.here.version.as_str(), read.machines[0].name.as_str()), ("9.0.0", "vps"));
    }
}
