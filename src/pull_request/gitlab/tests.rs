use std::path::PathBuf;

use rstest::rstest;
use serde_json::Value;

use super::*;
use crate::test_util::{TempDir, fake_glab, fixture};

const VIEW: &str = fixture!("glab/1.124.0/mr-view.json");

const CONFLICTING: &str = fixture!("glab/1.124.0/mr-view-conflicting.json");

const JOBS: &str = fixture!("glab/1.124.0/pipeline-jobs.json");

const JOBS_FAILED: &str = fixture!("glab/1.124.0/pipeline-jobs-failed.json");

const NONE_STDOUT: &str = fixture!("glab/1.124.0/mr-view-none.stdout");

const NONE_STDERR: &str = fixture!("glab/1.124.0/mr-view-none.stderr");

const SEVERAL_STDOUT: &str = fixture!("glab/1.124.0/mr-view-several.stdout");

const SEVERAL_STDERR: &str = fixture!("glab/1.124.0/mr-view-several.stderr");

struct Fake {
    tmp: TempDir,
    glab: PathBuf,
}

impl Fake {
    fn new(files: &[(&str, &str)], script: &str) -> Self {
        let tmp = TempDir::new();
        for (name, contents) in files {
            std::fs::write(tmp.path().join(name), contents).expect("write a fixture");
        }
        let glab = fake_glab(tmp.path(), &format!("echo \"$*\" >> \"$0.calls\"\n{script}"));
        Self { tmp, glab }
    }

    fn answering(view: &str, jobs: &str) -> Self {
        let script = "case \"$1\" in mr) cat \"$0.view\";; api) cat \"$0.jobs\";; esac";
        Self::new(&[("glab.view", view), ("glab.jobs", jobs)], script)
    }

    fn find(&self) -> Result<Option<PullRequest>> {
        find(&self.glab, self.tmp.path())
    }

    fn calls(&self) -> Vec<String> {
        let calls = std::fs::read_to_string(format!("{}.calls", self.glab.display())).unwrap_or_default();
        calls.lines().map(str::to_string).collect()
    }
}

fn found(view: &str, jobs: &str) -> PullRequest {
    Fake::answering(view, jobs).find().expect("find").expect("a merge request")
}

fn names(pull: &PullRequest, outcome: Outcome) -> Vec<String> {
    pull.checks.iter().filter(|c| c.outcome == outcome).map(|c| format!("{} ({})", c.name, c.workflow)).collect()
}

fn edited(json: &str, change: impl FnOnce(&mut Value)) -> String {
    let mut value: Value = serde_json::from_str(json).expect("a fixture");
    change(&mut value);
    value.to_string()
}

fn raw(json: &str) -> Raw {
    serde_json::from_str(json).expect("a merge request")
}

mod find {
    use super::*;

    #[test]
    fn reads_the_merge_request_of_the_branch_and_its_pipeline_jobs() {
        let fake = Fake::answering(VIEW, JOBS);

        let pull = fake.find().expect("find").expect("a merge request");

        assert_eq!((pull.number, pull.state, pull.provider), (482, State::Open, Provider::Gitlab));
        assert_eq!(pull.title, "Pay with a gift card at checkout");
        assert_eq!(pull.url, "https://gitlab.com/acme/web-shop/-/merge_requests/482");
        assert_eq!((pull.base.as_str(), pull.conflicting), ("main", Some(false)));
        assert_eq!(
            names(&pull, Outcome::Passed),
            [
                "commit-lint (checks)",
                "docs:build (docs)",
                "secret-detection (security)",
                "lint (test)",
                "tests:unit (test)"
            ]
        );
        assert_eq!(names(&pull, Outcome::Skipped), ["review-deploy (review)", "vulnerability-check (security)"]);
        assert_eq!(pull.summary(), "open · 5 passed, 2 skipped");
        assert_eq!(pull.problems(), None);
        assert_eq!(
            fake.calls(),
            ["mr view --output json", "api --paginate projects/4101/pipelines/70482/jobs?per_page=100"]
        );
    }

    #[test]
    fn a_branch_without_one_is_none() {
        let script = format!("cat <<'EOF'\n{NONE_STDOUT}\nEOF\ncat >&2 <<'EOF'\n{NONE_STDERR}\nEOF\nexit 1");
        let fake = Fake::new(&[], &script);

        assert_eq!(fake.find().expect("not an error"), None);
        assert_eq!(fake.calls().len(), 1);
    }

    #[test]
    fn an_older_glab_without_the_json_error_still_says_none() {
        let script = format!("cat >&2 <<'EOF'\n{NONE_STDERR}\nEOF\nexit 1");

        assert_eq!(Fake::new(&[], &script).find().expect("not an error"), None);
    }

    #[test]
    fn several_from_the_branch_takes_the_newest() {
        let script = "case \"$1 $2 $3\" in\n\
                      'mr view --output') cat \"$0.out\"; cat \"$0.err\" >&2; exit 1;;\n\
                      'mr view 486') cat \"$0.view\";;\n\
                      'api --paginate'*) cat \"$0.jobs\";;\n\
                      esac";
        let files = [
            ("glab.out", SEVERAL_STDOUT),
            ("glab.err", SEVERAL_STDERR),
            ("glab.view", CONFLICTING),
            ("glab.jobs", JOBS_FAILED),
        ];
        let fake = Fake::new(&files, script);

        let pull = fake.find().expect("find").expect("a merge request");

        assert_eq!(pull.number, 486);
        assert_eq!(fake.calls()[1], "mr view 486 --output json");
    }

    #[test]
    fn no_head_pipeline_is_no_checks_and_no_api_call() {
        let view = edited(VIEW, |mr| mr["head_pipeline"] = Value::Null);
        let fake = Fake::answering(&view, JOBS);

        let pull = fake.find().expect("find").expect("a merge request");

        assert_eq!(pull.checks, Vec::<Check>::new());
        assert_eq!(pull.summary(), "open · no pipeline");
        assert_eq!(fake.calls(), ["mr view --output json"]);
    }

    #[test]
    fn the_pipeline_of_a_fork_is_read_from_its_own_project() {
        let view = edited(VIEW, |mr| mr["head_pipeline"]["project_id"] = 4102.into());
        let fake = Fake::answering(&view, JOBS);

        fake.find().expect("find");

        assert_eq!(fake.calls()[1], "api --paginate projects/4102/pipelines/70482/jobs?per_page=100");
    }

    #[test]
    fn other_failures_say_glabs_own_line_not_fangs_header() {
        let script = "echo '{\"error\":{\"message\":\"failed to get open merge request for \\\"feat/x\\\": 500\"}}'; \
                      printf '\\n  ERROR  \\n\\n  Failed.\\n' >&2; exit 1";

        let error = Fake::new(&[], script).find().expect_err("glab failed");

        assert_eq!(explain(&error), "failed to get open merge request for \"feat/x\": 500");
    }

    #[test]
    fn a_failing_jobs_call_is_an_error() {
        let script = "case \"$1\" in mr) cat \"$0.view\";; api) echo '{\"message\":\"404 Not found\"}'; \
                      echo 'glab: 404 Not found (HTTP 404)' >&2; exit 1;; esac";
        let fake = Fake::new(&[("glab.view", VIEW)], script);

        let error = fake.find().expect_err("the jobs failed");

        assert_eq!(explain(&error), "glab: 404 Not found (HTTP 404)");
    }

    #[test]
    fn jobs_in_several_pages_are_all_read() {
        let script = "case \"$1\" in mr) cat \"$0.view\";; api) cat \"$0.jobs\" \"$0.jobs\";; esac";
        let fake = Fake::new(&[("glab.view", VIEW), ("glab.jobs", JOBS)], script);

        let pull = fake.find().expect("find").expect("a merge request");

        assert_eq!(pull.checks.len(), 14);
    }

    #[test]
    fn something_unexpected_is_an_error() {
        let error = Fake::new(&[], "echo 'hello'").find().expect_err("not json");

        assert!(explain(&error).starts_with("glab printed something unexpected"));
    }
}

mod explain {
    use super::*;

    #[test]
    fn a_missing_glab_says_to_install_it() {
        let tmp = TempDir::new();

        let error = find(&tmp.path().join("no-glab"), tmp.path()).expect_err("no glab");

        assert_eq!(explain(&error), "install glab, GitLab's CLI, to see them");
    }

    #[rstest]
    #[case::a_401("glab: 401 Unauthorized (HTTP 401)")]
    #[case::asking_to_log_in("To get started with GitLab CLI, please run: glab auth login")]
    fn a_logged_out_glab_says_to_log_in(#[case] said: &str) {
        assert_eq!(explain(&Error::Glab(said.into())), glab::LOGGED_OUT);
    }
}

mod jobs {
    use super::*;

    #[rstest]
    #[case::success("success", false, Outcome::Passed)]
    #[case::failed("failed", false, Outcome::Failed)]
    #[case::failed_but_allowed("failed", true, Outcome::Skipped)]
    #[case::canceled("canceled", false, Outcome::Failed)]
    #[case::skipped("skipped", false, Outcome::Skipped)]
    #[case::manual("manual", true, Outcome::Skipped)]
    #[case::created("created", false, Outcome::Pending)]
    #[case::preparing("preparing", false, Outcome::Pending)]
    #[case::waiting_for_resource("waiting_for_resource", false, Outcome::Pending)]
    #[case::pending("pending", false, Outcome::Pending)]
    #[case::running("running", false, Outcome::Pending)]
    #[case::scheduled("scheduled", false, Outcome::Pending)]
    #[case::unknown("something-new", false, Outcome::Pending)]
    fn a_status_is_an_outcome(#[case] status: &str, #[case] allowed: bool, #[case] expected: Outcome) {
        assert_eq!(outcome(status, allowed), expected);
    }

    fn with_pipeline(status: &str, jobs: &[(&str, &str)]) -> Vec<Check> {
        let job = |(name, status): &(&str, &str)| {
            format!(r#"{{"name":"{name}","stage":"test","status":"{status}","web_url":"https://x/{name}"}}"#)
        };
        let jobs = jobs.iter().map(job).collect::<Vec<_>>().join(",");
        let view = format!(
            r#"{{"iid":3,"head_pipeline":{{"id":9,"project_id":2,"status":"{status}","web_url":"https://x/pipeline"}}}}"#
        );
        found(&view, &format!("[{jobs}]")).checks
    }

    #[test]
    fn a_failed_pipeline_with_passed_jobs_is_one_failed_check_with_its_link() {
        let checks = with_pipeline("failed", &[("lint", "success")]);

        assert_eq!(checks.len(), 2);
        assert_eq!(
            checks[0],
            Check {
                name: "pipeline".into(),
                workflow: String::new(),
                outcome: Outcome::Failed,
                url: "https://x/pipeline".into()
            }
        );
    }

    #[test]
    fn a_running_pipeline_with_finished_jobs_is_pending() {
        let checks = with_pipeline("running", &[("lint", "success")]);

        assert_eq!(checks.iter().filter(|c| c.outcome == Outcome::Pending).count(), 1);
    }

    #[rstest]
    #[case::passed("success", "success")]
    #[case::failed_with_a_failed_job("failed", "failed")]
    #[case::running_with_a_running_job("running", "running")]
    #[case::running_with_a_failed_job("running", "failed")]
    fn the_pipeline_adds_nothing_when_its_jobs_say_it(#[case] pipeline: &str, #[case] job: &str) {
        assert_eq!(with_pipeline(pipeline, &[("lint", job)]).len(), 1);
    }

    #[test]
    fn missing_and_null_fields_are_tolerated() {
        let view = r#"{"iid":3,"title":null,"state":null,"head_pipeline":{"id":9,"project_id":2,"status":null}}"#;

        let checks = found(view, r#"[{"name":null,"stage":null,"status":null,"web_url":null},{}]"#).checks;

        assert_eq!(checks.len(), 2);
        assert!(checks.iter().all(|c| c.outcome == Outcome::Pending));
    }

    #[test]
    fn a_pipeline_without_an_id_asks_for_no_jobs() {
        let fake = Fake::answering(r#"{"iid":3,"head_pipeline":{"status":"failed"}}"#, "[]");

        let pull = fake.find().expect("find").expect("a merge request");

        assert_eq!(pull.checks.len(), 1);
        assert_eq!(fake.calls(), ["mr view --output json"]);
    }
}

mod state {
    use super::*;

    #[rstest]
    #[case::opened(r#""state":"opened""#, State::Open)]
    #[case::draft(r#""state":"opened","draft":true"#, State::Draft)]
    #[case::work_in_progress(r#""state":"opened","work_in_progress":true"#, State::Draft)]
    #[case::locked(r#""state":"locked""#, State::Open)]
    #[case::merged(r#""state":"merged","draft":true"#, State::Merged)]
    #[case::closed(r#""state":"closed""#, State::Closed)]
    #[case::none(r#""state":null"#, State::Open)]
    fn a_merge_request_state(#[case] fields: &str, #[case] expected: State) {
        let pull = raw(&format!(r#"{{"iid":1,{fields}}}"#)).into_pull(Vec::new());

        assert_eq!(pull.state, expected);
    }
}

mod conflicts {
    use super::*;

    #[rstest]
    #[case::mergeable(Some(false), Some("mergeable"), Some(false))]
    #[case::conflict(Some(true), Some("conflict"), Some(true))]
    #[case::conflict_status_only(Some(false), Some("conflict"), Some(true))]
    #[case::first_blocker_is_the_draft(Some(true), Some("draft_status"), Some(true))]
    #[case::checking(Some(true), Some("checking"), None)]
    #[case::unchecked(Some(false), Some("unchecked"), None)]
    #[case::preparing(Some(false), Some("preparing"), None)]
    #[case::nothing(None, None, None)]
    fn conflicting(#[case] has: Option<bool>, #[case] status: Option<&str>, #[case] expected: Option<bool>) {
        let json = serde_json::json!({"iid": 1, "has_conflicts": has, "detailed_merge_status": status});

        assert_eq!(raw(&json.to_string()).into_pull(Vec::new()).conflicting, expected);
    }

    #[test]
    fn the_conflicting_fixture_is_a_draft_that_conflicts_and_failed() {
        let pull = found(CONFLICTING, JOBS_FAILED);

        assert_eq!((pull.number, pull.state, pull.conflicting), (486, State::Draft, Some(true)));
        assert_eq!(pull.summary(), "draft · conflicts with main · 2 failed, 3 passed, 1 skipped");
        assert_eq!(
            pull.problems().as_deref(),
            Some(
                "merge request !486 has merge conflicts with main and failed its pipeline: commit-lint \
                 https://gitlab.com/acme/web-shop/-/jobs/81003, lint https://gitlab.com/acme/web-shop/-/jobs/81001 "
            )
        );
        assert_eq!(names(&pull, Outcome::Failed), ["commit-lint (checks)", "lint (test)"]);
    }

    #[test]
    fn an_unchecked_answer_keeps_the_last_known_conflict() {
        let before = found(CONFLICTING, JOBS_FAILED);
        let unchecked = edited(CONFLICTING, |mr| {
            mr["detailed_merge_status"] = "unchecked".into();
            mr["has_conflicts"] = false.into();
        });
        let mut now = found(&unchecked, JOBS_FAILED);

        assert_eq!(now.conflicting, None);
        now.keep_known(&before);

        assert_eq!(now.conflicting, Some(true));
    }
}
