use rstest::rstest;

use super::*;
use crate::app::pull_requests::{Answered, PullRead};
use crate::app::tests::merge_requests::gitlab;
use crate::app::tests::pull_requests::{Repo, printing, repo_on, repo_with};
use crate::pull_request::Provider;
use crate::pull_request::glab::LOGGED_OUT;
use crate::test_util::{fake_gh, fixture, git, git_repo};

const JOB: &str = "https://github.com/acme/web-shop/actions/runs/1001/job";

const JOBS_FAILED: &str = fixture!("glab/1.124.0/pipeline-jobs-failed.json");

const MR: &str = fixture!("glab/1.124.0/mr-view.json");

fn read_with(repo: &mut Repo, args: &Value) -> Value {
    call_later(&mut repo.app, &repo.rx, "pull-request", args).expect("an answer")
}

fn read(repo: &mut Repo) -> Value {
    let args = json!({"workspace": repo.worktree()});
    read_with(repo, &args)
}

fn refused_read(repo: &mut Repo) -> Refused {
    let args = json!({"workspace": repo.worktree()});
    call_later(&mut repo.app, &repo.rx, "pull-request", &args).expect_err("refused")
}

fn failing(message: &str) -> Repo {
    repo_with(&format!("echo '{message}' >&2; exit 1"), |_| {})
}

fn gated() -> Repo {
    let gate = "while [ ! -f \"$0.go\" ]; do sleep 0.05; done";
    repo_on(
        &["feat/x"],
        |dir| {
            let script = format!(
                "if [ \"$1\" = api ]; then echo '{{\"data\":{{\"resource\":{{\"stack\":null}}}}}}'; exit 0; fi\npwd >> \"$0.calls\"\n{gate}\n{}",
                printing()
            );
            fake_gh(dir, &script)
        },
        |_| {},
    )
}

fn release(repo: &Repo) {
    std::fs::write(repo.tmp.path().join("gh.go"), "").expect("release gh");
}

mod what_it_answers {
    use super::*;

    #[test]
    fn it_lists_every_check_in_the_dialogs_order_with_its_workflow_and_link() {
        let mut repo = repo_with(&printing(), |_| {});

        let answer = read(&mut repo);

        assert_eq!((&answer["branch"], &answer["reason"]), (&json!("feat/x"), &Value::Null));
        let checks = answer["pull_request"]["all_checks"].as_array().expect("checks").clone();
        let seen: Vec<(&str, &str, &str, &str)> = checks
            .iter()
            .map(|c| {
                let text = |key: &str| c[key].as_str().expect("text");
                (text("name"), text("outcome"), text("workflow"), text("url"))
            })
            .collect();
        assert_eq!(
            seen,
            vec![
                ("tests (ubuntu-24.04)", "failed", "CI", format!("{JOB}/2001").as_str()),
                ("tests (macos-15)", "pending", "CI", format!("{JOB}/2002").as_str()),
                ("lint", "passed", "CI", format!("{JOB}/2003").as_str()),
                ("review-bot", "passed", "", ""),
                ("deploy", "skipped", "Pages", "https://github.com/acme/web-shop/actions/runs/1002/job/2004"),
            ]
        );
    }

    #[test]
    fn the_rest_is_the_object_status_gives() {
        let mut repo = repo_with(&printing(), |_| {});
        let mut answer = read(&mut repo);

        let report = serde_json::to_value(repo.app.report(None)).expect("json");
        let status = report["projects"][0]["workspaces"][1]["pull_request"].clone();

        answer["pull_request"].as_object_mut().expect("an object").remove("all_checks");
        assert_eq!(answer["pull_request"], status);
        assert_eq!(status["number"], 482);
    }

    #[test]
    fn status_leaves_the_list_out() {
        let mut repo = repo_with(&printing(), |_| {});
        read(&mut repo);

        let report = serde_json::to_value(repo.app.report(None)).expect("json");

        assert!(report["projects"][0]["workspaces"][1]["pull_request"].get("all_checks").is_none());
    }

    #[test]
    fn without_a_workspace_it_reads_the_shown_one() {
        let mut repo = repo_with(&printing(), |_| {});
        repo.app.projects[0].active = 1;

        let answer = read_with(&mut repo, &json!({}));

        assert_eq!(answer["workspace"], repo.worktree());
        assert_eq!(answer["pull_request"]["number"], 482);
    }

    #[test]
    fn a_merge_request_lists_its_jobs_with_their_stage() {
        let mut repo = gitlab(MR, JOBS_FAILED);

        let answer = read(&mut repo);

        let pull = &answer["pull_request"];
        assert_eq!(pull["provider"], "gitlab");
        let checks = pull["all_checks"].as_array().expect("checks");
        let lint = checks.iter().find(|c| c["name"] == "lint").expect("the lint job");
        assert_eq!((&lint["workflow"], &lint["outcome"]), (&json!("test"), &json!("failed")));
        assert!(lint["url"].as_str().is_some_and(|url| url.ends_with("/jobs/81001")), "{lint}");
    }
}

mod asking_at_once {
    use super::*;

    #[test]
    fn it_asks_gh_again_at_once() {
        let mut repo = repo_with(&printing(), |_| {});
        repo.ask("gh answers");

        read(&mut repo);

        assert_eq!(repo.calls().len(), 2);
    }

    #[test]
    fn it_asks_while_the_project_waits_after_a_failure_and_says_why() {
        let mut repo = failing("error connecting to api.github.com");
        repo.ask("gh fails");

        let refused = refused_read(&mut repo);

        assert_eq!((refused.code.as_str(), refused.message.as_str()), ("failed", "error connecting to api.github.com"));
        assert_eq!(repo.calls().len(), 2);
    }

    #[test]
    fn a_read_already_under_way_answers_it() {
        let mut repo = repo_with(&printing(), |_| {});
        repo.app.ask_pull_requests(Instant::now());

        let answer = read(&mut repo);

        assert_eq!(answer["pull_request"]["number"], 482);
        assert_eq!(repo.calls().len(), 1);
    }

    #[test]
    fn a_read_of_the_branch_left_behind_is_not_the_answer() {
        let mut repo = repo_with(&printing(), |_| {});
        repo.ask("gh answers");
        let path = repo.app.projects[0].workspaces[1].path.clone();
        git(&path, &["checkout", "--quiet", "-b", "feat/y"]);

        let read = repo.app.pull_request_read(repo.worktree());

        assert!(matches!(read, PullRead::Waiting));
    }

    #[test]
    fn a_failure_of_the_project_still_counts_after_the_branch_changed() {
        let mut repo = repo_with(&printing(), |_| {});
        repo.ask("gh answers");
        let path = repo.app.projects[0].workspaces[1].path.clone();
        git(&path, &["checkout", "--quiet", "-b", "feat/y"]);
        let project = repo.app.projects[0].id;
        let failed = Answered { at: Instant::now(), error: Some("boom".into()), provider: Provider::Github };
        repo.app.git.pulls.answered.insert(project, failed);

        let read = repo.app.pull_request_read(repo.worktree());

        assert!(matches!(read, PullRead::Failed(why) if why == "boom"));
    }
}

mod failures {
    use super::*;

    #[rstest]
    #[case::no_gh("/nonexistent/gh", "install gh, GitHub's CLI, to see them")]
    fn a_failed_read_is_refused_with_why(#[case] gh: &str, #[case] why: &str) {
        let mut repo = repo_with(&printing(), |c| c.gh = gh.into());

        let refused = refused_read(&mut repo);

        assert_eq!((refused.code.as_str(), refused.message.as_str()), ("failed", why));
    }

    #[test]
    fn a_logged_out_gh_says_to_log_in() {
        let mut repo = failing("To get started with GitHub CLI, please run:  gh auth login");

        let refused = refused_read(&mut repo);

        assert_eq!(refused.message, "gh is not logged in: run gh auth login");
    }

    #[test]
    fn a_workspace_that_closes_while_it_waits_fails_the_request() {
        let mut repo = gated();
        let workspace = repo.worktree();
        let text = json!({"contract": 1, "command": "pull-request", "args": {"workspace": workspace}}).to_string();
        repo.app.request(CLIENT, &text, Some(AREA), Instant::now());
        assert_eq!(repo.app.take_answers().len(), 0);

        repo.app.projects[0].workspaces.remove(1);
        let mut reply = None;
        wait_until("the request fails", || {
            drain(&mut repo.app, &repo.rx);
            repo.app.refresh(Instant::now());
            reply = repo.app.take_answers().into_iter().next().map(|(_, text)| text);
            reply.is_some()
        });
        release(&repo);

        let refused = reply_of(&reply.expect("a reply")).expect_err("refused");
        assert_eq!(refused.code, "failed");
    }

    #[test]
    fn a_logged_out_glab_refuses_with_why() {
        let mut repo = gitlab(MR, JOBS_FAILED);
        let status = fixture!("glab/1.124.0/auth-status-logged-out.json");
        std::fs::write(repo.tmp.path().join("glab.status"), status).expect("a status");

        let refused = refused_read(&mut repo);

        assert_eq!((refused.code.as_str(), refused.message.as_str()), ("failed", LOGGED_OUT));
    }

    #[test]
    fn a_view_is_refused_as_malformed() {
        let mut repo = repo_with(&printing(), |_| {});
        let text = json!({"contract": 1, "view": 5, "command": "pull-request", "args": {}}).to_string();
        repo.app.request(CLIENT, &text, Some(AREA), Instant::now());

        let answers = repo.app.take_answers();

        let refused = reply_of(&answers[0].1).expect_err("refused");
        assert_eq!(refused.code, "malformed");
    }

    #[test]
    fn an_unknown_workspace_is_not_found() {
        let mut repo = repo_with(&printing(), |_| {});

        let refused = refused(&mut repo.app, "pull-request", &json!({"workspace": 999}));

        assert_eq!(refused.code, "not-found");
    }
}

mod never_asked {
    use super::*;

    #[test]
    fn a_branch_without_one_answers_null_and_says_so() {
        let mut repo = repo_with("echo 'no pull requests found for branch' >&2; exit 1", |_| {});

        let answer = read(&mut repo);

        assert_eq!(answer["pull_request"], Value::Null);
        assert_eq!(answer["reason"], "no pull request for feat/x");
    }

    fn detached(repo: &Repo) {
        let path = repo.app.projects[0].workspaces[1].path.clone();
        git(&path, &["checkout", "--quiet", "--detach"]);
    }

    #[rstest]
    #[case::off("off", 1, "pull requests are off in settings")]
    #[case::default_branch(
        "default",
        0,
        "main is the default branch: cornercase looks for pull requests on other branches only"
    )]
    #[case::detached("detached", 1, "this workspace is not on a branch")]
    fn a_workspace_cornercase_never_asks_about_is_answered_at_once(
        #[case] change: &str,
        #[case] index: usize,
        #[case] reason: &str,
    ) {
        let mut repo = repo_with(&printing(), |_| {});
        match change {
            "off" => repo.app.config.pull_requests = false,
            "detached" => detached(&repo),
            _ => {}
        }
        let id = repo.app.projects[0].workspaces[index].id;

        let answer = call(&mut repo.app, "pull-request", &json!({"workspace": id})).expect("an answer");

        assert_eq!((&answer["pull_request"], answer["reason"].as_str()), (&Value::Null, Some(reason)));
        assert_eq!(repo.calls().len(), 0);
    }

    #[test]
    fn a_repository_without_a_remote_says_so() {
        let repo = git_repo(&[]);
        let tmp = TempDir::new();
        let checkout = tmp.path().join("feat-x");
        git(repo.path(), &["worktree", "add", "--quiet", "-b", "feat/x", &checkout.display().to_string()]);
        let (mut app, _rx) = app_in(repo.path(), no_config());
        app.sync_worktrees();
        let id = app.projects[0].workspaces[1].id;

        let answer = call(&mut app, "pull-request", &json!({"workspace": id})).expect("an answer");

        assert_eq!(answer["reason"], "this workspace's repository has no remote");
    }

    #[test]
    fn turning_pull_requests_off_while_it_waits_answers_the_reason() {
        let mut repo = gated();
        let text = json!({"contract": 1, "command": "pull-request", "args": {"workspace": repo.worktree()}});
        repo.app.request(CLIENT, &text.to_string(), Some(AREA), Instant::now());
        assert_eq!(repo.app.take_answers().len(), 0);

        repo.app.config.pull_requests = false;
        repo.app.refresh(Instant::now());
        let reply = repo.app.take_answers().into_iter().next().expect("an answer").1;
        release(&repo);

        let answer = reply_of(&reply).expect("an answer");
        assert_eq!(answer["reason"], "pull requests are off in settings");
    }
}
