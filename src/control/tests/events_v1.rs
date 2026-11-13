use std::path::Path;

use rstest::rstest;

use super::*;
use crate::control::Direction;
use crate::test_util::fixture;

const TIME: &str = "2026-10-16T09:30:00.123Z";
const FOLDER: &str = "control/events/v1";

struct Sample {
    path: &'static str,
    text: &'static str,
    event: Event,
}

macro_rules! sample {
    ($path:literal, $event:expr) => {
        Sample { path: $path, text: fixture!($path), event: $event }
    };
}

fn event(what: What, ids: Ids) -> Event {
    Event { time: TIME.into(), what, ids }
}

fn recap(state: &str, range: &str, agent: &str, error: Option<&str>) -> Event {
    let what =
        What::Recap { state: state.into(), range: range.into(), agent: agent.into(), error: error.map(str::to_string) };
    event(what, Ids::default())
}

fn tunnel(local: Option<u16>, state: &str, error: Option<&str>) -> Event {
    held_tunnel(local, state, error, None)
}

fn held_tunnel(local: Option<u16>, state: &str, error: Option<&str>, kind: Option<&str>) -> Event {
    let (error, kind) = (error.map(str::to_string), kind.map(str::to_string));
    event(
        What::Tunnel { port: 5173, local, state: state.into(), error, holder: 3, kind, direction: None },
        Ids::default(),
    )
}

fn pane() -> Ids {
    Ids { project: Some(1), workspace: Some(2), tab: Some(3), pane: Some(4) }
}

fn notice(token: u64, status: &str, message: &str, sound: Option<&str>, machine: Option<&str>) -> What {
    What::Notice {
        token: format!("cc-ab12cd-{token}"),
        agent: "claude".into(),
        status: status.into(),
        place: "shop › default".into(),
        message: message.into(),
        panes: vec![4],
        sound: sound.map(str::to_string),
        machine: machine.map(str::to_string),
        subagent: false,
    }
}

fn reverse_tunnel(port: u16, local: Option<u16>, state: &str) -> Event {
    let (kind, direction) = (Some("hold".to_string()), Some(Direction::Reverse));
    event(What::Tunnel { port, local, state: state.into(), error: None, holder: 3, kind, direction }, Ids::default())
}

fn pane_samples() -> Vec<Sample> {
    vec![
        sample!(
            "control/events/v1/opened.json",
            event(
                What::Opened { kind: Kind::Tab, parent: None, accounts: BTreeMap::new() },
                Ids { pane: None, ..pane() },
            )
        ),
        sample!("control/events/v1/closed.json", event(What::Closed { kind: Kind::Pane }, pane())),
        sample!(
            "control/events/v1/status.json",
            event(
                What::Status { agent: "claude".into(), from: Some("working".into()), to: Some("waiting".into()) },
                pane(),
            )
        ),
        sample!("control/events/v1/exited.json", event(What::Exited { program: "cargo".into() }, pane())),
        sample!("control/events/v1/dropped.json", event(What::Dropped { count: 12 }, Ids::default())),
        sample!("control/events/v1/clipboard.json", event(What::Clipboard { text: "hello\nworld".into() }, pane())),
        sample!("control/events/v1/bell.json", event(What::Bell { count: 3 }, pane())),
    ]
}

fn stream_samples() -> Vec<Sample> {
    vec![
        sample!("control/events/v1/recap-started.json", recap("started", "yesterday", "claude", None)),
        sample!("control/events/v1/recap-finished.json", recap("finished", "yesterday", "claude", None)),
        sample!("control/events/v1/recap-failed.json", recap("failed", "week", "codex", Some("Not logged in"))),
        sample!("control/events/v1/tunnel.json", tunnel(Some(5174), "up", None)),
        sample!("control/events/v1/tunnel-gone.json", tunnel(None, "gone", None)),
        sample!("control/events/v1/tunnel-hold.json", held_tunnel(Some(5174), "up", None, Some("hold"))),
        sample!("control/events/v1/tunnel-reverse.json", reverse_tunnel(9223, Some(9222), "up")),
        sample!("control/events/v1/tunnel-reverse-gone.json", reverse_tunnel(9223, None, "gone")),
    ]
}

fn rich_samples() -> Vec<Sample> {
    vec![
        sample!(
            "control/events/v1/notice.json",
            event(
                notice(3, "waiting", "claude needs you in shop › default", None, None),
                Ids { pane: None, tab: None, ..pane() },
            )
        ),
        sample!(
            "control/events/v1/opened-parent.json",
            event(What::Opened { kind: Kind::Pane, parent: Some(2), accounts: BTreeMap::new() }, pane())
        ),
    ]
}

fn sound_and_accounts() -> Vec<Sample> {
    vec![
        sample!(
            "control/events/v1/notice-sound.json",
            event(
                notice(4, "done", "claude finished in shop › default", Some("finished"), None),
                Ids { pane: None, tab: None, ..pane() },
            )
        ),
        sample!(
            "control/events/v1/opened-accounts.json",
            event(
                What::Opened {
                    kind: Kind::Pane,
                    parent: Some(2),
                    accounts: [("claude", "work"), ("codex", "personal")]
                        .into_iter()
                        .map(|(agent, account)| (agent.to_string(), account.to_string()))
                        .collect(),
                },
                pane(),
            )
        ),
    ]
}

fn machines() -> Vec<Sample> {
    let message = "claude on vps needs you in shop › default";
    vec![sample!(
        "control/events/v1/notice-machine.json",
        event(notice(5, "waiting", message, None, Some("vps")), Ids::default())
    )]
}

fn subagents() -> Vec<Sample> {
    let mut what = notice(6, "waiting", "claude needs you in shop › default", None, None);
    if let What::Notice { subagent, .. } = &mut what {
        *subagent = true;
    }
    vec![sample!("control/events/v1/notice-subagent.json", event(what, Ids { pane: None, tab: None, ..pane() }))]
}

fn messages() -> Vec<Sample> {
    let what = |kind: &str, message: &str, panes: Vec<u64>| What::Message {
        kind: kind.into(),
        message: message.into(),
        panes,
    };
    let merged = "pull request merged in fix-login: 2 agents closed · ▶ on the workspace reopens them";
    vec![
        sample!(
            "control/events/v1/message.json",
            event(what("notify", "the release build is ready", vec![]), Ids::default())
        ),
        sample!(
            "control/events/v1/message-workspace.json",
            event(
                what("flow-merged", merged, vec![4, 5]),
                Ids { project: Some(1), workspace: Some(2), ..Ids::default() },
            )
        ),
    ]
}

fn a_message(panes: Vec<u64>) -> Event {
    event(What::Message { kind: "notify".into(), message: "tests pass".into(), panes }, Ids::default())
}

fn samples() -> Vec<Sample> {
    [pane_samples(), stream_samples(), rich_samples(), sound_and_accounts(), machines(), subagents(), messages()]
        .into_iter()
        .flatten()
        .collect()
}

fn sample(name: &str) -> Sample {
    let path = format!("{FOLDER}/{name}.json");
    samples().into_iter().find(|sample| sample.path == path).unwrap_or_else(|| panic!("no sample of {path}"))
}

#[test]
fn the_fixtures_are_of_the_events_version_the_contract_names() {
    assert_eq!(contract::EVENTS_VERSION, 1);
}

#[test]
fn every_event_is_written_as_its_fixture() {
    for Sample { path, text, event } in samples() {
        assert_eq!(serde_json::to_string(&event).expect("json"), text.trim_end(), "{path}");
    }
}

#[test]
fn every_fixture_reads_back_as_its_event() {
    for Sample { path, text, event } in samples() {
        assert_eq!(serde_json::from_str::<Event>(text).expect(path), event, "{path}");
    }
}

#[test]
fn every_fixture_in_the_folder_is_named_by_a_sample() {
    let named: Vec<&str> = samples().iter().map(|sample| sample.path).collect();
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(FOLDER);
    for file in std::fs::read_dir(folder).expect("the fixtures") {
        let path = format!("{FOLDER}/{}", file.expect("a fixture").file_name().to_string_lossy());
        assert!(named.contains(&path.as_str()), "{path} is checked by no sample");
    }
}

#[rstest]
#[case::opened("opened", false, false)]
#[case::status("status", false, true)]
#[case::dropped("dropped", false, false)]
#[case::clipboard("clipboard", true, true)]
#[case::bell("bell", true, true)]
fn only_what_programs_ask_is_a_host_event(#[case] name: &str, #[case] host: bool, #[case] of_a_pane: bool) {
    let event = sample(name).event;

    assert_eq!(event.host(), host);
    assert_eq!(event.kind() == Some(Kind::Pane), of_a_pane);
}

#[test]
fn a_streamed_event_is_its_fixture_under_ok() {
    let status = sample("status");

    let line: Value = serde_json::from_str(&status.event.streamed().expect("a line")).expect("json");

    assert_eq!(line, json!({"ok": serde_json::from_str::<Value>(status.text).expect("json")}));
}

#[rstest]
#[case::named_pane_reader(vec![4], vec![4], true)]
#[case::another_pane_reader(vec![4], vec![9], false)]
#[case::no_pane_named(vec![], vec![4], false)]
fn a_message_reaches_readers_of_notices_and_the_pane_readers_it_names(
    #[case] named: Vec<u64>,
    #[case] reader: Vec<u64>,
    #[case] about: bool,
) {
    let event = a_message(named);

    assert!(event.notice());
    assert!(!event.host());
    assert_eq!(event.about(&reader), about);
}
