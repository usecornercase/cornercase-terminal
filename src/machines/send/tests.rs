use std::path::PathBuf;

use rstest::rstest;
use serde_json::{Value, json};

use super::*;
use crate::control::contract::Code;

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
        machine_boot: Some("vps-1".into()),
        ..Start::default()
    }
}

fn new_workspace() -> Send {
    let machine = Some("vps".into());
    Send::NewWorkspace(NewWorkspace {
        name: "web".into(),
        project: Some(4),
        worktree: true,
        focus: true,
        machine,
        machine_boot: Some("vps-1".into()),
        fresh: None,
    })
}

fn flow_run() -> Send {
    let machine = Some("vps".into());
    let machine_boot = Some("vps-1".into());
    Send::FlowRun(FlowRun {
        name: "review".into(),
        project: Some(4),
        focus: true,
        machine,
        machine_boot,
        ..FlowRun::default()
    })
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
        assert_eq!(sent["args"].get("machine_boot"), None);
    }

    #[rstest]
    #[case::start(Send::Start(start()), Some("vps-1"))]
    #[case::start_with(Send::StartWith(start()), Some("vps-1"))]
    #[case::new_workspace(new_workspace(), Some("vps-1"))]
    #[case::flow_run(flow_run(), Some("vps-1"))]
    #[case::without(Send::Start(Start { machine_boot: None, ..start() }), None)]
    #[case::account_list(Send::AccountList, None)]
    #[case::fold(Send::Fold { pane: 3, collapse: true }, None)]
    fn reads_the_boot_it_was_aimed_at(#[case] send: Send, #[case] boot: Option<&str>) {
        assert_eq!(send.machine_boot(), boot);
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
    #[case::collapse(
        Send::Fold { pane: 12, collapse: true },
        json!({"command": "collapse", "args": {"item": {"pane": 12}}}),
    )]
    #[case::expand(Send::Fold { pane: 12, collapse: false }, json!({"command": "expand", "args": {"item": {"pane": 12}}}))]
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
    #[case::collapse(Send::Fold { pane: 12, collapse: true }, "collapse", QUICK)]
    #[case::expand(Send::Fold { pane: 12, collapse: false }, "expand", QUICK)]
    #[case::clone(Send::Clone(clone_with(Vec::new())), "clone", CLONE)]
    fn waits_as_long_as_its_work_takes(#[case] send: Send, #[case] verb: &str, #[case] timeout: Duration) {
        assert_eq!((send.verb(), send.timeout()), (verb, timeout));
    }

    #[rstest]
    #[case::start(Send::Start(start()), true)]
    #[case::start_with(Send::StartWith(Start { focus: false, ..start() }), false)]
    #[case::new_workspace(new_workspace(), true)]
    #[case::flow_run(flow_run(), true)]
    #[case::unfocused(Send::FlowRun(FlowRun::default()), false)]
    #[case::new_tab(Send::NewTab(NewTab { focus: true, ..NewTab::default() }), true)]
    #[case::clone(Send::Clone(clone_with(Vec::new())), true)]
    #[case::accounts(Send::AccountList, false)]
    #[case::issue_sources(Send::IssueSources(4), false)]
    #[case::flows(Send::FlowList(4), false)]
    #[case::fold(Send::Fold { pane: 12, collapse: true }, false)]
    fn says_whether_the_window_follows_it(#[case] send: Send, #[case] focus: bool) {
        assert_eq!(send.focus(), focus);
    }

    #[rstest]
    #[case::start(Send::Start(start()), Some(4))]
    #[case::flow_run(flow_run(), Some(4))]
    #[case::no_project(Send::NewWorkspace(NewWorkspace::default()), None)]
    #[case::issue_sources(Send::IssueSources(5), Some(5))]
    #[case::flows(Send::FlowList(6), Some(6))]
    #[case::accounts(Send::AccountList, None)]
    #[case::clone(Send::Clone(clone_with(Vec::new())), None)]
    #[case::fold(Send::Fold { pane: 12, collapse: true }, None)]
    fn names_the_project_of_the_machine_it_works_on(#[case] send: Send, #[case] project: Option<u64>) {
        assert_eq!(send.project(), project);
    }

    #[rstest]
    #[case::start(Send::Start(start()), Some(("project", 4)))]
    #[case::fold(Send::Fold { pane: 12, collapse: false }, Some(("pane", 12)))]
    #[case::clone(Send::Clone(clone_with(Vec::new())), None)]
    fn logs_what_it_works_on_there(#[case] send: Send, #[case] about: Option<(&str, u64)>) {
        assert_eq!(send.about(), about);
    }
}

mod the_bytes {
    use super::*;

    #[rstest]
    #[case::unpadded(&["S0VZPTEK"], 6)]
    #[case::one_padding(&["S0VZPTE="], 5)]
    #[case::two_paddings(&["S0VZPQ=="], 4)]
    #[case::empty(&[""], 0)]
    #[case::summed(&["S0VZPTEK", "S0VZPQ==", ""], 10)]
    fn of_the_files_count_what_they_decode_to_not_their_base64(#[case] data: &[&str], #[case] decoded: u64) {
        let files: Vec<CloneFile> = data.iter().map(|data| env_file(data)).collect();

        assert_eq!(bytes(&files), decoded);
    }
}

mod an_answer {
    use super::*;
    use crate::control::contract::Structured;

    #[rstest]
    #[case::flows(Send::FlowList(4), "Read it.\nThen fix it.")]
    #[case::accounts(Send::AccountList, "Read it. Then fix it.")]
    fn keeps_lines_only_in_a_flow_listing(#[case] send: Send, #[case] kept: &str) {
        let text = serde_json::to_string(&Structured::Ok(json!({ "prompt": "Read it.\nThen fix it." }))).expect("json");

        let heard = (send.hearing())(&text).expect("an answer");

        assert_eq!(heard["prompt"], json!(kept));
    }
}

mod same_run {
    use super::*;

    #[rstest]
    #[case::the_same(Some("vps-1"), Some("vps-1"), None)]
    #[case::another(Some("vps-1"), Some("vps-2"), Some(Code::StaleBoot))]
    #[case::none_asked(None, Some("vps-2"), None)]
    #[case::none_reported(Some("vps-1"), None, None)]
    fn compares_what_was_asked_with_what_was_reported(
        #[case] asked: Option<&str>,
        #[case] reported: Option<&str>,
        #[case] refused: Option<Code>,
    ) {
        let result = same_run("vps", asked, reported);

        assert_eq!(result.err().map(|refusal| refusal.code), refused);
    }

    #[test]
    fn says_which_runs_and_which_machine() {
        let refusal = same_run("vps", Some("vps-1"), Some("vps-2")).expect_err("another run");

        assert!(
            refusal.message.starts_with("this request is for the cornercase server started as vps-1 on vps, "),
            "{}",
            refusal.message
        );
        assert!(refusal.message.contains("vps last reported from the one started as vps-2"));
        assert!(refusal.message.contains("its ids may name other items now"));
    }
}
