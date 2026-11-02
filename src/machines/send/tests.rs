use std::path::PathBuf;

use rstest::rstest;
use serde_json::{Value, json};

use super::*;

fn start() -> Start {
    Start {
        agent: Some("claude".into()),
        project: Some(4),
        issue: Some("https://github.com/acme/api/issues/7".into()),
        wait: true,
        until: Until::Idle,
        timeout: Some(90.0),
        focus: true,
        machine: Some("vps".into()),
        ..Start::default()
    }
}

fn new_workspace() -> Send {
    let machine = Some("vps".into());
    Send::NewWorkspace(NewWorkspace { name: "web".into(), project: Some(4), worktree: true, focus: true, machine })
}

fn flow_run() -> Send {
    let machine = Some("vps".into());
    Send::FlowRun(FlowRun { name: "review".into(), project: Some(4), focus: true, machine, ..FlowRun::default() })
}

fn clone_with(files: Vec<CloneFile>) -> CloneRepository {
    CloneRepository { url: "git@github.com:acme/api.git".into(), parent: PathBuf::from("~/code"), focus: true, files }
}

fn env_file(data: &str) -> CloneFile {
    CloneFile { path: PathBuf::from(".env"), data: data.into(), mode: 0o600 }
}

fn sent(send: Send) -> Value {
    serde_json::to_value(send.command()).expect("a command")
}

mod a_send {
    use super::*;

    #[rstest]
    #[case::start(Send::Start(start()), "start")]
    #[case::new_workspace(new_workspace(), "new-workspace")]
    #[case::flow_run(flow_run(), "flow-run")]
    #[case::new_tab(Send::NewTab(NewTab { workspace: Some(9), focus: true, ..NewTab::default() }), "new-tab")]
    #[case::clone(Send::Clone(clone_with(Vec::new())), "clone")]
    fn never_asks_the_machine_to_focus_what_it_makes(#[case] send: Send, #[case] command: &str) {
        let sent = sent(send);

        assert_eq!((sent["command"].as_str(), &sent["args"]["focus"]), (Some(command), &json!(false)));
    }

    #[rstest]
    #[case::start(Send::Start(start()))]
    #[case::start_with(Send::StartWith(start()))]
    #[case::new_workspace(new_workspace())]
    #[case::flow_run(flow_run())]
    fn never_names_the_machine_to_itself(#[case] send: Send) {
        let sent = sent(send);

        assert_eq!((sent["args"].get("machine"), &sent["args"]["project"]), (None, &json!(4)));
    }

    #[test]
    fn starts_without_waiting_for_the_agent() {
        let sent = sent(Send::Start(start()));

        assert_eq!(
            (&sent["args"]["wait"], &sent["args"]["until"], &sent["args"]["timeout"]),
            (&json!(false), &json!("stops"), &Value::Null)
        );
        assert_eq!((&sent["args"]["agent"], &sent["args"]["project"]), (&json!("claude"), &json!(4)));
    }

    #[test]
    fn keeps_a_start_with_a_start_with() {
        let sent = sent(Send::StartWith(Start { model: Some("opus".into()), ..start() }));

        assert_eq!(
            (&sent["command"], &sent["args"]["model"], &sent["args"]["wait"]),
            (&json!("start-with"), &json!("opus"), &json!(false))
        );
    }

    #[test]
    fn keeps_the_files_of_a_clone() {
        let sent = sent(Send::Clone(clone_with(vec![env_file("S0VZPTEK")])));

        assert_eq!(sent["args"]["files"], json!([{"path": ".env", "data": "S0VZPTEK", "mode": 0o600}]));
    }

    #[rstest]
    #[case::accounts(Send::AccountList, json!({"command": "account-list", "args": {"check": true}}))]
    #[case::issue_sources(Send::IssueSources(4), json!({"command": "issue-sources", "args": {"project": 4}}))]
    #[case::flows(Send::FlowList(4), json!({"command": "flow-list", "args": {"project": 4}}))]
    fn asks_for_what_the_machine_has_there(#[case] send: Send, #[case] asked: Value) {
        assert_eq!(sent(send), asked);
    }

    #[rstest]
    #[case::start(Send::Start(start()), "start", WORKTREE)]
    #[case::start_with(Send::StartWith(start()), "start-with", WORKTREE)]
    #[case::flow_run(Send::FlowRun(FlowRun::default()), "flow-run", WORKTREE)]
    #[case::worktree(
        Send::NewWorkspace(NewWorkspace { worktree: true, ..NewWorkspace::default() }),
        "new-workspace",
        WORKTREE,
    )]
    #[case::plain_workspace(Send::NewWorkspace(NewWorkspace::default()), "new-workspace", QUICK)]
    #[case::new_tab(Send::NewTab(NewTab::default()), "new-tab", QUICK)]
    #[case::issue_sources(Send::IssueSources(4), "issue-sources", QUICK)]
    #[case::flows(Send::FlowList(4), "flow-list", QUICK)]
    #[case::accounts(Send::AccountList, "account-list", CHECK)]
    #[case::clone(Send::Clone(clone_with(Vec::new())), "clone", CLONE)]
    fn waits_as_long_as_its_work_takes(#[case] send: Send, #[case] verb: &str, #[case] timeout: Duration) {
        assert_eq!((send.verb(), send.timeout()), (verb, timeout));
    }

    #[rstest]
    #[case::start(Send::Start(start()), Some(4))]
    #[case::issue_sources(Send::IssueSources(5), Some(5))]
    #[case::flows(Send::FlowList(6), Some(6))]
    #[case::accounts(Send::AccountList, None)]
    #[case::clone(Send::Clone(clone_with(Vec::new())), None)]
    fn names_the_project_of_the_machine_it_works_on(#[case] send: Send, #[case] project: Option<u64>) {
        assert_eq!(send.project(), project);
    }
}

mod the_bytes {
    use super::*;

    #[test]
    fn of_the_files_count_what_they_decode_to_not_their_base64() {
        assert_eq!(bytes(&[env_file("S0VZPTEK"), env_file("")]), 6);
    }
}
