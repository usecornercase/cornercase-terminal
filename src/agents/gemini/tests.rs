use super::*;

mod gemini_title {
    use super::*;

    #[test]
    fn pinned_titles_report_only_the_state_gemini_supplies() {
        let titles: serde_json::Value =
            serde_json::from_str(crate::test_util::fixture!("gemini/0.63.0/titles.json")).expect("Gemini titles");
        for fixture in titles.as_array().expect("titles array") {
            let expected = match fixture["state"].as_str() {
                Some("working") => Some(Activity::Working),
                Some("waiting") => Some(Activity::Waiting),
                Some("idle") => Some(Activity::Idle),
                _ => None,
            };
            assert_eq!(gemini(fixture["title"].as_str().expect("title")), expected, "{}", fixture["name"]);
        }
        assert_eq!(gemini("unrelated title"), None);
    }
}
