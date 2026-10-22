use rstest::rstest;

use super::*;
use crate::test_util::{TempDir, fake_gh, fixture};

const VIEW: &str = fixture!("gh/2.101.0/pr-view.json");

const CONFLICTING: &str = fixture!("gh/2.101.0/pr-view-conflicting.json");

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
    PullRequest {
        number: 7,
        title: "x".into(),
        url: "u".into(),
        state,
        conflicting: None,
        base: String::new(),
        checks,
        provider: Provider::Github,
    }
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
            found.problems().as_deref(),
            Some(
                "pull request #482 failed its checks: tests (ubuntu-24.04) \
                 https://github.com/acme/web-shop/actions/runs/1001/job/2001, bot "
            )
        );
    }

    #[test]
    fn the_summary_says_it_conflicts_before_the_checks() {
        assert_eq!(parsed(CONFLICTING).summary(), "open · conflicts with main · 1 passed");
        let mut found = pull(State::Open, Vec::new());
        found.conflicting = Some(true);
        assert_eq!(found.summary(), "open · conflicts with its base · no checks");
    }

    #[test]
    fn the_problems_name_the_conflict_then_the_failed_checks() {
        let mut found = parsed(CONFLICTING);
        assert_eq!(found.problems().as_deref(), Some("pull request #486 has merge conflicts with main "));
        found.checks.push(check("bot", Outcome::Failed));
        assert_eq!(
            found.problems().as_deref(),
            Some("pull request #486 has merge conflicts with main and failed its checks: bot ")
        );
        assert_eq!(pull(State::Open, Vec::new()).problems(), None);
    }
}

mod conflicts {
    use super::*;

    #[rstest]
    #[case::conflicting(r#""mergeable":"CONFLICTING""#, Some(true))]
    #[case::mergeable(r#""mergeable":"MERGEABLE""#, Some(false))]
    #[case::unknown(r#""mergeable":"UNKNOWN""#, None)]
    #[case::missing(r#""title":"x""#, None)]
    #[case::null(r#""mergeable":null"#, None)]
    fn come_from_gh_mergeable(#[case] field: &str, #[case] expected: Option<bool>) {
        assert_eq!(parsed(&format!(r#"{{"number":1,{field}}}"#)).conflicting, expected);
    }

    #[test]
    fn the_fixtures_say_whether_they_conflict_and_name_their_base() {
        let (view, conflicting) = (parsed(VIEW), parsed(CONFLICTING));

        assert_eq!((view.conflicting, view.base.as_str()), (Some(false), "main"));
        assert_eq!((conflicting.conflicting, conflicting.base.as_str()), (Some(true), "main"));
        assert_eq!(conflicting.checks, [check_with("review-bot", Outcome::Passed)]);
    }

    fn check_with(name: &str, outcome: Outcome) -> Check {
        check(name, outcome)
    }

    #[rstest]
    #[case::open(State::Open, true)]
    #[case::draft(State::Draft, true)]
    #[case::merged(State::Merged, false)]
    #[case::closed(State::Closed, false)]
    fn only_an_open_or_draft_one_conflicts(#[case] state: State, #[case] expected: bool) {
        let mut found = pull(state, Vec::new());
        found.conflicting = Some(true);

        assert_eq!(found.conflicts(), expected);
    }

    #[rstest]
    #[case::unknown_keeps(None, Some(true), 7, Some(true))]
    #[case::known_replaces(Some(false), Some(true), 7, Some(false))]
    #[case::another_pull_request_starts_unknown(None, Some(true), 8, None)]
    fn an_answer_keeps_what_the_last_one_knew_only_when_it_knows_nothing(
        #[case] now: Option<bool>,
        #[case] before: Option<bool>,
        #[case] number: u64,
        #[case] expected: Option<bool>,
    ) {
        let mut last = pull(State::Open, Vec::new());
        last.conflicting = before;
        let mut answer = pull(State::Open, Vec::new());
        (answer.conflicting, answer.number) = (now, number);

        answer.keep_known(&last);

        assert_eq!(answer.conflicting, expected);
    }

    #[test]
    fn the_mark_has_it_beside_the_checks() {
        let mark = parsed(CONFLICTING).mark();

        assert_eq!((mark.conflicts, mark.ci), (true, Some(Ci::Passing)));
    }
}

mod routing {
    use super::*;

    fn listed(host: &str, ssh_host: &str) -> glab::Host {
        glab::Host { host: host.into(), ssh_host: ssh_host.into(), ..glab::Host::default() }
    }

    #[rstest]
    #[case::github("github.com", Provider::Github)]
    #[case::github_over_ssh("ssh.github.com", Provider::Github)]
    #[case::enterprise("ghe.acme.dev", Provider::Github)]
    #[case::an_ssh_alias_for_github("github.com-work", Provider::Github)]
    #[case::bitbucket("bitbucket.org", Provider::Github)]
    #[case::gitlab("gitlab.com", Provider::Gitlab)]
    #[case::gitlab_over_ssh("altssh.gitlab.com", Provider::Gitlab)]
    #[case::self_hosted("gitlab.acme.dev", Provider::Gitlab)]
    #[case::an_ssh_alias_for_gitlab("gitlab-work", Provider::Gitlab)]
    fn a_host_names_its_provider(#[case] host: &str, #[case] expected: Provider) {
        assert_eq!(provider_of(host, &[]), expected);
    }

    #[rstest]
    #[case::listed("git.acme.dev")]
    #[case::a_subdomain("ssh.git.acme.dev")]
    #[case::by_its_ssh_host("git-ssh.acme.dev")]
    fn a_host_glab_serves_is_gitlab(#[case] host: &str) {
        let hosts = [listed("git.acme.dev", "git-ssh.acme.dev")];

        assert_eq!(provider_of(host, &hosts), Provider::Gitlab);
        assert_eq!(provider_of(host, &[]), Provider::Github);
    }

    #[rstest]
    #[case("github.com", false)]
    #[case("GitHub.com", false)]
    #[case("ssh.github.com", false)]
    #[case("ghe.acme.dev", true)]
    #[case("gitlab.com", true)]
    fn only_a_host_that_is_not_github_com_asks_glab(#[case] host: &str, #[case] asks: bool) {
        assert_eq!(asks_glab(host), asks);
    }

    #[test]
    fn a_provider_says_its_words() {
        assert_eq!(Provider::of("gitlab"), Provider::Gitlab);
        assert_eq!(Provider::of(""), Provider::Github);
        assert_eq!(Provider::of(Provider::Github.id()), Provider::Github);
        assert_eq!(Provider::Gitlab.reference(486), "!486");
        assert_eq!(Provider::Github.reference(482), "#482");
        assert_eq!((Provider::Gitlab.noun(), Provider::Gitlab.checks()), ("merge request", "pipeline"));
    }
}
