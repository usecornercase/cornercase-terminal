use std::thread;

use parking_lot::Mutex;
use rstest::rstest;
use serde_json::json;

use super::*;
use crate::control::Response;
use crate::machines::ask::ask;
use crate::machines::link::fake::{Once, line, request, respond};

const BOOT: &str = "4242-1761990000";

fn sent(act: &Act) -> Value {
    let (line, mut theirs) = line("0.12.232", None);
    let connect = Once(Mutex::new(Some(line)));
    let (command, timeout) = (act.command(3), act.timeout());
    let asking = thread::spawn(move || ask(&connect, "vps", command, Some(BOOT.into()), timeout));
    let asked = request(&mut theirs);
    respond(&mut theirs, &Response::Ok(json!({})));
    asking.join().expect("the question ends").expect("an answer");
    asked
}

mod an_act {
    use super::*;

    #[rstest]
    #[case::rename(
        Act::Rename("api".into()),
        "rename",
        json!({"item": {"workspace": 3}, "name": "api"}),
        QUICK,
    )]
    #[case::check(Act::Check, "removal-check", json!({"workspace": 3}), CHECK)]
    #[case::close(
        Act::Close,
        "close",
        json!({"item": {"workspace": 3}, "remove_worktree": false, "force": false, "unlock": false}),
        QUICK,
    )]
    #[case::remove(
        Act::Remove { force: false, unlock: false },
        "close",
        json!({"item": {"workspace": 3}, "remove_worktree": true, "force": false, "unlock": false}),
        REMOVE,
    )]
    #[case::remove_anyway(
        Act::Remove { force: true, unlock: false },
        "close",
        json!({"item": {"workspace": 3}, "remove_worktree": true, "force": true, "unlock": false}),
        REMOVE,
    )]
    #[case::unlock_and_remove(
        Act::Remove { force: false, unlock: true },
        "close",
        json!({"item": {"workspace": 3}, "remove_worktree": true, "force": false, "unlock": true}),
        REMOVE,
    )]
    fn asks_for_its_workspace_with_the_boot_it_was_seen_with(
        #[case] act: Act,
        #[case] command: &str,
        #[case] args: Value,
        #[case] timeout: Duration,
    ) {
        let asked = sent(&act);

        assert_eq!(asked["boot"], BOOT, "{asked}");
        assert_eq!(asked["command"], command, "{asked}");
        assert_eq!(asked["args"], args, "{asked}");
        assert_eq!(act.timeout(), timeout);
    }

    #[rstest]
    #[case::rename(Act::Rename("api".into()), "rename")]
    #[case::check(Act::Check, "check")]
    #[case::close(Act::Close, "close")]
    #[case::remove(Act::Remove { force: true, unlock: true }, "remove")]
    fn is_named_by_its_verb(#[case] act: Act, #[case] verb: &str) {
        assert_eq!(act.verb(), verb);
    }

    #[test]
    fn waits_longest_for_a_removal_and_least_for_a_rename() {
        assert!(QUICK < CHECK && CHECK < REMOVE, "{QUICK:?} {CHECK:?} {REMOVE:?}");
        assert_eq!(REMOVE, Duration::from_secs(600));
    }
}

mod an_answer {
    use super::*;

    #[test]
    fn that_agrees_gives_its_value_cleaned() {
        let text = json!({"ok": {"workspace": 3, "path": "/srv/a\u{1b}b", "branch": "x\u{202e}y"}}).to_string();

        assert_eq!(heard(&text), Ok(json!({"workspace": 3, "path": "/srv/ab", "branch": "xy"})));
    }

    #[test]
    fn that_warns_keeps_its_whole_warning_on_one_line_without_hidden_characters() {
        let why = format!("the teardown of `qa` failed: {}", "x".repeat(300));
        let text = json!({"ok": {"warning": format!("{why}\n\u{1b}[31m"), "branch": "y".repeat(300)}}).to_string();

        let value = heard(&text).expect("a value");

        assert_eq!(value[WARNING], format!("{why} [31m"));
        assert_eq!(value["branch"].as_str().map(str::len), Some(200));
    }

    #[rstest]
    #[case::stale_boot(Code::StaleBoot, "this request is for the cornercase server started as 1-2")]
    #[case::not_found(Code::NotFound, "there is no workspace 3; `cornercase status` lists them")]
    #[case::failed(Code::Failed, "is a role's worktree whose run lives")]
    fn that_refuses_keeps_its_code(#[case] code: Code, #[case] message: &str) {
        let text = serde_json::to_string(&Structured::Error(Refusal::new(code, message))).expect("a refusal");

        assert_eq!(heard(&text), Err(Refusal::new(code, message)));
    }

    #[test]
    fn that_refuses_is_shown_on_one_line_without_hidden_characters() {
        let text = json!({"error": {"code": "failed", "message": "git said\nno\u{1b}[31m"}}).to_string();

        assert_eq!(heard(&text), Err(Refusal::new(Code::Failed, "git said no[31m")));
    }

    #[rstest]
    #[case::not_json("Connection closed")]
    #[case::unstructured(r#"{"error": "no such workspace"}"#)]
    #[case::neither(r#"{"maybe": 1}"#)]
    fn that_cannot_be_read_says_so(#[case] text: &str) {
        assert_eq!(heard(text), Err(Refusal::new(Code::Malformed, UNREADABLE)));
    }
}
