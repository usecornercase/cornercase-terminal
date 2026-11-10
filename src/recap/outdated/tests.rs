use std::collections::BTreeMap;

use rstest::rstest;

use super::*;
use crate::recap::Named;

fn recap_with(places: Vec<(&str, Place)>) -> Recap {
    Recap {
        places: places.into_iter().map(|(id, place)| (id.to_string(), place)).collect::<BTreeMap<_, _>>(),
        ..Recap::default()
    }
}

fn place(status: Option<&str>, conversation: Option<&str>, pane: u64, boot: &str) -> Place {
    Place {
        kind: "conversation".into(),
        agent: Some("claude".into()),
        conversation: conversation.map(str::to_string),
        pane: Some(pane),
        boot: Some(boot.into()),
        status: status.map(str::to_string),
        workspace: Some(Named { label: "w".into(), path: "/w".into() }),
        ..Place::default()
    }
}

fn pane(conversation: Option<&str>, id: u64, status: &str) -> PresentPane {
    PresentPane {
        agent: "claude".into(),
        conversation: conversation.map(str::to_string),
        pane: id,
        status: Some(status.into()),
    }
}

fn pull_place(checks: &str) -> Place {
    Place {
        kind: "pull".into(),
        number: Some(212),
        state: Some("open".into()),
        checks: Some(checks.into()),
        workspace: Some(Named { label: "w".into(), path: "/w".into() }),
        ..Place::default()
    }
}

fn pull(path: &str, state: &str, checks: &str) -> PullLive {
    PullLive {
        workspace: path.into(),
        number: 212,
        state: state.into(),
        checks: Some(checks.into()),
        ..PullLive::default()
    }
}

fn present(panes: Vec<PresentPane>, pulls: Vec<PullLive>, boot: &str) -> Present {
    Present { panes, pulls, boot: boot.into() }
}

fn ids(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|id| (*id).to_string()).collect()
}

#[rstest]
#[case::waiting_then_closed(place(Some("waiting"), Some("a"), 1, "b"), present(vec![], vec![], "b"), &["c1"], false)]
#[case::waiting_then_answered(
    place(Some("waiting"), Some("a"), 1, "b"),
    present(vec![pane(Some("a"), 1, "working")], vec![], "b"),
    &["c1"],
    false
)]
#[case::waiting_still(
    place(Some("waiting"), Some("a"), 1, "b"),
    present(vec![pane(Some("a"), 1, "waiting")], vec![], "b"),
    &[],
    false
)]
#[case::working_then_closed(place(Some("working"), Some("a"), 1, "b"), present(vec![], vec![], "b"), &["c1"], false)]
#[case::working_then_done(
    place(Some("working"), Some("a"), 1, "b"),
    present(vec![pane(Some("a"), 1, "done")], vec![], "b"),
    &[],
    false
)]
#[case::done_then_closed(place(Some("done"), Some("a"), 1, "b"), present(vec![], vec![], "b"), &["c1"], false)]
#[case::closed_when_written(place(None, Some("a"), 1, "b"), present(vec![], vec![], "b"), &[], false)]
#[case::known_by_its_pane_while_the_boot_is_the_same(
    place(Some("waiting"), None, 3, "b"),
    present(vec![pane(None, 3, "waiting")], vec![], "b"),
    &[],
    false
)]
#[case::another_boot(
    place(Some("waiting"), None, 3, "b"),
    present(vec![pane(None, 3, "waiting")], vec![], "c"),
    &["c1"],
    true
)]
#[case::a_new_question(
    place(None, Some("a"), 1, "b"),
    present(vec![pane(Some("z"), 2, "waiting")], vec![], "b"),
    &[],
    true
)]
fn a_conversation_place(#[case] written: Place, #[case] now: Present, #[case] changed: &[&str], #[case] new: bool) {
    assert_eq!(check(&recap_with(vec![("c1", written)]), &now), Outdated { ids: ids(changed), new });
}

#[rstest]
#[case::a_new_failure_in_a_named_workspace(vec![("c1", place(None, Some("a"), 1, "b"))], pull("/w", "open", "failing"), &[], true)]
#[case::a_failure_elsewhere(vec![("c1", place(None, Some("a"), 1, "b"))], pull("/x", "open", "failing"), &[], false)]
#[case::checks_fixed(vec![("p1", pull_place("failing"))], pull("/w", "open", "passing"), &["p1"], false)]
#[case::merged(vec![("p1", pull_place("failing"))], pull("/w", "merged", "failing"), &["p1"], false)]
#[case::still_failing(vec![("p1", pull_place("failing"))], pull("/w", "open", "failing"), &[], false)]
fn a_pull_request(
    #[case] places: Vec<(&str, Place)>,
    #[case] now: PullLive,
    #[case] changed: &[&str],
    #[case] new: bool,
) {
    let outdated = check(&recap_with(places), &present(vec![], vec![now], "b"));
    assert_eq!(outdated, Outdated { ids: ids(changed), new });
}

#[test]
fn an_empty_recap_and_nothing_open_is_current() {
    assert!(!check(&Recap::default(), &Present::default()).any());
}
