use rstest::rstest;
use serde_json::{Value, json};

use super::*;
use crate::control::{Report, WorkspaceInfo};
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

mod focusing {
    use super::*;
    use crate::control::{Command, Request};

    #[rstest]
    #[case::a_workspace(json!({"workspace": 4}), Item::Workspace(4))]
    #[case::a_tab(json!({"tab": 41}), Item::Tab(41))]
    fn names_the_machine_and_its_item(#[case] item: Value, #[case] read: Item) {
        let sent = json!({"command": "focus-machine", "args": {"machine": "vps", "item": item}});

        let request: Request = serde_json::from_value(sent).expect("a request");

        assert_eq!(request.command, Command::FocusMachine(FocusMachine { machine: "vps".into(), item: read }));
        assert_eq!(request.command.name(), "focus-machine");
    }

    #[test]
    fn a_request_without_its_item_is_malformed() {
        let sent = json!({"command": "focus-machine", "args": {"machine": "vps"}});

        assert!(serde_json::from_value::<Request>(sent).is_err());
    }

    #[test]
    fn its_fields_name_the_machine_and_the_item() {
        let focus = Command::FocusMachine(FocusMachine { machine: "vps".into(), item: Item::Tab(41) });

        assert_eq!(focus.fields(), [("machine", "vps".to_string()), ("item", "tab-41".to_string())]);
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

mod in_status {
    use super::*;

    fn project(id: u64, name: &str, paired: Option<u64>, repo: Option<&str>) -> ProjectInfo {
        let workspace = WorkspaceInfo {
            id: id + 1,
            name: "main".into(),
            path: format!("/home/ana/{name}").into(),
            machine: Some("vps".into()),
            ..WorkspaceInfo::default()
        };
        ProjectInfo {
            id,
            name: name.into(),
            path: format!("/home/ana/{name}").into(),
            workspaces: vec![workspace],
            repo: repo.map(str::to_string),
            machine: Some("vps".into()),
            paired: Some(paired),
            ..ProjectInfo::default()
        }
    }

    fn machines() -> Vec<MachineSeen> {
        let linked = MachineSeen {
            name: "vps".into(),
            icon: "☁".into(),
            state: State::Linked,
            seen: Some("2026-10-29T10:00:00.000Z".into()),
            boot: "4242-1760600000".into(),
            projects: vec![
                project(1, "web-shop", Some(7), Some("github.com/acme/web-shop")),
                project(3, "blog", None, None),
            ],
        };
        let fallen = MachineSeen {
            name: "pi".into(),
            icon: "◫".into(),
            state: State::Unreachable,
            seen: Some("2026-10-29T09:58:00.000Z".into()),
            boot: "77-1760500000".into(),
            projects: Vec::new(),
        };
        vec![linked, fallen]
    }

    fn shown() -> ShownRemote {
        ShownRemote {
            machine: "vps".into(),
            boot: "4242-1760600000".into(),
            project: 1,
            workspace: 2,
            tab: Some(5),
            path: "/home/ana/web-shop".into(),
        }
    }

    #[derive(Serialize)]
    struct InStatus {
        machines: Vec<MachineSeen>,
        shown_remote: ShownRemote,
    }

    #[test]
    fn the_machines_and_what_is_shown_there_are_written_as_their_fixture() {
        let written = InStatus { machines: machines(), shown_remote: shown() };
        let written = serde_json::to_string_pretty(&written).expect("json");
        assert_eq!(written, fixture!("control/machines/status.json").trim_end());
    }

    #[test]
    fn their_fixture_reads_back_into_a_report() {
        let read: Report = serde_json::from_str(fixture!("control/machines/status.json")).expect("a report");
        assert_eq!((read.machines, read.shown_remote), (machines(), Some(shown())));
    }

    #[test]
    fn a_report_without_machines_has_none_of_their_keys() {
        let project = ProjectInfo { workspaces: vec![WorkspaceInfo::default()], ..ProjectInfo::default() };
        let written = serde_json::to_value(Report { projects: vec![project], ..Report::default() }).expect("json");
        let keys = |value: &Value| value.as_object().expect("an object").keys().cloned().collect::<Vec<_>>();
        let (report, project) = (keys(&written), keys(&written["projects"][0]));
        let workspace = keys(&written["projects"][0]["workspaces"][0]);
        assert!(!report.iter().any(|key| key == "machines" || key == "shown_remote"), "{report:?}");
        assert!(!project.iter().any(|key| key == "machine" || key == "paired"), "{project:?}");
        assert!(!workspace.iter().any(|key| key == "machine"), "{workspace:?}");
    }

    #[rstest]
    #[case::paired(json!({"paired": 7}), true, Some(7))]
    #[case::unpaired(json!({"paired": null}), true, None)]
    #[case::here(json!({}), false, None)]
    fn a_project_tells_an_unpaired_one_from_one_of_its_own(
        #[case] sent: Value,
        #[case] said: bool,
        #[case] local: Option<u64>,
    ) {
        let read = serde_json::from_value::<ProjectInfo>(sent).expect("a project");
        assert_eq!((read.paired.is_some(), read.paired.flatten()), (said, local));
    }
}
