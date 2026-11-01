use super::*;
use crate::control::PullRequestInfo;

fn check(name: &str, workflow: &str, outcome: &str, url: &str) -> CheckInfo {
    CheckInfo { name: name.into(), workflow: workflow.into(), outcome: outcome.into(), url: url.into() }
}

fn answered(provider: &str, checks: Vec<CheckInfo>) -> PullRequestAnswer {
    let pull = PullRequestInfo {
        number: 482,
        title: "Returns page crashes on an empty address".into(),
        url: "https://github.com/acme/web-shop/pull/482".into(),
        state: "open".into(),
        checks: Some("failing".into()),
        provider: provider.into(),
        all_checks: Some(checks),
        ..PullRequestInfo::default()
    };
    PullRequestAnswer { workspace: 4, branch: Some("feat/x".into()), pull_request: Some(pull), reason: None }
}

#[test]
fn a_pull_request_lists_every_check_with_its_workflow_and_link() {
    let job = "https://github.com/acme/web-shop/actions/runs/1001/job";
    let checks = vec![
        check("tests (ubuntu-24.04)", "CI", "failed", &format!("{job}/2001")),
        check("tests (macos-15)", "CI", "pending", &format!("{job}/2002")),
        check("lint", "CI", "passed", &format!("{job}/2003")),
        check("review-bot", "", "passed", ""),
        check("deploy", "Pages", "skipped", &format!("{job}/2004")),
    ];

    let said = text(&answered("github", checks));

    assert_eq!(
        said,
        "pull request #482 open, checks failing\n\
         Returns page crashes on an empty address\n\
         https://github.com/acme/web-shop/pull/482\n\
         failed   tests (ubuntu-24.04)  CI  https://github.com/acme/web-shop/actions/runs/1001/job/2001\n\
         pending  tests (macos-15)  CI  https://github.com/acme/web-shop/actions/runs/1001/job/2002\n\
         passed   lint  CI  https://github.com/acme/web-shop/actions/runs/1001/job/2003\n\
         passed   review-bot\n\
         skipped  deploy  Pages  https://github.com/acme/web-shop/actions/runs/1001/job/2004\n"
    );
}

#[test]
fn a_merge_request_uses_gitlabs_words_and_stage_names() {
    let said = text(&answered("gitlab", vec![check("rspec", "test", "failed", "https://gitlab.example/job/1")]));

    assert!(said.starts_with("merge request !482 open, pipeline failing\n"), "{said}");
    assert!(said.ends_with("failed   rspec  test  https://gitlab.example/job/1\n"), "{said}");
}

#[test]
fn no_checks_says_so_in_the_providers_word() {
    assert!(text(&answered("github", Vec::new())).ends_with("\nno checks\n"));
    assert!(text(&answered("gitlab", Vec::new())).ends_with("\nno pipeline\n"));
}

#[test]
fn no_pull_request_prints_the_reason() {
    let read = PullRequestAnswer {
        workspace: 4,
        branch: Some("feat/x".into()),
        pull_request: None,
        reason: Some("no pull request for feat/x".into()),
    };

    assert_eq!(text(&read), "no pull request for feat/x\n");
}
