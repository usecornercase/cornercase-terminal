use rstest::rstest;

use super::*;
use crate::agents::context::jsonl;
use crate::test_util::fixture;
use crate::time::Stamp;

const TURN: &str = fixture!("copilot/1.0.98/turn.jsonl");
const PERMISSION: &str = fixture!("copilot/1.0.98/permission.jsonl");
const PERMISSION_COMPLETED: &str = fixture!("copilot/1.0.98/permission-completed.jsonl");
const ASK: &str = fixture!("copilot/1.0.98/ask.jsonl");
const ASK_ANSWERED: &str = fixture!("copilot/1.0.98/ask-answered.jsonl");
const PLAN: &str = fixture!("copilot/1.0.98/plan.jsonl");
const ABORT: &str = fixture!("copilot/1.0.98/abort.jsonl");
const QUEUED: &str = fixture!("copilot/1.0.98/queued.jsonl");
const NOT_PROMPTS: &str = fixture!("copilot/1.0.98/not-prompts.jsonl");
const AUTO: &str = fixture!("copilot/1.0.98/auto.jsonl");
const COMPACTION: &str = fixture!("copilot/1.0.98/compaction.jsonl");
const TRUNCATION: &str = fixture!("copilot/1.0.98/truncation.jsonl");
const CONTEXT_CLEARED: &str = fixture!("copilot/1.0.98/context-cleared.jsonl");
const MODEL_CHANGE: &str = fixture!("copilot/1.0.98/model-change.jsonl");
const SUBAGENT: &str = fixture!("copilot/1.0.98/subagent.jsonl");
const SUBAGENT_STARTED: &str = fixture!("copilot/1.0.98/subagent-started.jsonl");
const SUBAGENT_COMPLETED: &str = fixture!("copilot/1.0.98/subagent-completed.jsonl");
const SUBAGENT_CANCELLED: &str = fixture!("copilot/1.0.98/subagent-cancelled.jsonl");
const SUBAGENT_FAILED: &str = fixture!("copilot/1.0.98/subagent-failed.jsonl");
const SUBAGENT_ACTIVE: &str = fixture!("copilot/1.0.98/subagent-active.jsonl");
const RESUME: &str = fixture!("copilot/1.0.98/resume.jsonl");
const SHELL: &str = fixture!("copilot/1.0.98/shell.jsonl");
const SHELL_DONE: &str = fixture!("copilot/1.0.98/shell-done.jsonl");

fn fold(files: &[&str]) -> Fold {
    let mut fold = Fold::default();
    for file in files {
        for event in jsonl::values(file.as_bytes()) {
            fold.apply(&event);
        }
    }
    fold
}

fn shown(fold: &Fold) -> Option<(String, Option<u16>)> {
    fold.context().map(|context| (context.model, context.percent))
}

mod waiting {
    use super::*;

    #[test]
    fn a_pending_permission_waits_and_its_completion_works_again() {
        let open = fold(&[PERMISSION]);
        let answered = fold(&[PERMISSION, PERMISSION_COMPLETED]);

        assert_eq!((open.waiting(), answered.waiting()), (true, false));
    }

    #[test]
    fn an_open_question_waits_until_it_is_answered() {
        let open = fold(&[ASK]);
        let answered = fold(&[ASK, ASK_ANSWERED]);

        assert_eq!((open.waiting(), answered.waiting()), (true, false));
    }

    #[test]
    fn a_plan_review_waits_like_a_question() {
        assert!(fold(&[PLAN]).waiting());
    }

    #[test]
    fn ctrl_c_clears_what_was_pending() {
        let mut state = fold(&[ASK]);
        let before = state.waiting();
        for event in jsonl::values(ABORT.as_bytes()) {
            state.apply(&event);
        }

        assert_eq!((before, state.waiting()), (true, false));
    }

    #[test]
    fn a_new_process_forgets_what_the_old_one_asked() {
        assert!(!fold(&[PERMISSION, RESUME]).waiting());
    }

    #[test]
    fn a_background_shell_counts_once_the_turn_is_over_and_until_it_completes() {
        let running = fold(&[SHELL]);
        let done = fold(&[SHELL, SHELL_DONE]);

        assert_eq!((running.shell_only(), done.shell_only()), (true, false));
    }

    #[test]
    fn an_open_turn_is_not_a_background_shell() {
        let turn = fold(&[SHELL]);
        let mut open = fold(&[SHELL]);
        for event in jsonl::values(
            r#"{"type":"assistant.turn_start","data":{},"timestamp":"2026-10-17T17:23:09.000Z"}"#.as_bytes(),
        ) {
            open.apply(&event);
        }

        assert_eq!((turn.shell_only(), open.shell_only()), (true, false));
    }
}

mod prompts {
    use super::*;

    #[test]
    fn a_queued_prompt_counts_at_the_time_it_was_taken() {
        let state = fold(&[QUEUED]);

        assert_eq!((state.prompted, state.counted.len()), (Stamp::parse("2026-10-17T17:23:06.000Z"), 1));
    }

    #[test]
    fn messages_that_are_not_the_users_never_count() {
        let state = fold(&[NOT_PROMPTS]);

        assert_eq!((state.prompted, state.counted.len(), state.resumable), (None, 0, false));
    }

    #[test]
    fn a_session_becomes_resumable_with_its_first_prompt() {
        let empty = fold(&[]);
        let started = fold(&[&TURN[..TURN.find("{\"type\": \"user.message\"").unwrap_or(0)]]);
        let prompted = fold(&[TURN]);

        assert_eq!((empty.resumable, started.resumable, prompted.resumable), (false, false, true));
    }

    #[test]
    fn a_prompt_counts_once_whatever_its_delivery() {
        let state = fold(&[TURN, QUEUED]);

        assert_eq!(state.counted.iter().filter(|record| record.usage == Usage::Prompt).count(), 2);
    }
}

mod context {
    use super::*;

    #[test]
    fn the_percentage_is_the_input_over_the_prompt_limit() {
        assert_eq!(shown(&fold(&[TURN])), Some(("gpt-4.1".into(), Some(19))));
    }

    #[test]
    fn auto_shows_until_a_model_is_chosen() {
        let before = &AUTO[..AUTO.find("{\"type\": \"session.auto_mode_resolved\"").unwrap_or(0)];

        assert_eq!(shown(&fold(&[before])), Some(("auto".into(), None)));
        assert_eq!(shown(&fold(&[AUTO])), Some(("gpt-4.1".into(), Some(19))));
    }

    #[test]
    fn nothing_is_shown_before_a_model_is_known() {
        assert_eq!(shown(&fold(&[])), None);
    }

    #[rstest]
    #[case::compaction(COMPACTION)]
    #[case::truncation(TRUNCATION)]
    #[case::clear(CONTEXT_CLEARED)]
    fn a_boundary_hides_the_percentage_until_the_next_main_reply(#[case] boundary: &str) {
        let after = fold(&[TURN, boundary]);
        let mut again = fold(&[TURN, boundary]);
        for event in jsonl::values(
            r#"{"type":"session.usage_record","data":{"usage":{"model":"gpt-4.1","inputTokens":32000,"maxPromptTokens":64000}},"id":"x","timestamp":"2026-10-17T17:23:20.000Z"}"#.as_bytes(),
        ) {
            again.apply(&event);
        }

        assert_eq!(
            (shown(&after), shown(&again)),
            (Some(("gpt-4.1".into(), None)), Some(("gpt-4.1".into(), Some(50))))
        );
    }

    #[test]
    fn a_compaction_is_said_until_a_boundary_or_a_main_reply_follows() {
        let usage = r#"{"type":"session.usage_record","data":{"usage":{"model":"gpt-4.1","inputTokens":32000,"maxPromptTokens":64000}},"id":"x","timestamp":"2026-10-17T17:23:20.000Z"}"#;
        let cleared_later = CONTEXT_CLEARED.replace("17:23:08", "17:23:12");
        let at = Stamp::parse("2026-10-17T17:23:08.000Z");

        assert_eq!(fold(&[TURN, COMPACTION]).compaction(), Compaction { at, hides: true });
        assert_eq!(fold(&[TURN, COMPACTION, usage]).compaction(), Compaction { at, hides: false });
        assert_eq!(fold(&[TURN, COMPACTION, &cleared_later]).compaction(), Compaction { at, hides: false });
        assert_eq!(fold(&[TURN, TRUNCATION]).compaction(), Compaction::default());
    }

    #[test]
    fn a_model_change_shows_the_new_model_without_a_percentage() {
        assert_eq!(shown(&fold(&[TURN, MODEL_CHANGE])), Some(("claude-sonnet-4.5".into(), None)));
    }

    #[test]
    fn a_failed_compaction_keeps_the_percentage() {
        let failed = r#"{"type":"session.compaction_complete","data":{"success":false},"id":"y","timestamp":"2026-10-17T17:23:08.000Z"}"#;

        assert_eq!(shown(&fold(&[TURN, failed])), Some(("gpt-4.1".into(), Some(19))));
    }

    #[test]
    fn a_restart_with_the_same_model_keeps_the_percentage() {
        assert_eq!(shown(&fold(&[TURN, RESUME])), Some(("gpt-4.1".into(), Some(19))));
    }

    #[test]
    fn the_limit_has_to_be_known() {
        let unlimited = TURN.replace("\"maxPromptTokens\": 64000", "\"maxPromptTokens\": 0");

        assert_eq!(shown(&fold(&[&unlimited])), Some(("gpt-4.1".into(), None)));
    }
}

mod tokens {
    use super::*;

    fn completed(state: &Fold) -> Vec<Tokens> {
        state
            .counted
            .iter()
            .filter_map(|record| match record.usage {
                Usage::Completed(tokens) => Some(tokens),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_input_leaves_out_what_the_cache_served() {
        assert_eq!(completed(&fold(&[TURN])), [Tokens { input: 10345, out: 42, cache_read: 2000, cache_write: 0 }]);
    }

    #[test]
    fn a_sub_agent_adds_tokens_but_never_the_percentage() {
        let state = fold(&[TURN, SUBAGENT]);

        assert_eq!((completed(&state).len(), shown(&state)), (2, Some(("gpt-4.1".into(), Some(19)))));
    }

    #[test]
    fn every_record_carries_the_event_id_so_it_counts_once() {
        let state = fold(&[TURN]);
        let ids: Vec<&str> = state.counted.iter().map(|record| record.id.as_str()).collect();

        assert!(ids.iter().all(|id| !id.is_empty()) && ids.len() == 2);
    }
}

mod subagents {
    use std::time::Duration;

    use super::*;

    fn minutes(after: u64) -> SystemTime {
        Stamp::parse("2026-10-17T17:30:00.000Z").expect("a time") + Duration::from_secs(60 * after)
    }

    #[rstest]
    #[case::completed(SUBAGENT_COMPLETED)]
    #[case::failed(SUBAGENT_FAILED)]
    #[case::cancelled(SUBAGENT_CANCELLED)]
    fn a_sub_agent_runs_until_it_completes_or_fails(#[case] end: &str) {
        let open = fold(&[SUBAGENT_STARTED]);
        let ended = fold(&[SUBAGENT_STARTED, end]);

        assert_eq!((open.subagents(minutes(1)), ended.subagents(minutes(1))), (1, 0));
    }

    #[test]
    fn a_new_process_forgets_its_sub_agents() {
        assert_eq!(fold(&[SUBAGENT_STARTED, RESUME]).subagents(minutes(1)), 0);
    }

    #[test]
    fn a_quiet_sub_agent_stops_counting_after_ten_minutes() {
        let open = fold(&[SUBAGENT_STARTED]);

        assert_eq!((open.subagents(minutes(9)), open.subagents(minutes(10))), (1, 0));
    }

    #[test]
    fn its_own_events_keep_it_fresh() {
        let open = fold(&[SUBAGENT_STARTED, SUBAGENT_ACTIVE]);

        assert_eq!((open.subagents(minutes(15)), open.subagents(minutes(19))), (1, 0));
    }
}
