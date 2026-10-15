use rstest::rstest;

use super::*;
use crate::test_util::TempDir;

const T0: i64 = 1_760_000_000;

fn after_days(days: u32) -> Saved {
    Saved { days, ..Saved::default() }
}

mod schedule {
    use super::*;

    #[rstest]
    #[case::fresh(0, false)]
    #[case::thirteen_days(13, false)]
    #[case::fourteen_days(14, true)]
    #[case::more(40, true)]
    fn the_first_ask_waits_for_active_days(#[case] days: u32, #[case] due: bool) {
        assert_eq!(after_days(days).due(T0), due);
    }

    #[rstest]
    #[case::next_day(DAY, false)]
    #[case::eighty_nine_days(89 * DAY, false)]
    #[case::ninety_days(LATER_FOR, true)]
    fn an_ask_that_got_no_answer_comes_back_after_ninety_days(#[case] later: i64, #[case] due: bool) {
        let mut saved = after_days(14);
        saved.shown(T0);
        assert_eq!(saved.due(T0 + later), due);
    }

    #[rstest]
    #[case::just_later(LATER_FOR - 1, false)]
    #[case::ninety_days(LATER_FOR, true)]
    fn later_asks_again_after_ninety_days(#[case] later: i64, #[case] due: bool) {
        let mut saved = after_days(14);
        saved.shown(T0);
        saved.answered(Answer::Later);
        assert_eq!(saved.due(T0 + later), due);
        assert!(!saved.settled());
    }

    #[rstest]
    #[case::starred(Answer::Starred)]
    #[case::never(Answer::Never)]
    fn a_permanent_answer_never_asks_again(#[case] answer: Answer) {
        let mut saved = after_days(14);
        saved.shown(T0);
        saved.answered(answer);
        assert!(saved.settled());
        assert!(!saved.due(T0 + 3650 * DAY));
    }
}

mod counting {
    use super::*;

    #[test]
    fn a_day_counts_once() {
        let mut saved = Saved::default();
        assert!(saved.count(T0));
        assert!(!saved.count(T0 + 60));
        assert_eq!(saved.days, 1);
    }

    #[test]
    fn the_next_day_counts() {
        let mut saved = Saved::default();
        saved.count(T0);
        assert!(saved.count(T0 + DAY));
        assert_eq!(saved.days, 2);
    }

    #[test]
    fn a_clock_set_back_does_not_count_again() {
        let mut saved = Saved::default();
        saved.count(T0);
        saved.count(T0 + DAY);
        assert!(!saved.count(T0));
        assert_eq!(saved.days, 2);
    }

    #[test]
    fn the_first_count_sets_since() {
        let mut saved = Saved::default();
        saved.count(T0);
        saved.count(T0 + 5 * DAY);
        assert_eq!(saved.since, Some(T0));
    }
}

mod file {
    use super::*;

    #[test]
    fn the_fixture_pins_the_format() {
        let text = crate::test_util::fixture!("star/star.json");
        let saved = serde_json::from_str::<Saved>(text).expect("fixture");
        assert_eq!(
            saved,
            Saved {
                version: 1,
                since: Some(1_760_000_000),
                days: 14,
                day: Some(1_761_200_000),
                shown: Some(1_761_210_000),
                answer: Some(Answer::Later),
            }
        );
        let dir = TempDir::new();
        let path = dir.path().join("star.json");
        save(&path, &saved).expect("saved");
        assert_eq!(std::fs::read_to_string(path).expect("read").trim_end(), text.trim_end());
    }

    #[test]
    fn a_missing_file_gives_the_default() {
        let dir = TempDir::new();
        assert_eq!(load(&dir.path().join("star.json")), Saved::default());
    }

    #[rstest]
    #[case::corrupt("not json")]
    #[case::another_version(r#"{"version": 2, "days": 20}"#)]
    fn a_file_that_cannot_be_used_gives_the_default(#[case] text: &str) {
        let dir = TempDir::new();
        let path = dir.path().join("star.json");
        std::fs::write(&path, text).expect("written");
        assert_eq!(load(&path), Saved::default());
    }
}
