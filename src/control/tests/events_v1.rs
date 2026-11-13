use rstest::rstest;

use super::*;
use crate::control::Direction;
use crate::test_util::fixture;

const TIME: &str = "2026-10-16T09:30:00.123Z";

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

fn plain_samples() -> Vec<(Event, &'static str)> {
    vec![
        (
            event(
                What::Opened { kind: Kind::Tab, parent: None, accounts: BTreeMap::new() },
                Ids { pane: None, ..pane() },
            ),
            fixture!("control/events/v1/opened.json"),
        ),
        (event(What::Closed { kind: Kind::Pane }, pane()), fixture!("control/events/v1/closed.json")),
        (
            event(
                What::Status { agent: "claude".into(), from: Some("working".into()), to: Some("waiting".into()) },
                pane(),
            ),
            fixture!("control/events/v1/status.json"),
        ),
        (event(What::Exited { program: "cargo".into() }, pane()), fixture!("control/events/v1/exited.json")),
        (event(What::Dropped { count: 12 }, Ids::default()), fixture!("control/events/v1/dropped.json")),
        (event(What::Clipboard { text: "hello\nworld".into() }, pane()), fixture!("control/events/v1/clipboard.json")),
        (event(What::Bell { count: 3 }, pane()), fixture!("control/events/v1/bell.json")),
        (recap("started", "yesterday", "claude", None), fixture!("control/events/v1/recap-started.json")),
        (recap("finished", "yesterday", "claude", None), fixture!("control/events/v1/recap-finished.json")),
        (recap("failed", "week", "codex", Some("Not logged in")), fixture!("control/events/v1/recap-failed.json")),
        (tunnel(Some(5174), "up", None), fixture!("control/events/v1/tunnel.json")),
        (tunnel(None, "gone", None), fixture!("control/events/v1/tunnel-gone.json")),
        (held_tunnel(Some(5174), "up", None, Some("hold")), fixture!("control/events/v1/tunnel-hold.json")),
    ]
}

fn rich_samples() -> Vec<(Event, &'static str)> {
    vec![
        (
            event(
                notice(3, "waiting", "claude needs you in shop › default", None, None),
                Ids { pane: None, tab: None, ..pane() },
            ),
            fixture!("control/events/v1/notice.json"),
        ),
        (
            event(What::Opened { kind: Kind::Pane, parent: Some(2), accounts: BTreeMap::new() }, pane()),
            fixture!("control/events/v1/opened-parent.json"),
        ),
    ]
}

fn sound_and_accounts() -> Vec<(Event, &'static str)> {
    vec![
        (
            event(
                notice(4, "done", "claude finished in shop › default", Some("finished"), None),
                Ids { pane: None, tab: None, ..pane() },
            ),
            fixture!("control/events/v1/notice-sound.json"),
        ),
        (
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
            ),
            fixture!("control/events/v1/opened-accounts.json"),
        ),
    ]
}

fn machines() -> Vec<(Event, &'static str)> {
    let message = "claude on vps needs you in shop › default";
    vec![(
        event(notice(5, "waiting", message, None, Some("vps")), Ids::default()),
        fixture!("control/events/v1/notice-machine.json"),
    )]
}

fn subagents() -> Vec<(Event, &'static str)> {
    let mut what = notice(6, "waiting", "claude needs you in shop › default", None, None);
    if let What::Notice { subagent, .. } = &mut what {
        *subagent = true;
    }
    vec![(event(what, Ids { pane: None, tab: None, ..pane() }), fixture!("control/events/v1/notice-subagent.json"))]
}

fn messages() -> Vec<(Event, &'static str)> {
    let what = |kind: &str, message: &str, panes: Vec<u64>| What::Message {
        kind: kind.into(),
        message: message.into(),
        panes,
    };
    let merged = "pull request merged in fix-login: 2 agents closed · ▶ on the workspace reopens them";
    vec![
        (
            event(what("notify", "the release build is ready", vec![]), Ids::default()),
            fixture!("control/events/v1/message.json"),
        ),
        (
            event(
                what("flow-merged", merged, vec![4, 5]),
                Ids { project: Some(1), workspace: Some(2), ..Ids::default() },
            ),
            fixture!("control/events/v1/message-workspace.json"),
        ),
    ]
}

fn a_message(panes: Vec<u64>) -> Event {
    event(What::Message { kind: "notify".into(), message: "tests pass".into(), panes }, Ids::default())
}

fn samples() -> Vec<(Event, &'static str)> {
    [plain_samples(), rich_samples(), sound_and_accounts(), machines(), subagents(), messages()].concat()
}

fn reverse_tunnel(port: u16, local: Option<u16>, state: &str) -> Event {
    let (kind, direction) = (Some("hold".to_string()), Some(Direction::Reverse));
    event(What::Tunnel { port, local, state: state.into(), error: None, holder: 3, kind, direction }, Ids::default())
}

#[rstest]
#[case::up(reverse_tunnel(9223, Some(9222), "up"), fixture!("control/events/v1/tunnel-reverse.json"))]
#[case::gone(reverse_tunnel(9223, None, "gone"), fixture!("control/events/v1/tunnel-reverse-gone.json"))]
fn a_reverse_hold_s_event_is_written_and_read_as_its_fixture(#[case] event: Event, #[case] fixture: &str) {
    assert_eq!(serde_json::to_string(&event).expect("json"), fixture.trim_end());
    assert_eq!(serde_json::from_str::<Event>(fixture).expect("an event"), event);
}

#[test]
fn the_fixtures_are_of_the_events_version_the_contract_names() {
    assert_eq!(contract::EVENTS_VERSION, 1);
}

#[rstest]
#[case::opened(0)]
#[case::closed(1)]
#[case::status(2)]
#[case::exited(3)]
#[case::dropped(4)]
#[case::clipboard(5)]
#[case::bell(6)]
#[case::opened_from_a_pane(8)]
#[case::notice(7)]
#[case::notice_with_a_sound(9)]
#[case::opened_with_accounts(10)]
#[case::notice_from_a_machine(11)]
#[case::notice_from_a_subagent(15)]
#[case::message(16)]
#[case::message_about_a_workspace(17)]
fn an_event_is_written_as_its_fixture(#[case] index: usize) {
    let (event, fixture) = &samples()[index];

    assert_eq!(serde_json::to_string(event).expect("json"), fixture.trim_end());
}

#[rstest]
#[case::opened(0)]
#[case::closed(1)]
#[case::status(2)]
#[case::exited(3)]
#[case::dropped(4)]
#[case::clipboard(5)]
#[case::bell(6)]
#[case::opened_from_a_pane(8)]
#[case::notice(7)]
#[case::notice_with_a_sound(9)]
#[case::opened_with_accounts(10)]
#[case::notice_from_a_machine(11)]
#[case::notice_from_a_subagent(15)]
#[case::message(16)]
#[case::message_about_a_workspace(17)]
fn a_fixture_reads_back_as_its_event(#[case] index: usize) {
    let (event, fixture) = &samples()[index];

    assert_eq!(&serde_json::from_str::<Event>(fixture).expect("an event"), event);
}

#[rstest]
#[case::opened(0, false)]
#[case::status(2, false)]
#[case::dropped(4, false)]
#[case::clipboard(5, true)]
#[case::bell(6, true)]
fn only_what_programs_ask_is_a_host_event(#[case] index: usize, #[case] host: bool) {
    let (event, _) = &samples()[index];

    assert_eq!(event.host(), host);
    assert_eq!(event.kind() == Some(Kind::Pane), host || index == 2);
}

#[test]
fn a_streamed_event_is_its_fixture_under_ok() {
    let (event, fixture) = &samples()[2];

    let line: Value = serde_json::from_str(&event.streamed().expect("a line")).expect("json");

    assert_eq!(line, json!({"ok": serde_json::from_str::<Value>(fixture).expect("json")}));
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
