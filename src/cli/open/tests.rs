use std::thread;

use serde_json::json;

use super::*;
use crate::cli::args::{Command as Parsed, Control};
use crate::cli::line::{self, Printed};
use crate::cli::tests::parse;
use crate::client;
use crate::control::Response;
use crate::error::Error;
use crate::os::socket::UnixStream;
use crate::protocol::{self, ClientMessage, ServerMessage};
use crate::test_util::os_path;

const FEATURE: &str = "open-code-workspace";

fn folder() -> Value {
    json!({"project": 1, "workspace": 2, "tab": 3, "pane": 4})
}

fn group(projects: &[u64], missing: usize, failed: usize) -> Value {
    let group = (!projects.is_empty()).then_some(5);
    json!({"group": group, "name": "team", "projects": projects, "missing": missing, "failed": failed})
}

fn open_args(words: &[&str]) -> OpenArgs {
    let path = os_path("/srv/team.code-workspace");
    let words: Vec<&str> = ["open", path.as_str()].iter().chain(words).copied().collect();
    let cli = parse(&words).expect("the arguments");
    let Some(Parsed::Control(Control::Open(args))) = cli.command else { panic!("not `open`") };
    args
}

struct Ran {
    result: Result<bool>,
    printed: Printed,
    asked: Vec<Value>,
}

fn ran_against(features: &[&str], answer: Value, words: &[&str]) -> Ran {
    let (ours, mut theirs) = UnixStream::pair().expect("a socket pair");
    let descriptor = json!({"features": features});
    let server = thread::spawn(move || {
        let mut asked = Vec::new();
        while let Ok(Some(ClientMessage::Request(text))) = protocol::recv(&mut theirs) {
            let request: Value = serde_json::from_str(&text).expect("json");
            let reply = if request["command"] == "contract" { descriptor.clone() } else { answer.clone() };
            asked.push(request);
            let reply = serde_json::to_string(&Response::Ok(reply)).expect("json");
            protocol::send(&mut theirs, &ServerMessage::Response(reply)).expect("an answer");
        }
        asked
    });
    client::share_socket(ours);
    let args = open_args(words);
    let (result, printed) = line::run(None, || run(&args).map(|()| true));
    client::unshare();
    Ran { result, printed, asked: server.join().expect("the fake server") }
}

fn commands(asked: &[Value]) -> Vec<&str> {
    asked.iter().filter_map(|request| request["command"].as_str()).collect()
}

mod the_request {
    use super::*;

    #[test]
    fn of_a_plain_open_asks_for_no_feature_and_carries_no_group() {
        let ran = ran_against(&[], folder(), &[]);

        assert_eq!(commands(&ran.asked), ["open"]);
        let mut keys: Vec<&String> = ran.asked[0]["args"].as_object().expect("arguments").keys().collect();
        keys.sort();
        assert_eq!(keys, ["focus", "path"]);
    }

    #[test]
    fn carries_a_path_this_machine_does_not_have_as_it_is() {
        let ran = ran_against(&[], folder(), &[]);

        let path = std::path::absolute(os_path("/srv/team.code-workspace")).expect("an absolute path");
        assert!(!path.exists(), "{}", path.display());
        assert_eq!(ran.asked[0]["args"]["path"], json!(path));
    }

    #[test]
    fn into_a_group_checks_the_server_first_and_names_the_group() {
        let ran = ran_against(&[FEATURE], folder(), &["--into", "5", "--focus"]);

        assert_eq!(commands(&ran.asked), ["contract", "open"]);
        assert_eq!((&ran.asked[1]["args"]["into"], &ran.asked[1]["args"]["focus"]), (&json!(5), &json!(true)));
    }

    #[test]
    fn into_a_group_never_reaches_a_server_that_would_drop_it() {
        let ran = ran_against(&[], folder(), &["--into", "5"]);

        let error = ran.result.expect_err("refused");
        assert!(error.to_string().contains("cannot open a project into a group"), "{error}");
        assert_eq!(commands(&ran.asked), ["contract"]);
    }

    #[test]
    fn takes_a_group_only_by_its_id() {
        let wrong = parse(&["open", "x", "--into", "team"]).map(drop).expect_err("not an id");

        assert_eq!(wrong.kind(), clap::error::ErrorKind::ValueValidation);
    }
}

mod a_folder {
    use super::*;

    #[test]
    fn prints_the_projects_id() {
        let ran = ran_against(&[], folder(), &[]);

        assert!(ran.result.is_ok_and(|ok| ok));
        assert_eq!((ran.printed.text.as_str(), ran.printed.stderr.as_str()), ("1\n", ""));
    }

    #[test]
    fn prints_the_answer_as_it_came_with_json() {
        let ran = ran_against(&[], folder(), &["--json"]);

        assert_eq!(ran.printed.json, Some(folder()));
    }
}

mod a_workspace_file {
    use super::*;

    #[test]
    fn prints_the_groups_id_when_every_folder_opened() {
        let ran = ran_against(&[], group(&[6, 7], 0, 0), &[]);

        assert!(ran.result.is_ok_and(|ok| ok));
        assert_eq!((ran.printed.text.as_str(), ran.printed.stderr.as_str()), ("5\n", ""));
    }

    #[test]
    fn prints_the_answer_as_it_came_with_json() {
        let ran = ran_against(&[], group(&[6, 7], 0, 0), &["--json"]);

        assert!(ran.result.is_ok_and(|ok| ok));
        assert_eq!(ran.printed.json, Some(group(&[6, 7], 0, 0)));
    }

    #[test]
    fn that_opened_in_part_prints_the_group_warns_and_is_partial() {
        let ran = ran_against(&[], group(&[6], 1, 1), &[]);

        assert!(matches!(ran.result, Err(Error::Partial)), "{:?}", ran.result);
        assert_eq!(ran.printed.text, "5\n");
        assert_eq!(
            ran.printed.stderr,
            "warning: 1 project imported into team, 1 not found, 1 could not open, see server.log\n"
        );
    }

    #[test]
    fn that_opened_in_part_is_a_whole_document_with_json() {
        let ran = ran_against(&[], group(&[6], 1, 0), &["--json"]);

        assert!(matches!(ran.result, Err(Error::Partial)), "{:?}", ran.result);
        assert_eq!((ran.printed.json, ran.printed.stderr.as_str()), (Some(group(&[6], 1, 0)), ""));
    }

    #[test]
    fn that_imported_nothing_has_no_id_to_print() {
        let ran = ran_against(&[], group(&[], 2, 0), &[]);

        assert!(matches!(ran.result, Err(Error::Partial)), "{:?}", ran.result);
        assert_eq!(ran.printed.text, "");
        assert_eq!(ran.printed.stderr, "warning: nothing imported from team, 2 not found\n");
    }
}
