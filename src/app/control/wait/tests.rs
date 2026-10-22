use rstest::rstest;

use std::time::{Duration, Instant, SystemTime};

use super::{CANNOT_WATCH, Condition, Confirm, holds_prompts, recorded};
use crate::agents::Known;
use crate::agents::activity::{Activity, Pane, Status};

#[test]
fn a_failed_agent_has_ended_its_turn_and_an_idle_wait() {
    let mut pane = Pane::default();
    pane.update(Some(Activity::Failed), false, Instant::now());
    assert_eq!(Condition::TurnOver.ended(&pane), Some("failed"));
    assert_eq!(Condition::Idle.ended(&pane), Some("failed"));
    assert_eq!(Condition::Waiting.ended(&pane), None);
}

#[test]
fn the_wait_refusal_names_every_known_agent() {
    for agent in Known::ALL {
        assert!(CANNOT_WATCH.contains(agent.title()), "missing {}", agent.title());
    }
}

#[rstest]
#[case::codex_at_work(Some(Known::Codex), Some(Status::Working), true)]
#[case::codex_asking(Some(Known::Codex), Some(Status::Waiting), true)]
#[case::gemini_at_work(Some(Known::Gemini), Some(Status::Working), true)]
#[case::gemini_asking(Some(Known::Gemini), Some(Status::Waiting), true)]
#[case::gemini_idle(Some(Known::Gemini), Some(Status::Idle), false)]
#[case::codex_idle(Some(Known::Codex), Some(Status::Idle), false)]
#[case::claude_at_work(Some(Known::Claude), Some(Status::Working), false)]
#[case::opencode_at_work(Some(Known::Opencode), Some(Status::Working), false)]
#[case::grok_at_work(Some(Known::Grok), Some(Status::Working), true)]
#[case::grok_asking(Some(Known::Grok), Some(Status::Waiting), true)]
#[case::grok_idle(Some(Known::Grok), Some(Status::Idle), false)]
#[case::cursor_at_work(Some(Known::Cursor), Some(Status::Working), true)]
#[case::cursor_asking(Some(Known::Cursor), Some(Status::Waiting), true)]
#[case::cursor_idle(Some(Known::Cursor), Some(Status::Idle), false)]
#[case::copilot_at_work(Some(Known::Copilot), Some(Status::Working), true)]
#[case::copilot_asking(Some(Known::Copilot), Some(Status::Waiting), true)]
#[case::copilot_idle(Some(Known::Copilot), Some(Status::Idle), false)]
fn agents_that_queue_prompts_record_them_late(
    #[case] agent: Option<Known>,
    #[case] status: Option<Status>,
    #[case] holds: bool,
) {
    assert_eq!(holds_prompts(agent, status), holds);
}

#[rstest]
#[case::compaction_after_a_compact_send(true, Some(10), None, true)]
#[case::compaction_before_the_enter(true, Some(1), None, false)]
#[case::compaction_after_an_ordinary_send(false, Some(10), None, false)]
#[case::prompt_after_the_enter(false, None, Some(10), true)]
#[case::nothing(true, None, None, false)]
fn a_compaction_confirms_only_a_compact_send_made_before_it(
    #[case] compacts: bool,
    #[case] compacted: Option<u64>,
    #[case] prompted: Option<u64>,
    #[case] expected: bool,
) {
    let at = |secs: u64| SystemTime::UNIX_EPOCH + Duration::from_secs(secs);
    let confirm = Confirm { pane: 1, since: at(5), compacts, by: Instant::now(), wait: None };

    assert_eq!(recorded(&confirm, compacted.map(at), prompted.map(at)), expected);
}
