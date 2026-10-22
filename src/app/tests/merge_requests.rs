use std::time::Duration;

use rstest::rstest;

use super::pull_requests::{Repo, notes, open_dialog_of, repo_with, row_text};
use super::*;
use crate::app::pull_requests::{ASK_AGENT, Answered, COPY_LINK, DONE};
use crate::pull_request::glab::LOGGED_OUT;
use crate::pull_request::{Provider, State};
use crate::test_util::{fake_glab, fixture, git};

const VIEW: &str = fixture!("glab/1.124.0/mr-view.json");

const CONFLICTING: &str = fixture!("glab/1.124.0/mr-view-conflicting.json");

const JOBS: &str = fixture!("glab/1.124.0/pipeline-jobs.json");

const JOBS_FAILED: &str = fixture!("glab/1.124.0/pipeline-jobs-failed.json");

const NONE_STDOUT: &str = fixture!("glab/1.124.0/mr-view-none.stdout");

const NONE_STDERR: &str = fixture!("glab/1.124.0/mr-view-none.stderr");

const SIGNED_IN: &str = fixture!("glab/1.123.0/auth-status.json");

const LOGGED_OUT_STATUS: &str = fixture!("glab/1.124.0/auth-status-logged-out.json");

const GITLAB: &str = "git@gitlab.com:acme/web-shop.git";

const URL: &str = "https://gitlab.com/acme/web-shop/-/merge_requests/482";

const SCRIPT: &str = "echo \"$*\" >> \"$0.calls\"
case \"$1 $2\" in
'auth status') cat \"$0.status\";;
'mr view')
  if [ -f \"$0.view\" ] && [ \"$(git rev-parse --abbrev-ref HEAD)\" = feat/x ]; then cat \"$0.view\"
  else cat \"$0.none\"; cat \"$0.err\" >&2; exit 1; fi;;
'api --paginate') cat \"$0.jobs\";;
esac";

pub(super) fn install_glab(repo: &mut Repo, view: &str, jobs: &str) {
    let tmp = repo.tmp.path();
    for (name, contents) in
        [("view", view), ("jobs", jobs), ("status", SIGNED_IN), ("none", NONE_STDOUT), ("err", NONE_STDERR)]
    {
        std::fs::write(tmp.join(format!("glab.{name}")), contents).expect("a fixture");
    }
    repo.app.config.glab = fake_glab(tmp, SCRIPT).display().to_string();
}

fn point_origin_at(repo: &Repo, url: &str) {
    git(&repo.app.projects[0].path, &["remote", "set-url", "origin", url]);
}

pub(super) fn gitlab(view: &str, jobs: &str) -> Repo {
    let mut repo = repo_with(&pull_requests::printing(), |_| {});
    point_origin_at(&repo, GITLAB);
    install_glab(&mut repo, view, jobs);
    repo
}

fn set(repo: &Repo, name: &str, contents: &str) {
    std::fs::write(repo.tmp.path().join(format!("glab.{name}")), contents).expect("an answer");
}

fn glab_calls(repo: &Repo) -> Vec<String> {
    let calls = std::fs::read_to_string(repo.tmp.path().join("glab.calls")).unwrap_or_default();
    calls.lines().map(str::to_string).collect()
}

fn ask_at(repo: &mut Repo, now: Instant) {
    repo.app.ask_pull_requests(now);
    pump_until(&mut repo.app, &repo.rx, "glab hosts", |a| !a.git.pulls.glab.asking());
}

fn ask(repo: &mut Repo) {
    ask_at(repo, Instant::now());
    repo.ask("glab answers");
}

fn found(view: &str, jobs: &str) -> Repo {
    let mut repo = gitlab(view, jobs);
    ask(&mut repo);
    assert!(repo.app.pull_request(repo.worktree()).is_some(), "{:?}", repo.app.pull_requests_note());
    repo
}

fn row(repo: &mut Repo) -> String {
    row_text(&mut repo.app, WorkspaceRow::Workspace(1))
}

mod rows {
    use super::*;

    #[test]
    fn a_worktree_on_gitlab_shows_its_merge_request_and_pipeline_on_its_row() {
        let mut passing = found(VIEW, JOBS);
        let mut conflicting = found(CONFLICTING, JOBS_FAILED);

        assert!(row(&mut passing).contains("#482✓"), "{}", row(&mut passing));
        assert!(row(&mut conflicting).contains("#486!"), "{}", row(&mut conflicting));
        let pull = passing.app.pull_request(passing.worktree()).expect("a merge request");
        assert_eq!((pull.number, pull.state, pull.url.as_str()), (482, State::Open, URL));
    }

    #[test]
    fn a_merge_request_is_in_no_stack() {
        let repo = found(VIEW, JOBS);

        let pull = repo.app.pull_request(repo.worktree()).expect("a merge request");

        assert_eq!(pull.stack, None);
        let report = repo.app.report(None);
        let info = report.projects[0].workspaces[1].pull_request.as_ref().expect("a merge request");
        assert_eq!(info.stack, None);
    }

    #[test]
    fn gh_is_never_run_for_a_gitlab_project() {
        let repo = found(VIEW, JOBS);

        assert_eq!(repo.calls(), Vec::<String>::new());
        assert_eq!(glab_calls(&repo)[0], "auth status --all --output json");
    }

    #[rstest]
    #[case::github_com(Some("git@github.com:acme/web-shop.git"))]
    #[case::a_local_remote(None)]
    fn glab_is_never_run_for_a_project_that_is_not_on_gitlab(#[case] origin: Option<&str>) {
        let mut repo = repo_with(&pull_requests::printing(), |_| {});
        install_glab(&mut repo, VIEW, JOBS);
        if let Some(url) = origin {
            point_origin_at(&repo, url);
        }

        repo.ask("gh answers");

        assert!(repo.app.pull_request(repo.worktree()).is_some_and(|p| p.provider == Provider::Github));
        assert_eq!(glab_calls(&repo), Vec::<String>::new());
    }

    #[test]
    fn a_branch_without_one_shows_nothing_and_says_merge_request() {
        let mut repo = gitlab(VIEW, JOBS);
        std::fs::remove_file(repo.tmp.path().join("glab.view")).expect("no merge request");
        ask(&mut repo);

        assert_eq!(repo.app.pull_request(repo.worktree()), None);
        assert!(!row(&mut repo).contains('#'));
        repo.app.projects[0].active = 1;
        repo.app.pull_request_by_key();
        assert_eq!(toast(&repo.app), Some("no merge request for feat/x"));
    }

    #[test]
    fn an_unchecked_answer_keeps_the_conflict_shown() {
        let mut repo = found(CONFLICTING, JOBS_FAILED);
        let mut unchecked: serde_json::Value = serde_json::from_str(CONFLICTING).expect("a fixture");
        unchecked["has_conflicts"] = false.into();
        unchecked["detailed_merge_status"] = "unchecked".into();
        set(&repo, "view", &unchecked.to_string());

        ask_at(&mut repo, Instant::now() + Duration::from_mins(10));
        repo.ask("glab answers");

        assert!(row(&mut repo).contains("#486!"), "{}", row(&mut repo));
    }
}

mod without_glab {
    use super::*;

    #[test]
    fn nothing_shows_and_the_note_says_to_install_it() {
        let mut repo = gitlab(VIEW, JOBS);
        repo.app.config.glab = "/nonexistent/glab".into();

        ask(&mut repo);

        assert_eq!(repo.app.pull_requests_note(), "install glab, GitLab's CLI, to see them");
        assert!(!row(&mut repo).contains('#'));
        repo.app.projects[0].active = 1;
        repo.app.pull_request_by_key();
        assert_eq!(toast(&repo.app), Some("install glab, GitLab's CLI, to see them"));
    }

    #[test]
    fn a_logged_out_glab_says_to_log_in_and_asks_nothing_else() {
        let mut repo = gitlab(VIEW, JOBS);
        set(&repo, "status", LOGGED_OUT_STATUS);

        ask(&mut repo);

        assert_eq!(repo.app.pull_requests_note(), LOGGED_OUT);
        assert!(!row(&mut repo).contains('#'));
        assert!(glab_calls(&repo).iter().all(|call| call.starts_with("auth status")), "{:?}", glab_calls(&repo));
    }

    #[test]
    fn logging_out_after_a_read_takes_the_merge_request_off_the_row() {
        let mut repo = found(VIEW, JOBS);
        set(&repo, "status", LOGGED_OUT_STATUS);

        let later = Instant::now() + Duration::from_mins(16);
        ask_at(&mut repo, later);
        pump_until(&mut repo.app, &repo.rx, "glab answers", |a| a.git.pulls.asking.is_empty());
        ask_at(&mut repo, later);

        assert_eq!(repo.app.pull_request(repo.worktree()), None);
        assert!(!row(&mut repo).contains('#'));
        assert_eq!(repo.app.pull_requests_note(), LOGGED_OUT);
    }

    #[test]
    fn logging_in_is_seen_without_waiting_five_minutes() {
        let mut repo = gitlab(VIEW, JOBS);
        set(&repo, "status", LOGGED_OUT_STATUS);
        ask(&mut repo);
        set(&repo, "status", SIGNED_IN);

        ask_at(&mut repo, Instant::now() + Duration::from_mins(6));
        repo.ask("glab answers");

        assert!(row(&mut repo).contains("#482✓"), "{}", row(&mut repo));
        assert_eq!(repo.app.pull_requests_note(), "each workspace's pull request and its checks, from gh or glab");
    }
}

mod note {
    use super::*;

    fn answered(repo: &mut Repo, answers: &[(u64, Provider, Option<&str>)]) -> String {
        let now = Instant::now();
        for (project, provider, error) in answers {
            let error = error.map(String::from);
            repo.app.git.pulls.answered.insert(*project, Answered { at: now, error, provider: *provider });
        }
        repo.app.pull_requests_note()
    }

    #[rstest]
    #[case::all_gitlab_failed(&[(1, Provider::Github, None), (2, Provider::Gitlab, Some("log in"))], "log in")]
    #[case::all_github_failed(&[(1, Provider::Github, Some("install gh")), (2, Provider::Gitlab, None)], "install gh")]
    #[case::one_of_each_works(&[(1, Provider::Github, None), (2, Provider::Gitlab, None)], NOTE)]
    #[case::one_gitlab_works(&[(1, Provider::Gitlab, None), (2, Provider::Gitlab, Some("log in"))], NOTE)]
    fn the_note_gives_a_reason_when_every_project_read_with_one_tool_failed(
        #[case] answers: &[(u64, Provider, Option<&str>)],
        #[case] expected: &str,
    ) {
        let mut repo = repo_with(&pull_requests::printing(), |_| {});

        assert_eq!(answered(&mut repo, answers), expected);
    }

    const NOTE: &str = "each workspace's pull request and its checks, from gh or glab";
}

mod dialog {
    use super::*;

    #[test]
    fn says_merge_request_and_lists_the_jobs() {
        let mut repo = found(CONFLICTING, JOBS_FAILED);

        open_dialog_of(&mut repo, "merge request !486");

        assert_eq!(
            notes(&repo.app),
            [
                "draft · conflicts with main · 2 failed, 3 passed, 1 skipped",
                "",
                "✗ commit-lint  checks",
                "✗ lint  test",
                "✓ docs:build  docs",
                "✓ secret-detection  security",
                "✓ tests:unit  test",
                "- vulnerability-check  security",
            ]
        );
        let Some(Overlay::PullRequest { workspace, scroll }) = repo.app.overlay else {
            panic!("the dialog is not open")
        };
        let Some(ui::Overlay::Update(update)) = repo.app.pull_request_view(workspace, scroll, AREA) else {
            panic!("no view")
        };
        assert_eq!(update.title, "Merge request");
        assert_eq!(update.message, "!486 Draft: Remember the last address used at checkout");
    }

    #[test]
    fn copy_link_copies_the_merge_request_link() {
        let mut repo = found(VIEW, JOBS);
        open_dialog_of(&mut repo, "merge request !482");

        click(&mut repo.app, ui::update_buttons(AREA, COPY_LINK, DONE)[0].as_position());

        assert_eq!(repo.app.take_host_writes(), [clipboard::osc52(URL)]);
        assert_eq!(toast(&repo.app), Some(COPIED));
    }

    #[test]
    fn ask_agent_hands_over_the_conflict_and_the_failed_jobs() {
        let mut repo = found(CONFLICTING, JOBS_FAILED);
        open_dialog_of(&mut repo, "merge request !486");

        click(&mut repo.app, ui::update_extra(AREA, COPY_LINK, DONE, ASK_AGENT).as_position());

        let text = "merge request !486 has merge conflicts with main and failed its pipeline: commit-lint \
                    https://gitlab.com/acme/web-shop/-/jobs/81003, lint https://gitlab.com/acme/web-shop/-/jobs/81001";
        assert_eq!(repo.app.take_host_writes(), [clipboard::osc52(text)]);
        assert_eq!(toast(&repo.app), Some(NO_AGENT));
    }
}

mod scripts {
    use super::*;

    #[test]
    fn see_the_provider() {
        let repo = found(CONFLICTING, JOBS_FAILED);

        let report = repo.app.report(None);

        let pull = report.projects[0].workspaces[1].pull_request.as_ref().expect("in the report");
        assert_eq!((pull.number, pull.provider.as_str(), pull.state.as_str()), (486, "gitlab", "draft"));
        assert_eq!((pull.checks.as_deref(), pull.conflicts), (Some("failing"), true));
        assert_eq!(pull.failed, ["commit-lint", "lint"]);
    }
}
