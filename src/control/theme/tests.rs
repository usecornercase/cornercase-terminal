use libghostty_vt::style::RgbColor;
use rstest::rstest;
use serde_json::json;

use super::*;
use crate::test_util::theme;

const DARK: RgbColor = RgbColor { r: 26, g: 27, b: 38 };
const LIGHT: RgbColor = RgbColor { r: 251, g: 250, b: 246 };

fn without(edit: impl FnOnce(&mut HostTheme)) -> HostTheme {
    let mut theme = theme(DARK);
    edit(&mut theme);
    theme
}

mod request {
    use super::*;

    #[test]
    fn it_round_trips() {
        let request = Theme { theme: Some(Box::new(theme(DARK))), clear: false };
        let text = serde_json::to_string(&request).expect("serialize");
        assert_eq!(serde_json::from_str::<Theme>(&text).expect("parse"), request);
    }

    #[test]
    fn a_serialized_host_theme_reads_back_as_the_request_theme() {
        let mut host = theme(DARK);
        host.truecolor = true;
        let request: Theme = serde_json::from_value(json!({ "theme": host })).expect("parse");
        assert_eq!(request.theme.as_deref(), Some(&host));
    }

    #[test]
    fn its_fields_say_what_was_asked_without_the_colours() {
        let mut fields = Fields(Vec::new());
        Theme { theme: Some(Box::new(theme(DARK))), clear: false }.fields(&mut fields);
        assert_eq!(fields.0, vec![("colours", "true".to_string()), ("clear", "false".to_string())]);
    }
}

mod asked {
    use super::*;

    #[rstest]
    #[case::read(Theme::default(), "read")]
    #[case::set(Theme { theme: Some(Box::new(theme(DARK))), clear: false }, "set")]
    #[case::clear(Theme { theme: None, clear: true }, "clear")]
    fn it_tells_what_to_do(#[case] request: Theme, #[case] expected: &str) {
        let got = match request.asked().expect("valid") {
            Asked::Read => "read",
            Asked::Set(_) => "set",
            Asked::Clear => "clear",
        };
        assert_eq!(got, expected);
    }

    #[test]
    fn colours_and_clear_together_are_refused() {
        let request = Theme { theme: Some(Box::new(theme(DARK))), clear: true };
        assert!(request.asked().expect_err("refused").contains("not both"));
    }

    #[test]
    fn a_theme_that_fails_the_check_is_refused() {
        let request = Theme { theme: Some(Box::new(without(|t| t.background = None))), clear: false };
        assert!(request.asked().expect_err("refused").contains("background"));
    }
}

mod check {
    use super::*;

    #[rstest]
    #[case::foreground(without(|t| t.foreground = None), "foreground")]
    #[case::background(without(|t| t.background = None), "background")]
    #[case::entry_nine(without(|t| t.palette[9] = None), "palette entry 9 is missing")]
    #[case::eight_entries(without(|t| t.palette[8..].fill(None)), "palette entry 8 is missing")]
    fn a_theme_missing_a_part_is_refused(#[case] theme: HostTheme, #[case] expected: &str) {
        assert!(check(&theme).expect_err("refused").contains(expected));
    }

    #[test]
    fn sixteen_colours_pass() {
        assert_eq!(check(&theme(DARK)), Ok(()));
    }

    #[test]
    fn the_colours_past_fifteen_are_optional() {
        let full = without(|t| t.palette[200] = Some(LIGHT));
        assert_eq!(check(&full), Ok(()));
    }
}

mod info {
    use super::*;

    #[test]
    fn it_writes_where_the_colours_come_from_and_their_scheme() {
        let dark = ThemeInfo::of(ThemeFrom::Client, &theme(DARK));
        assert_eq!(serde_json::to_value(dark).expect("json"), json!({"from": "client", "scheme": "dark"}));
        let none = ThemeInfo::of(ThemeFrom::Default, &HostTheme::default());
        assert_eq!(serde_json::to_value(none).expect("json"), json!({"from": "default", "scheme": null}));
    }

    #[test]
    fn a_light_background_is_light() {
        assert_eq!(ThemeInfo::of(ThemeFrom::Window, &theme(LIGHT)).scheme, Some(Scheme::Light));
    }

    #[test]
    fn an_answer_without_keys_reads_as_the_default() {
        let info: ThemeInfo = serde_json::from_value(json!({})).expect("parse");
        assert_eq!(info, ThemeInfo::default());
    }
}
