use rstest::rstest;

use super::*;
use crate::queue::Task;

fn said(text: &str) -> Said {
    Said { text: text.to_string(), written: Some("2026-11-12T09:41:00.000Z".into()) }
}

fn pull(number: u64) -> Pull {
    Pull { number, title: "Fix the login".into(), url: "https://example.test/1".into(), ..Pull::default() }
}

fn entry(agent: &str, state: State, result: Option<Outcome>) -> Entry {
    let task = Task { agent: agent.into(), ..Task::default() };
    Entry { result, ..Entry::new(7, task, state) }
}

fn started() -> State {
    State::Started { pane: 1, server: "s".into() }
}

#[rstest]
#[case::at_the_limit("a".repeat(4096), 4096, false)]
#[case::over_the_limit("a".repeat(4097), 4096, true)]
#[case::a_char_across_the_limit(format!("{}日", "a".repeat(4095)), 4095, true)]
fn a_text_over_four_kib_is_cut_on_a_char_boundary_and_says_so(
    #[case] text: String,
    #[case] kept: usize,
    #[case] cut: bool,
) {
    let (text, was_cut) = cap(&text);

    assert_eq!((text.len(), was_cut), (kept, cut));
}

#[rstest]
#[case::working_to_done(Some(Status::Working), Some(Status::Done), true)]
#[case::working_to_idle(Some(Status::Working), Some(Status::Idle), true)]
#[case::waiting_to_idle(Some(Status::Waiting), Some(Status::Idle), true)]
#[case::working_to_failed(Some(Status::Working), Some(Status::Failed), true)]
#[case::working_to_pending(Some(Status::Working), Some(Status::Pending), true)]
#[case::working_to_none(Some(Status::Working), None, true)]
#[case::working_to_waiting(Some(Status::Working), Some(Status::Waiting), false)]
#[case::idle_to_done(Some(Status::Idle), Some(Status::Done), false)]
#[case::none_to_working(None, Some(Status::Working), false)]
#[case::done_to_done(Some(Status::Done), Some(Status::Done), false)]
fn a_turn_ends_when_the_status_leaves_working_or_waiting(
    #[case] before: Option<Status>,
    #[case] now: Option<Status>,
    #[case] ended: bool,
) {
    assert_eq!(turn_ended(before, now), ended);
}

#[test]
fn a_capture_with_a_message_replaces_the_text_and_keeps_what_it_did_not_see() {
    let previous = Outcome {
        at: 1,
        text: "old".into(),
        branch: Some("feat/a".into()),
        pull_request: Some(pull(4)),
        ..Outcome::default()
    };
    let taken = Taken { agent: Some("claude".into()), branch: Some("feat/b".into()), ..Taken::default() };

    let outcome = Outcome::captured(Some(previous), taken, Some(said("  new  ")), 9);

    assert_eq!(outcome.text, "new");
    assert_eq!(outcome.at, 9);
    assert_eq!(outcome.written.as_deref(), Some("2026-11-12T09:41:00.000Z"));
    assert_eq!(outcome.branch.as_deref(), Some("feat/b"));
    assert_eq!(outcome.pull_request, Some(pull(4)));
}

#[test]
fn a_capture_without_a_message_keeps_the_previous_text() {
    let previous = Outcome { at: 1, text: "old".into(), cut: true, written: Some("w".into()), ..Outcome::default() };

    let outcome = Outcome::captured(Some(previous), Taken::default(), None, 5);

    assert_eq!(
        (outcome.text.as_str(), outcome.cut, outcome.written.as_deref(), outcome.at),
        ("old", true, Some("w"), 5)
    );
}

#[test]
fn an_empty_result_is_written_with_only_its_time() {
    let outcome = Outcome::captured(None, Taken::default(), None, 12);

    assert_eq!(serde_json::to_string(&outcome).expect("serialize"), r#"{"at":12}"#);
}

#[test]
fn the_closing_line_follows_the_prompt_after_a_blank_line() {
    assert_eq!(closing("fix it\n"), format!("fix it\n\n{CLOSING}"));
    assert!(!CLOSING.contains(['"', '\\', '%']));
}

#[test]
fn a_custom_agent_a_gone_record_and_a_failed_run_each_say_why() {
    let empty = Some(Outcome { at: 1, ..Outcome::default() });
    let kept = Outcome {
        at: 1,
        agent: Some("claude".into()),
        record: Some(Pointer::Claude("/a".into())),
        ..Outcome::default()
    };
    let with_text = Outcome { text: "done".into(), ..kept.clone() };
    let codex = Outcome { agent: Some("codex".into()), record: None, ..Outcome::default() };

    let says = |entry: Entry, readable| note(&entry, readable).unwrap_or_default();

    assert!(says(entry("fake", started(), empty), true).contains("keeps no record"));
    assert!(says(entry("codex", started(), Some(codex)), true).contains("was not found"));
    assert!(says(entry("claude", started(), Some(kept)), true).contains("had written no message"));
    assert!(says(entry("claude", State::Removed, Some(with_text.clone())), false).contains("no longer has"));
    assert_eq!(note(&entry("claude", State::Removed, Some(with_text)), true), None);
    assert_eq!(
        says(entry("claude", State::Failed { reason: "no room".into() }, None), true),
        "it did not start: no room"
    );
    assert_eq!(says(entry("claude", State::Missed, None), true), "it did not start: it was missed");
    assert!(says(entry("claude", started(), None), true).starts_with("no result was kept"));
}
