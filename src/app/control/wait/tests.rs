use rstest::rstest;

use super::{CANNOT_WATCH, holds_prompts};
use crate::agents::Known;
use crate::agents::activity::Status;

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
fn agents_that_queue_prompts_record_them_late(
    #[case] agent: Option<Known>,
    #[case] status: Option<Status>,
    #[case] holds: bool,
) {
    assert_eq!(holds_prompts(agent, status), holds);
}
