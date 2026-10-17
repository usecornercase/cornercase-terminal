use rstest::rstest;

use super::*;
use crate::test_util::fixture;

#[rstest]
#[case::dark(fixture!("theme/dark.json"), false)]
#[case::light(fixture!("theme/light.json"), true)]
fn the_documented_fixtures_parse(#[case] text: &str, #[case] light: bool) {
    let theme = parse("fixture", text).expect("a valid theme");
    assert_eq!(theme.is_light(), Some(light));
}

#[test]
fn text_that_is_not_json_names_its_source() {
    let error = parse("colours.json", "nope").expect_err("refused").to_string();
    assert!(error.starts_with("colours.json: "), "{error}");
}

#[test]
fn a_theme_missing_a_colour_names_its_source() {
    let text = r#"{"foreground": [1, 2, 3], "palette": []}"#;
    let error = parse("-", text).expect_err("refused").to_string();
    assert!(error.contains("-: "), "{error}");
}

#[test]
fn a_theme_with_too_few_ansi_colours_says_which_is_missing() {
    let text = r#"{"foreground": [1, 2, 3], "background": [4, 5, 6], "palette": [[0, 0, 0]]}"#;
    let error = parse("t.json", text).expect_err("refused").to_string();
    assert!(error.contains("palette entry 1 is missing"), "{error}");
}

#[rstest]
#[case::client_dark(ThemeFrom::Client, Some(Scheme::Dark), "client colours, dark\n")]
#[case::window_light(ThemeFrom::Window, Some(Scheme::Light), "window colours, light\n")]
#[case::default(ThemeFrom::Default, None, "default colours\n")]
fn the_text_says_where_the_colours_come_from(
    #[case] from: ThemeFrom,
    #[case] scheme: Option<Scheme>,
    #[case] expected: &str,
) {
    assert_eq!(describe(ThemeInfo { from, scheme }), expected);
}
