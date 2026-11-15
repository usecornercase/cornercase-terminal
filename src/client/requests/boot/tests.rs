use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use rstest::rstest;
use serde_json::{Value, json};

use super::*;
use crate::control::contract::{Code, Refusal, render};
use crate::control::{Response, changes, files, ports};
use crate::os::socket::UnixStream;
use crate::protocol::{self, ClientMessage, ServerMessage};
use crate::remote::Remote;

fn boot_envelope() -> Envelope {
    Envelope { boot: Some("1-2".into()), ..Envelope::default() }
}

fn with_boot(features: &[&str]) -> impl Fn(&str) -> ServerMessage + Send + 'static {
    let mut descriptor = control::contract::describe("boot", &[]);
    descriptor.features.retain(|feature| features.contains(&feature.as_str()));
    move |text| {
        let value = if text.contains("\"contract\"") {
            serde_json::to_value(&descriptor).expect("a descriptor")
        } else {
            json!({"done": true})
        };
        ServerMessage::Response(serde_json::to_string(&Response::Ok(value)).expect("json"))
    }
}

fn legacy_refusal(name: &str) -> ServerMessage {
    let refusal = serde_json::to_string(&Response::Error(control::unknown_command(name))).expect("json");
    ServerMessage::Response(refusal)
}

fn asking(
    command: control::Command,
    envelope: Envelope,
    known: &AtomicBool,
    answer: impl Fn(&str) -> ServerMessage + Send + 'static,
) -> (Result<Value>, Vec<String>) {
    let (ours, mut theirs) = UnixStream::pair().expect("a socket pair");
    let server = thread::spawn(move || {
        let mut asked = Vec::new();
        while let Ok(Some(ClientMessage::Request(text))) = protocol::recv::<ClientMessage>(&mut theirs) {
            let reply = answer(&text);
            asked.push(text);
            protocol::send(&mut theirs, &reply).expect("an answer");
        }
        asked
    });
    let mut link = Link::socket(ours).expect("a link");
    let name = command.name();
    let answered = ask(&mut link, name, command, envelope, known);
    drop(link);
    (answered, server.join().expect("the fake server"))
}

fn ports_of_a_workspace() -> control::Command {
    control::Command::Ports(ports::Ports { workspace: Some(3), all: false })
}

fn files_of_a_workspace() -> control::Command {
    control::Command::FilesList(files::FilesList { workspace: Some(3), folders: vec![], follow: false })
}

fn a_change_of_a_workspace() -> control::Command {
    control::Command::ChangesFile(changes::ChangesFile {
        workspace: Some(3),
        path: "a.txt".into(),
        ..changes::ChangesFile::default()
    })
}

fn a_close() -> control::Command {
    control::Command::Close(control::Close {
        item: control::Item::Workspace(3),
        remove_worktree: false,
        force: false,
        unlock: false,
    })
}

fn asked_of_a_server_with_boot(command: control::Command) -> Vec<String> {
    let (answered, asked) = asking(command, boot_envelope(), &AtomicBool::new(false), with_boot(&["boot"]));
    answered.expect("an answer");
    asked
}

mod a_request_newer_than_boot {
    use super::*;

    #[rstest]
    #[case::ports(ports_of_a_workspace())]
    #[case::files_list(files_of_a_workspace())]
    #[case::changes_file(a_change_of_a_workspace())]
    fn goes_alone_and_proves_the_server_knows_boot(#[case] command: control::Command) {
        let known = AtomicBool::new(false);

        let (answered, asked) = asking(command, boot_envelope(), &known, with_boot(&["boot"]));

        answered.expect("an answer");
        assert_eq!(asked.len(), 1, "{asked:?}");
        assert!(asked[0].contains("\"boot\":\"1-2\"") && !asked[0].contains("\"contract\""), "{}", asked[0]);
        assert!(known.load(Ordering::Relaxed));
    }

    #[rstest]
    #[case::ports(ports_of_a_workspace())]
    #[case::files_list(files_of_a_workspace())]
    #[case::changes_file(a_change_of_a_workspace())]
    fn is_refused_by_name_by_a_server_without_boot(#[case] command: control::Command) {
        let known = AtomicBool::new(false);
        let name = command.name();

        let (answered, asked) = asking(command, boot_envelope(), &known, |_| legacy_refusal(name));

        let error = answered.expect_err("refused");
        assert!(
            matches!(&error, Error::IgnoresBoot(m) if m.contains("would ignore --boot and act on whatever has that id now")),
            "{error:?}"
        );
        assert_eq!(asked.len(), 1, "{asked:?}");
        assert!(!known.load(Ordering::Relaxed));
    }

    #[test]
    fn says_how_to_restart_the_server_there_when_it_is_far() {
        let remote = Remote { destination: "devbox".into(), command: None, option: "--remote-command" };

        let error = ignored(Some(&remote)).to_string();

        assert!(
            error.contains("`devbox`") && error.contains("would ignore --boot") && error.contains("restart it there"),
            "{error}"
        );
    }

    #[test]
    fn is_refused_by_a_server_older_than_requests() {
        let known = AtomicBool::new(false);

        let (answered, asked) = asking(ports_of_a_workspace(), boot_envelope(), &known, |_| {
            ServerMessage::Rejected("another build".into())
        });

        assert!(matches!(answered, Err(Error::IgnoresBoot(_))), "{answered:?}");
        assert_eq!(asked.len(), 1);
    }

    #[test]
    fn hears_a_stale_boot_and_knows_the_server_has_boot() {
        let known = AtomicBool::new(false);
        let stale = render(Err(Refusal::new(Code::StaleBoot, "it restarted")), true).expect("an answer");

        let (answered, asked) =
            asking(ports_of_a_workspace(), boot_envelope(), &known, move |_| ServerMessage::Response(stale.clone()));

        assert!(matches!(answered, Err(Error::StaleBoot(_))), "{answered:?}");
        assert_eq!(asked.len(), 1);
        assert!(known.load(Ordering::Relaxed));
    }

    #[test]
    fn hears_a_structured_refusal_as_its_own_message() {
        let known = AtomicBool::new(false);
        let refusal = render(Err(Refusal::new(Code::NotFound, "no workspace 3")), true).expect("an answer");

        let (answered, _) =
            asking(ports_of_a_workspace(), boot_envelope(), &known, move |_| ServerMessage::Response(refusal.clone()));

        assert!(matches!(&answered, Err(Error::Control(m)) if m == "no workspace 3"), "{answered:?}");
    }
}

mod a_request_a_server_without_boot_would_run {
    use super::*;

    #[rstest]
    #[case::close(a_close())]
    #[case::status(control::Command::Status(control::Status {}))]
    fn asks_the_contract_first_on_the_same_link(#[case] command: control::Command) {
        let asked = asked_of_a_server_with_boot(command);

        assert_eq!(asked.len(), 2, "{asked:?}");
        assert!(asked[0].contains("\"contract\"") && !asked[0].contains("\"boot\""), "{}", asked[0]);
        assert!(asked[1].contains("\"boot\":\"1-2\""), "{}", asked[1]);
    }

    #[test]
    fn is_never_sent_to_a_server_with_no_contract() {
        let known = AtomicBool::new(false);

        let (answered, asked) = asking(a_close(), boot_envelope(), &known, |_| legacy_refusal("contract"));

        assert!(matches!(answered, Err(Error::IgnoresBoot(_))), "{answered:?}");
        assert_eq!(asked.len(), 1, "{asked:?}");
        assert!(asked[0].contains("\"contract\""));
    }

    #[test]
    fn is_never_sent_to_a_server_whose_features_lack_boot() {
        let known = AtomicBool::new(false);

        let (answered, asked) = asking(a_close(), boot_envelope(), &known, with_boot(&["views"]));

        assert!(matches!(answered, Err(Error::IgnoresBoot(_))), "{answered:?}");
        assert_eq!(asked.len(), 1, "{asked:?}");
    }

    #[test]
    fn goes_alone_once_the_server_is_known_to_have_boot() {
        let known = AtomicBool::new(true);

        let (answered, asked) = asking(a_close(), boot_envelope(), &known, with_boot(&["views"]));

        answered.expect("an answer");
        assert_eq!(asked.len(), 1, "{asked:?}");
    }
}

#[rstest]
#[case::newer(ports_of_a_workspace())]
#[case::older(a_close())]
fn a_request_without_a_boot_is_one_request(#[case] command: control::Command) {
    let known = AtomicBool::new(false);

    let (answered, asked) = asking(command, Envelope::default(), &known, with_boot(&["views"]));

    answered.expect("an answer");
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert!(!known.load(Ordering::Relaxed));
}

#[rstest]
#[case::lacking(&["views"], true, false, true)]
#[case::listed(&["views", "boot"], true, true, false)]
#[case::nothing_pending(&["views"], false, false, false)]
fn seen_judges_a_descriptor(
    #[case] features: &[&str],
    #[case] pending: bool,
    #[case] set: bool,
    #[case] refused: bool,
) {
    let known = AtomicBool::new(false);
    let features: Vec<String> = features.iter().map(ToString::to_string).collect();

    let judged = seen(&features, pending, &known, None);

    assert_eq!(judged.is_err(), refused, "{judged:?}");
    assert_eq!(known.load(Ordering::Relaxed), set);
}

#[test]
fn the_names_before_boot_are_pinned_and_the_polled_ones_are_not_among_them() {
    assert_eq!(control::Command::NAMES[BEFORE_BOOT], "contract");
    assert_eq!(
        control::Command::NAMES[..BEFORE_BOOT].join(" "),
        "status open new-workspace new-tab split start send keys read wait close rename focus notify todo wait-several \
         events last-message restart-when-idle stats away mark queue start-with flow-list flow-run flow-export \
         flow-import search"
    );
    for name in ["contract", "ports", "files-list", "changes-file", "surface", "input"] {
        assert!(!known_before_boot(name), "{name}");
    }
}
