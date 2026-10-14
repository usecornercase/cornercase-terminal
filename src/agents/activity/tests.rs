use rstest::rstest;

use super::*;
use crate::agents::Known;
use crate::agents::claude::tests::session;

mod pane {
    use super::*;

    #[test]
    fn a_resumed_result_keeps_its_mark_through_reported_idle_until_seen() {
        let mut pane = Pane::default();
        pane.restore(true, false);
        let now = Instant::now();
        for step in 0..3 {
            assert_eq!(pane.update(Some(Activity::Ready), false, now + Duration::from_secs(step)), None);
        }
        assert_eq!(pane.status(), Some(Status::Done));
        pane.see();
        pane.update(Some(Activity::Ready), true, now + Duration::from_secs(3));
        assert_eq!(pane.status(), Some(Status::Idle));
    }

    fn after(steps: &[(Option<Activity>, bool)]) -> Option<Status> {
        let mut pane = Pane::default();
        let now = Instant::now();
        for (activity, seen) in steps {
            pane.update(*activity, *seen, now);
        }
        pane.status()
    }

    #[rstest]
    #[case::nothing_runs(&[(None, false)], None)]
    #[case::fresh_claude(&[(Some(Activity::Idle), false)], Some(Status::Idle))]
    #[case::working(&[(Some(Activity::Working), false)], Some(Status::Working))]
    #[case::asking(&[(Some(Activity::Working), false), (Some(Activity::Waiting), false)], Some(Status::Waiting))]
    #[case::finished_out_of_sight(&[(Some(Activity::Working), false), (Some(Activity::Idle), false)], Some(Status::Done))]
    #[case::finished_in_sight(&[(Some(Activity::Working), true), (Some(Activity::Idle), true)], Some(Status::Idle))]
    #[case::stays_done(
        &[(Some(Activity::Working), false), (Some(Activity::Idle), false), (Some(Activity::Idle), false)],
        Some(Status::Done)
    )]
    #[case::seen_later(
        &[(Some(Activity::Working), false), (Some(Activity::Idle), false), (Some(Activity::Idle), true)],
        Some(Status::Idle)
    )]
    #[case::works_again(
        &[(Some(Activity::Working), false), (Some(Activity::Idle), false), (Some(Activity::Working), false)],
        Some(Status::Working)
    )]
    #[case::answered_out_of_sight(&[(Some(Activity::Waiting), false), (Some(Activity::Idle), false)], Some(Status::Done))]
    #[case::exited(&[(Some(Activity::Working), false), (Some(Activity::Idle), false), (None, false)], None)]
    #[case::left_a_background_shell(&[(Some(Activity::Working), false), (Some(Activity::Shell), false)], Some(Status::Working))]
    #[case::its_background_shell_ended(
        &[(Some(Activity::Shell), false), (Some(Activity::Idle), false)],
        Some(Status::Done)
    )]
    fn follows_the_agent(#[case] steps: &[(Option<Activity>, bool)], #[case] expected: Option<Status>) {
        assert_eq!(after(steps), expected);
    }

    fn reacted(first: Option<Activity>, between: Option<Activity>, last: Option<Activity>) -> bool {
        let mut pane = Pane::default();
        let t0 = Instant::now();
        pane.update(first, false, t0);
        let sent = t0 + Duration::from_millis(500);
        pane.update(between, false, sent + Duration::from_millis(500));
        pane.update(last, false, sent + Duration::from_secs(1));
        pane.reacted(sent)
    }

    #[rstest]
    #[case::it_worked(Some(Activity::Idle), Some(Activity::Idle), Some(Activity::Working), true)]
    #[case::its_question_was_answered(Some(Activity::Waiting), Some(Activity::Waiting), Some(Activity::Idle), true)]
    #[case::it_stayed_idle(Some(Activity::Idle), Some(Activity::Idle), Some(Activity::Idle), false)]
    #[case::it_only_lost_and_found_its_agent(Some(Activity::Idle), None, Some(Activity::Idle), false)]
    #[case::its_background_shell_still_runs(Some(Activity::Shell), Some(Activity::Shell), Some(Activity::Shell), false)]
    #[case::it_took_the_prompt(Some(Activity::Shell), Some(Activity::Shell), Some(Activity::Working), true)]
    #[case::it_left_a_background_shell(Some(Activity::Idle), Some(Activity::Idle), Some(Activity::Shell), true)]
    fn reacting_is_working_or_changing_after_the_moment(
        #[case] first: Option<Activity>,
        #[case] between: Option<Activity>,
        #[case] last: Option<Activity>,
        #[case] expected: bool,
    ) {
        assert_eq!(reacted(first, between, last), expected);
    }

    #[rstest]
    #[case::nothing_runs(None, None)]
    #[case::working(Some(Activity::Working), Some("working"))]
    #[case::background_shell(Some(Activity::Shell), Some("shell"))]
    #[case::waiting(Some(Activity::Waiting), Some("waiting"))]
    #[case::idle(Some(Activity::Idle), Some("idle"))]
    fn tells_scripts_about_a_background_shell(#[case] activity: Option<Activity>, #[case] expected: Option<&str>) {
        let mut pane = Pane::default();
        pane.update(activity, true, Instant::now());

        assert_eq!(pane.state(), expected);
    }

    #[test]
    fn seeing_it_clears_done() {
        let mut pane = Pane::default();
        let now = Instant::now();
        pane.update(Some(Activity::Working), false, now);
        pane.update(Some(Activity::Idle), false, now);

        pane.see();

        assert_eq!(pane.status(), Some(Status::Idle));
    }
}

mod notice {
    use super::*;

    const WORKING: (Option<Activity>, bool) = (Some(Activity::Working), false);
    const WAITING: (Option<Activity>, bool) = (Some(Activity::Waiting), false);
    const IDLE: (Option<Activity>, bool) = (Some(Activity::Idle), false);
    const WAITING_IN_SIGHT: (Option<Activity>, bool) = (Some(Activity::Waiting), true);
    const TICK: Duration = Duration::from_millis(500);

    fn notices(steps: &[(Option<Activity>, bool)]) -> Vec<(usize, Status)> {
        let mut pane = Pane::default();
        let start = Instant::now();
        let mut now = start;
        let mut notices = Vec::new();
        for (i, (activity, seen)) in steps.iter().enumerate() {
            if let Some(status) = pane.update(*activity, *seen, now) {
                notices.push((i, status));
            }
            now += TICK;
        }
        notices
    }

    #[rstest]
    #[case::asks_out_of_sight(&[WORKING, WAITING, WAITING, WAITING, WAITING], &[(3, Status::Waiting)])]
    #[case::finishes_out_of_sight(&[WORKING, IDLE, IDLE, IDLE, IDLE], &[(3, Status::Done)])]
    #[case::asks_in_sight(&[WORKING, WAITING_IN_SIGHT, WAITING_IN_SIGHT, WAITING_IN_SIGHT], &[])]
    #[case::flips_straight_back(&[WORKING, WAITING, WORKING, WORKING, WORKING], &[])]
    #[case::seen_before_it_settled(&[WORKING, WAITING_IN_SIGHT, WAITING, WAITING, WAITING], &[])]
    #[case::asks_then_finishes(&[WORKING, WAITING, WAITING, WAITING, IDLE, IDLE, IDLE], &[(3, Status::Waiting), (6, Status::Done)])]
    #[case::finishes_twice(&[WORKING, IDLE, IDLE, IDLE, WORKING, IDLE, IDLE, IDLE], &[(3, Status::Done), (7, Status::Done)])]
    #[case::only_works(&[WORKING, WORKING, WORKING, IDLE], &[])]
    fn comes_once_the_change_settles(#[case] steps: &[(Option<Activity>, bool)], #[case] expected: &[(usize, Status)]) {
        assert_eq!(notices(steps), expected);
    }

    #[test]
    fn a_background_shell_finishes_once_it_ends() {
        let steps: Vec<_> = ["busy", "shell", "shell", "shell", "shell", "idle", "idle", "idle"]
            .iter()
            .map(|status| (session(&format!(r#"{{"pid":7,"status":"{status}"}}"#), 7), false))
            .collect();

        assert_eq!(notices(&steps), [(7, Status::Done)]);
    }

    #[test]
    fn another_agent_taking_over_the_pane_tells_you_again() {
        let mut pane = Pane::default();
        let start = Instant::now();
        pane.follow(Some(Known::Claude.name()));
        pane.update(Some(Activity::Waiting), false, start);
        let first = pane.update(Some(Activity::Waiting), false, start + NOTIFY_AFTER);

        pane.follow(Some(Known::Codex.name()));
        pane.update(Some(Activity::Waiting), false, start + NOTIFY_AFTER);
        let second = pane.update(Some(Activity::Waiting), false, start + NOTIFY_AFTER * 2);

        assert_eq!((first, second), (Some(Status::Waiting), Some(Status::Waiting)));
    }

    #[test]
    fn the_same_agent_keeps_its_state() {
        let mut pane = Pane::default();
        let start = Instant::now();
        pane.follow(Some(Known::Codex.name()));
        pane.update(Some(Activity::Waiting), false, start);
        pane.update(Some(Activity::Waiting), false, start + NOTIFY_AFTER);

        pane.follow(Some(Known::Codex.name()));

        assert_eq!(pane.update(Some(Activity::Waiting), false, start + NOTIFY_AFTER * 2), None);
    }

    #[test]
    fn none_comes_once_the_tab_was_seen() {
        let mut pane = Pane::default();
        let now = Instant::now();
        pane.update(Some(Activity::Working), false, now);
        pane.update(Some(Activity::Waiting), false, now);

        pane.see();

        assert_eq!(pane.update(Some(Activity::Waiting), false, now + NOTIFY_AFTER), None);
    }
}

mod spinner {
    use super::*;

    #[test]
    fn steps_once_a_tenth_of_a_second_and_wraps() {
        let frames: Vec<&str> =
            (0..12).map(|i| SPINNER[usize::from(spin_frame(SPIN_EVERY * u32::try_from(i).expect("step")))]).collect();
        assert_eq!(frames, ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "⠋", "⠙"]);
    }

    #[test]
    fn the_next_wake_lands_on_a_frame_and_is_never_immediate() {
        let waits: Vec<Duration> = (0..SPIN_EVERY.as_millis())
            .map(|ms| spin_wait(Duration::from_millis(u64::try_from(ms).expect("ms"))))
            .collect();
        assert!(waits.iter().all(|wait| !wait.is_zero() && *wait <= SPIN_EVERY));
        assert_eq!(
            (spin_wait(Duration::ZERO), spin_wait(Duration::from_millis(40)), spin_wait(SPIN_EVERY)),
            (SPIN_EVERY, Duration::from_millis(60), SPIN_EVERY)
        );
    }
}

mod rollup {
    use super::*;

    #[rstest]
    #[case::nothing(&[], None)]
    #[case::only_work(&[Some(Status::Working), Some(Status::Idle), None], None)]
    #[case::done(&[Some(Status::Working), Some(Status::Done)], Some(Status::Done))]
    #[case::waiting_wins(&[Some(Status::Done), Some(Status::Waiting), Some(Status::Working)], Some(Status::Waiting))]
    #[case::pending_over_done(&[Some(Status::Done), Some(Status::Pending)], Some(Status::Pending))]
    #[case::waiting_over_pending(&[Some(Status::Pending), Some(Status::Waiting)], Some(Status::Waiting))]
    fn keeps_what_needs_you(#[case] statuses: &[Option<Status>], #[case] expected: Option<Status>) {
        assert_eq!(attention(statuses.iter().copied()), expected);
    }
}

mod marks {
    use super::*;

    const TICK: Duration = Duration::from_millis(500);

    fn claude(activity: Activity) -> (Pane, Instant) {
        let mut pane = Pane::default();
        let now = Instant::now();
        pane.follow(Some(Known::Claude.name()));
        pane.update(Some(activity), false, now);
        (pane, now)
    }

    #[test]
    fn a_pending_mark_stays_when_the_tab_is_seen() {
        let (mut pane, now) = claude(Activity::Idle);
        pane.mark(true);

        pane.see();
        pane.update(Some(Activity::Idle), true, now + TICK);

        assert_eq!(pane.status(), Some(Status::Pending));
    }

    #[test]
    fn a_new_prompt_takes_the_mark_off() {
        let (mut pane, now) = claude(Activity::Idle);
        pane.mark(true);

        pane.update(Some(Activity::Working), false, now + TICK);
        pane.update(Some(Activity::Idle), true, now + TICK * 2);

        assert_eq!(pane.status(), Some(Status::Idle));
    }

    #[test]
    fn a_mark_put_while_working_shows_once_the_turn_is_over() {
        let (mut pane, now) = claude(Activity::Working);
        pane.mark(true);

        pane.update(Some(Activity::Working), true, now + TICK);
        assert_eq!(pane.status(), Some(Status::Working));
        pane.update(Some(Activity::Idle), true, now + TICK * 2);

        assert_eq!(pane.status(), Some(Status::Pending));
    }

    #[test]
    fn a_question_shows_over_the_mark_and_answering_it_takes_the_mark_off() {
        let (mut pane, now) = claude(Activity::Idle);
        pane.mark(true);

        pane.update(Some(Activity::Waiting), false, now + TICK);
        assert_eq!(pane.status(), Some(Status::Waiting));
        pane.update(Some(Activity::Working), false, now + TICK * 2);

        assert!(!pane.pending());
    }

    #[test]
    fn reviewing_takes_off_the_done_as_well() {
        let (mut pane, now) = claude(Activity::Working);
        pane.update(Some(Activity::Idle), false, now + TICK);
        pane.mark(true);

        pane.mark(false);

        assert_eq!(pane.status(), Some(Status::Idle));
    }

    #[test]
    fn marking_never_notifies() {
        let (mut pane, now) = claude(Activity::Working);
        pane.update(Some(Activity::Idle), false, now + TICK);
        pane.mark(true);
        pane.mark(false);
        pane.mark(true);

        let notices: Vec<_> = (2..8).filter_map(|i| pane.update(Some(Activity::Idle), false, now + TICK * i)).collect();

        assert_eq!(notices, []);
    }

    #[test]
    fn the_agent_leaving_takes_the_mark_with_it() {
        let (mut pane, now) = claude(Activity::Idle);
        pane.mark(true);

        pane.follow(None);
        pane.update(None, false, now + TICK);
        pane.follow(Some(Known::Claude.name()));
        pane.update(Some(Activity::Idle), false, now + TICK * 2);

        assert_eq!(pane.status(), Some(Status::Idle));
    }

    #[rstest]
    #[case::pending(false, true, Status::Pending)]
    #[case::done(true, false, Status::Done)]
    fn a_restored_mark_waits_for_its_agent_and_does_not_notify(
        #[case] unseen: bool,
        #[case] pending: bool,
        #[case] expected: Status,
    ) {
        let mut pane = Pane::default();
        let now = Instant::now();
        pane.restore(unseen, pending);
        pane.update(None, false, now);
        pane.update(None, false, now + TICK);

        pane.follow(Some(Known::Claude.name()));
        let notices: Vec<_> = (2..8).filter_map(|i| pane.update(Some(Activity::Idle), false, now + TICK * i)).collect();

        assert_eq!((pane.status(), notices), (Some(expected), vec![]));
    }

    #[test]
    fn a_restored_mark_goes_when_the_agent_never_comes_back() {
        let mut pane = Pane::default();
        pane.restore(true, true);

        pane.give_up();
        pane.follow(Some(Known::Claude.name()));
        pane.update(Some(Activity::Idle), false, Instant::now());

        assert_eq!(pane.status(), Some(Status::Idle));
    }
}
