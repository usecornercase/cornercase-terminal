use serde_json::json;

use super::*;
use crate::control::{CheckInfo, Command, Request};

mod the_request {
    use super::*;

    #[test]
    fn names_its_workspace_and_is_not_chatty() {
        let request: Request =
            serde_json::from_str(r#"{"command": "pull-request", "args": {"workspace": 12}}"#).expect("a request");

        assert_eq!(request.command, Command::PullRequest(PullRequestRead { workspace: Some(12) }));
        assert_eq!(request.command.name(), "pull-request");
        assert_eq!(request.command.fields(), vec![("workspace", "12".to_string())]);
        assert!(!Command::chatty("pull-request"));
    }

    #[test]
    fn without_arguments_reads_the_workspace_here() {
        let request: Request = serde_json::from_str(r#"{"command": "pull-request", "args": {}}"#).expect("a request");

        assert_eq!(request.command, Command::PullRequest(PullRequestRead::default()));
    }
}

mod the_list_of_checks {
    use super::*;

    #[test]
    fn is_left_out_of_what_status_gives() {
        let sent = serde_json::to_value(PullRequestInfo::default()).expect("json");

        assert!(sent.get("all_checks").is_none());
    }

    #[test]
    fn is_there_even_when_empty() {
        let info = PullRequestInfo { all_checks: Some(Vec::new()), ..PullRequestInfo::default() };

        let sent = serde_json::to_value(info).expect("json");

        assert_eq!(sent["all_checks"], json!([]));
    }

    #[test]
    fn reads_as_none_from_an_older_server() {
        let older: PullRequestInfo = serde_json::from_str(r#"{"number": 482}"#).expect("an older one");

        assert_eq!(older.all_checks, None);
    }

    #[test]
    fn keeps_each_check_whole() {
        let text = r#"{"all_checks": [{"name": "lint", "workflow": "CI", "outcome": "passed", "url": "u"}]}"#;

        let read: PullRequestInfo = serde_json::from_str(text).expect("a pull request");

        let check = CheckInfo { name: "lint".into(), workflow: "CI".into(), outcome: "passed".into(), url: "u".into() };
        assert_eq!(read.all_checks, Some(vec![check]));
    }
}

#[test]
fn an_answer_without_a_pull_request_has_every_key() {
    let answer =
        PullRequestAnswer { workspace: 4, reason: Some("no pull request for feat/x".into()), ..Default::default() };

    let sent = serde_json::to_value(answer).expect("json");

    assert_eq!(
        sent,
        json!({"workspace": 4, "branch": null, "pull_request": null, "reason": "no pull request for feat/x"})
    );
}
