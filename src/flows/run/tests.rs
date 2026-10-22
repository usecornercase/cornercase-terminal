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
