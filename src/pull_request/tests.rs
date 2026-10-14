use rstest::rstest;

use super::*;
use crate::test_util::{TempDir, fake_gh, fixture};

const VIEW: &str = fixture!("gh/2.101.0/pr-view.json");

fn gh_printing(tmp: &TempDir, json: &str) -> std::path::PathBuf {
    fake_gh(tmp.path(), &format!("echo \"$@\" > args; cat <<'EOF'\n{json}\nEOF"))
}

fn parsed(json: &str) -> PullRequest {
    serde_json::from_str::<Raw>(json).expect("gh's json").into()
}

fn check(name: &str, outcome: Outcome) -> Check {
    Check { name: name.into(), workflow: String::new(), outcome, url: String::new() }
}

fn pull(state: State, checks: Vec<Check>) -> PullRequest {
    PullRequest { number: 7, title: "x".into(), url: "u".into(), state, checks }
}

mod find {
    use super::*;

    #[test]
    fn reads_what_gh_prints_for_the_branch_checked_out() {
        let tmp = TempDir::new();

        let found = find(&gh_printing(&tmp, VIEW), tmp.path()).expect("find").expect("a pull request");

        assert_eq!((found.number, found.state), (482, State::Open));
        assert_eq!(found.title, "Returns page crashes on an empty address");
        assert_eq!(found.url, "https://github.com/acme/web-shop/pull/482");
        let args = std::fs::read_to_string(tmp.path().join("args")).expect("args");
        assert_eq!(args.trim(), format!("pr view --json {FIELDS}"));
    }

    #[test]
    fn a_branch_without_one_is_none() {
        let tmp = TempDir::new();
        let gh = fake_gh(tmp.path(), "echo 'no pull requests found for branch \"feat/x\"' >&2; exit 1");

        assert_eq!(find(&gh, tmp.path()).expect("not an error"), None);
    }

    #[test]
    fn other_failures_are_errors() {
        let tmp = TempDir::new();
        let gh = fake_gh(tmp.path(), "echo 'error connecting to api.github.com' >&2; exit 1");

        let error = find(&gh, tmp.path()).expect_err("gh failed");

        assert_eq!(explain(&error), "error connecting to api.github.com");
    }
}

mod explain {
    use super::*;

    #[test]
    fn a_missing_gh_says_to_install_it() {
        let tmp = TempDir::new();

        let error = find(&tmp.path().join("gh"), tmp.path()).expect_err("no gh");

        assert_eq!(explain(&error), "install gh, GitHub's CLI, to see them");
    }

    #[test]
    fn a_gh_that_is_logged_out_says_how_to_log_in() {
        let error = Error::Gh("To get started with GitHub CLI, please run:  gh auth login".into());

        assert_eq!(explain(&error), "gh is not logged in: run gh auth login");
    }
}

mod checks {
    use super::*;

    #[test]
    fn come_failed_first_then_running_passed_and_skipped_with_commit_statuses_after_jobs() {
        let found = parsed(VIEW);

        let listed: Vec<(&str, Outcome)> = found.checks.iter().map(|c| (c.name.as_str(), c.outcome)).collect();

        assert_eq!(
            listed,
            [
                ("tests (ubuntu-24.04)", Outcome::Failed),
                ("tests (macos-15)", Outcome::Pending),
                ("lint", Outcome::Passed),
                ("review-bot", Outcome::Passed),
                ("deploy", Outcome::Skipped),
            ]
        );
    }

    #[test]
    fn a_check_listed_twice_counts_once_as_its_latest_run() {
        let found = parsed(VIEW);

        let ubuntu: Vec<&Check> = found.checks.iter().filter(|c| c.name == "tests (ubuntu-24.04)").collect();

        assert_eq!(ubuntu.len(), 1);
        assert_eq!(ubuntu[0].url, "https://github.com/acme/web-shop/actions/runs/1001/job/2001");
    }

    #[test]
    fn a_run_that_has_not_started_yet_is_the_latest() {
        let json = r#"{"number":1,"statusCheckRollup":[
            {"__typename":"CheckRun","name":"t","workflowName":"CI","status":"COMPLETED","conclusion":"FAILURE","startedAt":"2026-10-14T13:53:09Z"},
            {"__typename":"CheckRun","name":"t","workflowName":"CI","status":"QUEUED","conclusion":"","startedAt":"0001-01-01T00:00:00Z"}]}"#;

        let found = parsed(json);

        assert_eq!(found.checks.len(), 1);
        assert_eq!(found.checks[0].outcome, Outcome::Pending);
    }

    #[rstest]
    #[case::passed("COMPLETED", "SUCCESS", Outcome::Passed)]
    #[case::neutral("COMPLETED", "NEUTRAL", Outcome::Skipped)]
    #[case::skipped("COMPLETED", "SKIPPED", Outcome::Skipped)]
    #[case::failed("COMPLETED", "FAILURE", Outcome::Failed)]
    #[case::cancelled("COMPLETED", "CANCELLED", Outcome::Failed)]
    #[case::timed_out("COMPLETED", "TIMED_OUT", Outcome::Failed)]
    #[case::action_required("COMPLETED", "ACTION_REQUIRED", Outcome::Failed)]
    #[case::startup_failure("COMPLETED", "STARTUP_FAILURE", Outcome::Failed)]
    #[case::stale("COMPLETED", "STALE", Outcome::Pending)]
    #[case::queued("QUEUED", "", Outcome::Pending)]
    #[case::in_progress("IN_PROGRESS", "", Outcome::Pending)]
    fn a_job_counts_by_its_status_and_conclusion(
        #[case] status: &str,
        #[case] conclusion: &str,
        #[case] outcome: Outcome,
    ) {
        let json = format!(
            r#"{{"number":1,"statusCheckRollup":[{{"__typename":"CheckRun","name":"t","status":"{status}","conclusion":"{conclusion}"}}]}}"#
        );

        assert_eq!(parsed(&json).checks[0].outcome, outcome);
    }

    #[rstest]
    #[case::success("SUCCESS", Outcome::Passed)]
    #[case::failure("FAILURE", Outcome::Failed)]
    #[case::error("ERROR", Outcome::Failed)]
    #[case::pending("PENDING", Outcome::Pending)]
    #[case::expected("EXPECTED", Outcome::Pending)]
    fn a_commit_status_counts_by_its_state(#[case] state: &str, #[case] outcome: Outcome) {
        let json = format!(
            r#"{{"number":1,"statusCheckRollup":[{{"__typename":"StatusContext","context":"bot","state":"{state}"}}]}}"#
        );

        assert_eq!(parsed(&json).checks[0].outcome, outcome);
    }

    #[test]
    fn missing_and_null_fields_and_unknown_kinds_are_tolerated() {
        let json =
            r#"{"number":3,"statusCheckRollup":[{"__typename":"CheckRun","name":null},{"__typename":"Something"}]}"#;

        let found = parsed(json);

        assert_eq!((found.number, found.state, found.checks.len()), (3, State::Open, 1));
        assert_eq!(found.checks[0].outcome, Outcome::Pending);
    }
}

mod state {
    use super::*;

    #[rstest]
    #[case::open("OPEN", false, State::Open)]
    #[case::draft("OPEN", true, State::Draft)]
    #[case::merged("MERGED", false, State::Merged)]
    #[case::closed("CLOSED", false, State::Closed)]
    fn comes_from_gh_state_and_draft(#[case] state: &str, #[case] draft: bool, #[case] expected: State) {
        let json = format!(r#"{{"number":1,"state":"{state}","isDraft":{draft}}}"#);

        assert_eq!(parsed(&json).state, expected);
    }
}

mod ci {
    use super::*;

    #[rstest]
    #[case::a_failure_wins(vec![check("a", Outcome::Passed), check("b", Outcome::Pending), check("c", Outcome::Failed)], Some(Ci::Failing))]
    #[case::then_a_running_check(vec![check("a", Outcome::Passed), check("b", Outcome::Pending)], Some(Ci::Pending))]
    #[case::skipped_ones_do_not_count(vec![check("a", Outcome::Passed), check("b", Outcome::Skipped)], Some(Ci::Passing))]
    #[case::only_skipped_is_nothing(vec![check("a", Outcome::Skipped)], None)]
    #[case::no_checks(Vec::new(), None)]
    fn sums_up_the_checks(#[case] checks: Vec<Check>, #[case] expected: Option<Ci>) {
        assert_eq!(pull(State::Open, checks).ci(), expected);
    }

    #[test]
    fn running_needs_an_open_pull_request_with_a_check_going() {
        let going = || vec![check("a", Outcome::Pending)];

        assert!(pull(State::Open, going()).running());
        assert!(pull(State::Draft, going()).running());
        assert!(!pull(State::Merged, going()).running());
        assert!(!pull(State::Open, vec![check("a", Outcome::Passed)]).running());
    }
}

mod texts {
    use super::*;

    #[test]
    fn the_summary_counts_each_outcome() {
        assert_eq!(parsed(VIEW).summary(), "open · 1 failed, 1 pending, 2 passed, 1 skipped");
        assert_eq!(pull(State::Merged, Vec::new()).summary(), "merged · no checks");
    }

    #[test]
    fn the_failures_name_each_failed_check_and_its_link() {
        let mut found = parsed(VIEW);
        found.checks.push(check("bot", Outcome::Failed));

        assert_eq!(
            found.failures(),
            "pull request #482 failed its checks: tests (ubuntu-24.04) \
             https://github.com/acme/web-shop/actions/runs/1001/job/2001, bot "
        );
    }
}
