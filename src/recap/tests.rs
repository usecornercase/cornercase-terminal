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
