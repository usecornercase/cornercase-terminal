use rstest::rstest;

use super::*;

fn found(text: &str, available: &[Source]) -> Result<(Source, String)> {
    locator(text, None, available).map(|locator| (locator.source, locator.key))
}

#[rstest]
#[case::github("https://github.com/acme/shop/issues/7", Source::Github, "7")]
#[case::gitlab("https://gitlab.com/acme/shop/-/issues/7", Source::Gitlab, "7")]
#[case::gitlab_work_item("https://gitlab.com/acme/shop/-/work_items/7", Source::Gitlab, "7")]
#[case::shortcut("https://app.shortcut.com/acme/story/482/login", Source::Shortcut, "482")]
#[case::linear("https://linear.app/acme/issue/ENG-7/login", Source::Linear, "ENG-7")]
#[case::jira("https://acme.atlassian.net/browse/ENG-7", Source::Jira, "ENG-7")]
#[case::plane("https://app.plane.so/acme/browse/ENG-7/", Source::Plane, "ENG-7")]
#[case::asana_with_a_project("https://app.asana.com/1/12/project/34/task/56?focus=true", Source::Asana, "56")]
#[case::asana_without_a_project("https://app.asana.com/1/12/task/56", Source::Asana, "56")]
#[case::asana_old("https://app.asana.com/0/34/56", Source::Asana, "56")]
#[case::asana_old_focused("https://app.asana.com/0/34/56/f#top", Source::Asana, "56")]
fn reads_every_trackers_url(#[case] url: &str, #[case] source: Source, #[case] key: &str) {
    assert_eq!(locator(url, None, &[]).expect("locator"), Locator { source, key: key.into(), url: Some(url.into()) });
}

#[rstest]
#[case::a_project("https://app.asana.com/1/12/project/34/list/78")]
#[case::the_old_project("https://app.asana.com/0/34")]
#[case::no_path("https://app.asana.com/")]
fn an_asana_url_without_a_task_is_refused(#[case] url: &str) {
    let err = locator(url, None, &[]).err().map(|e| e.to_string());
    assert_eq!(err.as_deref(), Some("the issue URL has no readable issue key"));
}

#[test]
fn a_bare_number_is_read_by_gid_with_source_asana() {
    assert_eq!(
        locator("120000000000042", Some("asana"), &[]).expect("asana"),
        Locator { source: Source::Asana, key: "120000000000042".into(), url: None }
    );
}

#[test]
fn ambiguous_keys_need_a_source_and_urls_cannot_disagree_with_it() {
    assert!(found("ENG-7", &[Source::Jira, Source::Linear]).is_err());
    assert_eq!(
        locator("ENG-7", Some("jira"), &[]).expect("jira"),
        Locator { source: Source::Jira, key: "ENG-7".into(), url: None }
    );
    assert!(locator("https://github.com/acme/shop/issues/7", Some("jira"), &[]).is_err());
}

#[rstest]
#[case::hash_on_github("#7", &[Source::Github, Source::Linear], Some((Source::Github, "7")))]
#[case::hash_on_gitlab("#7", &[Source::Gitlab, Source::Shortcut], Some((Source::Gitlab, "7")))]
#[case::hash_without_a_repository("#7", &[Source::Linear], None)]
#[case::number_on_github_with_shortcut("482", &[Source::Github, Source::Shortcut], None)]
#[case::number_on_github("482", &[Source::Github, Source::Jira], Some((Source::Github, "482")))]
#[case::number_outside_a_repository("482", &[Source::Shortcut], Some((Source::Shortcut, "482")))]
#[case::story_prefix("sc-482", &[Source::Github], Some((Source::Shortcut, "482")))]
#[case::key_with_one_tracker("ENG-7", &[Source::Github, Source::Jira], Some((Source::Jira, "ENG-7")))]
#[case::key_without_a_tracker("ENG-7", &[Source::Github, Source::Shortcut], None)]
#[case::key_with_only_asana("ENG-7", &[Source::Github, Source::Asana], Some((Source::Asana, "ENG-7")))]
#[case::key_between_linear_and_asana("ENG-7", &[Source::Linear, Source::Asana], None)]
#[case::number_is_never_asana("120000000000042", &[Source::Asana], None)]
#[case::number_with_a_repository_is_not_asana("482", &[Source::Github, Source::Asana], Some((Source::Github, "482")))]
fn a_bare_key_goes_to_the_only_tracker_that_reads_it(
    #[case] text: &str,
    #[case] available: &[Source],
    #[case] expected: Option<(Source, &str)>,
) {
    assert_eq!(found(text, available).ok(), expected.map(|(source, key)| (source, key.to_string())));
}

#[rstest]
#[case::same("https://github.com/acme/shop/issues/7", "https://github.com/acme/shop/issues/7", true)]
#[case::case_and_slug(
    "https://linear.app/Acme/issue/ENG-7/login",
    "https://linear.app/acme/issue/ENG-7/login-fix",
    true
)]
#[case::query(
    "https://acme.atlassian.net/browse/ENG-7?focusedCommentId=1",
    "https://acme.atlassian.net/browse/ENG-7",
    true
)]
#[case::gitlab_work_item(
    "https://gitlab.com/acme/shop/-/work_items/7",
    "https://gitlab.com/acme/shop/-/issues/7",
    true
)]
#[case::another_repository("https://github.com/other/shop/issues/7", "https://github.com/acme/shop/issues/7", false)]
#[case::another_site("https://client.atlassian.net/browse/OPS-12", "https://acme.atlassian.net/browse/OPS-12", false)]
#[case::another_workspace("https://app.plane.so/other/browse/ENG-7/", "https://app.plane.so/acme/browse/ENG-7/", false)]
#[case::another_gitlab_project(
    "https://gitlab.com/acme/api/-/issues/7",
    "https://gitlab.com/acme/shop/-/issues/7",
    false
)]
fn a_url_must_name_the_issue_the_connected_tracker_found(#[case] given: &str, #[case] found: &str, #[case] same: bool) {
    assert_eq!(place(given) == place(found), same);
}

#[rstest]
#[case::the_other_format("https://app.asana.com/0/34/56", "https://app.asana.com/1/12/project/34/task/56", true)]
#[case::without_the_project(
    "https://app.asana.com/1/12/task/56",
    "https://app.asana.com/1/12/project/34/task/56",
    true
)]
#[case::another_task("https://app.asana.com/0/34/57", "https://app.asana.com/1/12/project/34/task/56", false)]
#[case::no_task("https://app.asana.com/0/34", "https://app.asana.com/1/12/project/34/task/56", false)]
fn an_asana_link_names_the_task_the_connected_account_found(
    #[case] given: &str,
    #[case] found: &str,
    #[case] same: bool,
) {
    assert_eq!(same_issue(Source::Asana, given, found), same);
}
