use rstest::rstest;

use super::*;
use crate::test_util::TempDir;

fn cached(day: Option<&str>, written_at: i64) -> Recap {
    Recap { day: day.map(str::to_string), written_at, text: "## Done\n- [c1] x".into(), ..Recap::default() }
}

#[rstest]
#[case::none_yet(Choice::Now, None, 1000, "2026-11-04", None, true)]
#[case::now_fresh(Choice::Now, Some(cached(None, 900)), 1000, "2026-11-04", None, false)]
#[case::now_stale(Choice::Now, Some(cached(None, 0)), 2000, "2026-11-04", None, true)]
#[case::yesterday_kept(
    Choice::Yesterday,
    Some(cached(Some("2026-11-03"), 0)),
    9_999_999,
    "2026-11-04",
    Some("2026-11-03"),
    false
)]
#[case::yesterday_new_day(
    Choice::Yesterday,
    Some(cached(Some("2026-11-03"), 0)),
    9_999_999,
    "2026-11-05",
    Some("2026-11-04"),
    true
)]
#[case::week_same_day(Choice::Week, Some(cached(Some("2026-11-04"), 0)), 5000, "2026-11-04", None, false)]
#[case::week_next_day(Choice::Week, Some(cached(Some("2026-11-04"), 0)), 5000, "2026-11-05", None, true)]
fn the_caching_rules_decide_when_a_choice_is_written_again(
    #[case] choice: Choice,
    #[case] cached: Option<Recap>,
    #[case] now: i64,
    #[case] today: &str,
    #[case] last_day: Option<&str>,
    #[case] expected: bool,
) {
    assert_eq!(due(choice, cached.as_ref(), Moment { now, today, last_day }), expected);
}

#[test]
fn a_recap_is_saved_as_json_and_markdown_and_loaded_back() {
    let dir = TempDir::new();
    let recap = cached(Some("2026-11-03"), 5);
    save(dir.path(), Choice::Yesterday, &recap).expect("save");
    assert_eq!(load(dir.path(), Choice::Yesterday), Some(recap.clone()));
    let md = std::fs::read_to_string(dir.path().join("yesterday").join("recap.md")).expect("markdown");
    assert_eq!(md, recap.text);
    assert!(load(dir.path(), Choice::Week).is_none());
}
