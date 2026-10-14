use rstest::rstest;

use super::*;

#[rstest]
#[case::github("https://github.com/acme/shop/issues/7", Source::Github, "https://github.com/acme/shop/issues/7")]
#[case::gitlab("https://gitlab.com/acme/shop/-/issues/7", Source::Gitlab, "https://gitlab.com/acme/shop/-/issues/7")]
#[case::shortcut("https://app.shortcut.com/acme/story/482/login", Source::Shortcut, "482")]
#[case::linear("https://linear.app/acme/issue/ENG-7/login", Source::Linear, "ENG-7")]
#[case::jira("https://acme.atlassian.net/browse/ENG-7", Source::Jira, "ENG-7")]
#[case::plane("https://app.plane.so/acme/browse/ENG-7/", Source::Plane, "ENG-7")]
fn reads_every_trackers_url(#[case] url: &str, #[case] source: Source, #[case] key: &str) {
    assert_eq!(locator(url, None, &[]).expect("locator"), (source, key.into()));
}

#[test]
fn ambiguous_keys_need_a_source_and_urls_cannot_disagree_with_it() {
    assert!(locator("ENG-7", None, &[Source::Jira, Source::Linear]).is_err());
    assert_eq!(locator("ENG-7", Some("jira"), &[]).expect("jira"), (Source::Jira, "ENG-7".into()));
    assert!(locator("https://github.com/acme/shop/issues/7", Some("jira"), &[]).is_err());
}
