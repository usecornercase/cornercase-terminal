use rstest::rstest;

use super::*;
use crate::agents::Known;
use crate::flows::builtin;

fn run(end: End) -> Run {
    let mut flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
    flow.end = end;
    Run::new(1, flow, "task".into())
}

fn pull(state: pull_request::State) -> PullRequest {
    PullRequest {
        number: 485,
        title: "fix".into(),
        url: String::new(),
        state,
        conflicting: None,
        base: "main".into(),
        checks: Vec::new(),
        provider: pull_request::Provider::Github,
    }
}

mod reached {
    use pull_request::State;

    use super::*;

    #[rstest]
    #[case::no_pull_request(End::PullRequest, None, None, false)]
    #[case::open(End::PullRequest, Some(State::Open), None, true)]
    #[case::draft(End::PullRequest, Some(State::Draft), None, true)]
    #[case::merged(End::PullRequest, Some(State::Merged), None, true)]
    #[case::closed(End::PullRequest, Some(State::Closed), None, false)]
    #[case::closed_but_marked(End::PullRequest, Some(State::Closed), Some("fix-login"), true)]
    #[case::commits_with_a_pull_request(End::Commits, Some(State::Open), None, false)]
    #[case::commits_marked(End::Commits, None, Some("fix-login"), true)]
    fn follows_the_pull_request_and_the_latch(
        #[case] end: End,
        #[case] state: Option<State>,
        #[case] ended: Option<&str>,
        #[case] expected: bool,
    ) {
        let mut run = run(end);
        run.ended = ended.map(str::to_owned);
        let found = state.map(pull);
        assert_eq!(run.reached(found.as_ref()), expected);
    }

    #[test]
    fn a_commits_flow_has_no_pull_request() {
        let found = pull(State::Open);
        assert!(run(End::Commits).pull(Some(&found)).is_none());
        assert!(run(End::PullRequest).pull(Some(&found)).is_some());
    }
}

mod latch {
    use super::*;

    #[test]
    fn an_unmarked_run_does_not_latch_and_reads_no_branch() {
        let mut run = run(End::PullRequest);
        assert!(!run.latch(false, || panic!("no branch needed")));
        assert_eq!(run.ended, None);
    }

    #[test]
    fn a_mark_latches_the_branch_once() {
        let mut run = run(End::PullRequest);
        assert!(run.latch(true, || Some("fix-login".into())));
        assert!(!run.latch(true, || Some("other".into())));
        assert_eq!(run.ended.as_deref(), Some("fix-login"));
    }

    #[test]
    fn a_mark_on_a_detached_head_waits() {
        let mut run = run(End::PullRequest);
        assert!(!run.latch(true, || None));
        assert_eq!(run.ended, None);
    }
}

mod moved_to {
    use super::*;

    #[test]
    fn another_branch_forgets_the_end() {
        let mut run = run(End::PullRequest);
        run.ended = Some("fix-login".into());
        assert_eq!(run.moved_to(Some("other")).as_deref(), Some("fix-login"));
        assert_eq!(run.ended, None);
    }

    #[test]
    fn the_same_branch_keeps_it() {
        let mut run = run(End::PullRequest);
        run.ended = Some("fix-login".into());
        assert_eq!(run.moved_to(Some("fix-login")), None);
        assert!(run.ended.is_some());
    }

    #[test]
    fn a_detached_head_keeps_it() {
        let mut run = run(End::PullRequest);
        run.ended = Some("fix-login".into());
        assert_eq!(run.moved_to(None), None);
        assert!(run.ended.is_some());
    }

    #[test]
    fn a_run_never_marked_has_nothing_to_forget() {
        let mut run = run(End::PullRequest);
        assert_eq!(run.moved_to(Some("other")), None);
    }
}

mod sent {
    use std::time::Duration;

    use super::*;

    const CODER: u64 = 14;
    const REVIEWER: u64 = 15;

    fn team() -> Run {
        let mut run = run(End::Commits);
        run.members = vec![
            Member { role: ORCHESTRATOR.into(), pane: 13 },
            Member { role: "coder".into(), pane: CODER },
            Member { role: "reviewer".into(), pane: REVIEWER },
        ];
        run
    }

    fn message(from: &str, to: &str, kind: Kind) -> Message {
        Message {
            from: from.into(),
            to: to.into(),
            kind,
            text: None,
            bytes: 0,
            at: Instant::now(),
            time: SystemTime::now(),
            seq: 0,
            edge: None,
        }
    }

    fn seen(worked: bool, ended: Option<&'static str>) -> Seen {
        Seen { worked, ended, ..Seen::default() }
    }

    #[rstest]
    #[case::not_yet_working(seen(false, Some("idle")), Some(false))]
    #[case::still_working(seen(true, None), Some(false))]
    #[case::waiting(seen(true, Some("waiting")), Some(false))]
    #[case::finished(seen(true, Some("idle")), Some(true))]
    #[case::left_a_background_shell(seen(true, Some("shell")), Some(true))]
    #[case::failed(seen(true, Some("failed")), Some(true))]
    #[case::a_wait_reports_it(Seen { covered: true, ..seen(true, Some("idle")) }, None)]
    #[case::closed(Seen { gone: true, ..seen(true, Some("idle")) }, None)]
    fn settling_follows_the_turn(#[case] seen: Seen, #[case] expected: Option<bool>) {
        let mut run = team();
        run.sent_to(CODER, Instant::now());
        run.settle(CODER, seen);
        assert_eq!(run.sent.first().map(|sent| sent.finished), expected);
    }

    #[rstest]
    #[case::after(1, None)]
    #[case::before(-1, Some(true))]
    fn the_person_typing_takes_the_turn(#[case] offset: i64, #[case] expected: Option<bool>) {
        let mut run = team();
        let base = Instant::now();
        let at = base + Duration::from_secs(10);
        run.sent_to(CODER, at);
        let typed = if offset > 0 { base + Duration::from_secs(11) } else { base + Duration::from_secs(9) };
        run.settle(CODER, Seen { typed: Some(typed), ..seen(true, Some("idle")) });
        assert_eq!(run.sent.first().map(|sent| sent.finished), expected);
    }

    #[test]
    fn sending_ignores_the_orchestrator_and_strangers_and_replaces_an_earlier_one() {
        let mut run = team();
        run.sent_to(13, Instant::now());
        run.sent_to(99, Instant::now());
        assert_eq!(run.sent.len(), 0);
        let first = Instant::now();
        run.sent_to(CODER, first);
        run.settle(CODER, seen(true, Some("idle")));
        run.sent_to(CODER, first + Duration::from_secs(1));
        assert_eq!(run.sent.len(), 1);
        assert!(!run.sent[0].finished);
    }

    #[rstest]
    #[case::wait(Kind::Wait)]
    #[case::read(Kind::Read)]
    #[case::send(Kind::Send)]
    fn hearing_from_the_orchestrator_clears_a_finished_role_only(#[case] kind: Kind) {
        let mut run = team();
        run.sent_to(CODER, Instant::now());
        run.sent_to(REVIEWER, Instant::now());
        run.settle(CODER, seen(true, Some("idle")));
        run.record(message(ORCHESTRATOR, "coder", kind));
        run.record(message(ORCHESTRATOR, "reviewer", kind));
        assert_eq!(run.sent.iter().map(|sent| sent.pane).collect::<Vec<_>>(), vec![REVIEWER]);
    }

    #[test]
    fn a_message_from_another_member_clears_nothing() {
        let mut run = team();
        run.sent_to(CODER, Instant::now());
        run.settle(CODER, seen(true, Some("idle")));
        run.record(message("reviewer", "coder", Kind::Send));
        assert_eq!(run.due(), vec![CODER]);
    }

    #[test]
    fn taking_what_is_due_keeps_the_send_order() {
        let mut run = team();
        run.sent_to(REVIEWER, Instant::now());
        run.sent_to(CODER, Instant::now());
        run.settle(CODER, seen(true, Some("idle")));
        run.settle(REVIEWER, seen(true, Some("idle")));
        assert_eq!(run.take_due(), vec![REVIEWER, CODER]);
        assert_eq!(run.sent.len(), 0);
    }
}
