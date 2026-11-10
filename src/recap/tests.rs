use rstest::rstest;

use super::*;

#[rstest]
#[case::now("now", Some(Choice::Now))]
#[case::yesterday("yesterday", Some(Choice::Yesterday))]
#[case::week("week", Some(Choice::Week))]
#[case::seven_days("7 days", Some(Choice::Week))]
#[case::unknown("tomorrow", None)]
fn a_choice_parses_by_its_name(#[case] text: &str, #[case] expected: Option<Choice>) {
    assert_eq!(Choice::parse(text), expected);
}

#[test]
fn a_place_is_named_from_project_to_tab() {
    let place = Place {
        project: Some(Named { label: "api".into(), path: "/p".into() }),
        workspace: Some(Named { label: "rate-limit".into(), path: "/p/w".into() }),
        tab: Some("claude".into()),
        ..Place::default()
    };
    assert_eq!(place.name(), "api › rate-limit › claude");
}

#[rstest]
#[case::waiting(Some("waiting"), true)]
#[case::failed(Some("failed"), true)]
#[case::pending(Some("pending"), true)]
#[case::done(Some("done"), false)]
#[case::working(Some("working"), false)]
#[case::idle(Some("idle"), false)]
#[case::none(None, false)]
fn a_status_needs_you(#[case] status: Option<&str>, #[case] expected: bool) {
    assert_eq!(needs_you(status), expected);
}

#[rstest]
#[case::open_failing(Some("open"), Some("failing"), true)]
#[case::draft_failing(Some("draft"), Some("failing"), true)]
#[case::merged_failing(Some("merged"), Some("failing"), false)]
#[case::open_passing(Some("open"), Some("passing"), false)]
fn failing_checks_need_you_on_an_open_pull_request(
    #[case] state: Option<&str>,
    #[case] checks: Option<&str>,
    #[case] expected: bool,
) {
    assert_eq!(failing(state, checks), expected);
}
