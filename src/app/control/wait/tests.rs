use rstest::rstest;

use super::holds_prompts;
use crate::agents::activity::Status;
use crate::agents::{CLAUDE, CODEX, GEMINI, OPENCODE};

#[rstest]
#[case::codex_at_work(Some(CODEX), Some(Status::Working), true)]
#[case::codex_asking(Some(CODEX), Some(Status::Waiting), true)]
#[case::gemini_at_work(Some(GEMINI), Some(Status::Working), true)]
#[case::gemini_asking(Some(GEMINI), Some(Status::Waiting), true)]
#[case::gemini_idle(Some(GEMINI), Some(Status::Idle), false)]
#[case::codex_idle(Some(CODEX), Some(Status::Idle), false)]
#[case::claude_at_work(Some(CLAUDE), Some(Status::Working), false)]
#[case::opencode_at_work(Some(OPENCODE), Some(Status::Working), false)]
fn agents_that_queue_prompts_record_them_late(
    #[case] agent: Option<&str>,
    #[case] status: Option<Status>,
    #[case] holds: bool,
) {
    assert_eq!(holds_prompts(agent, status), holds);
}
