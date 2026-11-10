use rstest::rstest;

use super::*;
use crate::pull_request::{Check, Outcome, State};

fn pull(state: State, outcome: Outcome) -> PullRequest {
    PullRequest {
        number: 710,
        title: "show a merge sooner".into(),
        url: "https://example.com/pull/710".into(),
        state,
        conflicting: Some(false),
        base: "main".into(),
        head: Some("feat/sooner".into()),
        checks: vec![Check { name: "build".into(), workflow: "ci".into(), outcome, url: String::new() }],
        provider: Provider::Github,
        stack: None,
    }
}

fn found(pull: Option<PullRequest>, read: Instant, ran: Option<Instant>) -> Found {
    Found { branch: "feat/sooner".into(), at: Some(read), stack_at: Some(read), ran_at: ran, pull }
}

mod pace {
    use super::*;

    #[rstest]
    #[case::while_the_checks_run(State::Open, Outcome::Pending, None, RUNNING_EVERY)]
    #[case::four_minutes_after_they_ran(State::Open, Outcome::Passed, Some(360), RUNNING_EVERY)]
    #[case::five_minutes_after_they_ran(State::Open, Outcome::Passed, Some(300), OPEN_EVERY)]
    #[case::never_seen_running(State::Open, Outcome::Passed, None, OPEN_EVERY)]
    #[case::after_they_failed(State::Open, Outcome::Failed, Some(540), RUNNING_EVERY)]
    #[case::a_draft(State::Draft, Outcome::Passed, Some(540), RUNNING_EVERY)]
    #[case::merged(State::Merged, Outcome::Passed, Some(540), SETTLED_EVERY)]
    #[case::closed(State::Closed, Outcome::Passed, Some(540), SETTLED_EVERY)]
    fn follows_the_checks(
        #[case] state: State,
        #[case] outcome: Outcome,
        #[case] ran_at: Option<u64>,
        #[case] every: Duration,
    ) {
        let t0 = Instant::now();
        let now = t0 + AFTER_CHECKS * 2;
        let ran = ran_at.map(|secs| t0 + Duration::from_secs(secs));

        assert_eq!(found(Some(pull(state, outcome)), t0, ran).every(now), every);
    }

    #[test]
    fn a_branch_without_one_is_asked_every_minute() {
        let t0 = Instant::now();

        assert_eq!(found(None, t0, None).every(t0), OPEN_EVERY);
    }

    #[test]
    fn a_finished_one_is_due_20_s_after_its_read_inside_the_window() {
        let t0 = Instant::now();
        let finished = found(Some(pull(State::Open, Outcome::Passed)), t0 + Duration::from_secs(100), Some(t0));

        assert!(finished.due(t0 + Duration::from_secs(120)));
        assert!(!finished.due(t0 + Duration::from_secs(119)));
    }

    #[test]
    fn the_minute_comes_back_once_the_window_is_over() {
        let t0 = Instant::now();
        let finished = found(Some(pull(State::Open, Outcome::Passed)), t0 + Duration::from_secs(290), Some(t0));

        assert!(!finished.due(t0 + Duration::from_secs(310)));
    }
}

mod on_return {
    use super::*;

    #[rstest]
    #[case::open_read_10_s_ago(State::Open, 10, true)]
    #[case::draft_read_a_minute_ago(State::Draft, 60, true)]
    #[case::open_read_9_s_ago(State::Open, 9, false)]
    #[case::merged(State::Merged, 60, false)]
    #[case::closed(State::Closed, 60, false)]
    fn makes_open_ones_not_read_lately_due(#[case] state: State, #[case] read_ago: u64, #[case] due: bool) {
        let t0 = Instant::now();
        let mut pulls = PullRequests::default();
        pulls.found.insert(1, found(Some(pull(state, Outcome::Passed)), t0, None));

        let changed = pulls.due_on_return(t0 + Duration::from_secs(read_ago));

        assert_eq!((changed, pulls.found[&1].at.is_none()), (due, due));
    }

    #[test]
    fn leaves_a_branch_without_one() {
        let t0 = Instant::now();
        let mut pulls = PullRequests::default();
        pulls.found.insert(1, found(None, t0, None));

        assert!(!pulls.due_on_return(t0 + Duration::from_mins(1)));
        assert!(pulls.found[&1].at.is_some());
    }

    #[test]
    fn leaves_the_stack_and_a_failed_projects_wait() {
        let t0 = Instant::now();
        let now = t0 + Duration::from_mins(1);
        let mut pulls = PullRequests::default();
        pulls.found.insert(1, found(Some(pull(State::Open, Outcome::Passed)), t0, None));
        pulls.answered.insert(7, Answered { at: t0, error: Some("offline".into()), provider: Provider::Github });

        assert!(pulls.due_on_return(now));
        assert_eq!(pulls.found[&1].stack_at, Some(t0));
        assert!(pulls.waiting(7, now));
    }
}
