use rstest::rstest;

use super::*;
use crate::test_util::TempDir;

const T0: i64 = 1_760_000_000;

fn after(prompts: u32) -> Saved {
    Saved { prompts, ..Saved::default() }
}

mod schedule {
    use super::*;

    #[rstest]
    #[case::fresh(0, false)]
    #[case::four(4, false)]
    #[case::five(5, true)]
    #[case::nine(9, true)]
    fn it_is_due_at_the_fifth_prompt(#[case] prompts: u32, #[case] due: bool) {
        assert_eq!(after(prompts).due(), due);
    }

    #[test]
    fn once_shown_it_is_never_due() {
        let mut saved = after(5);
        saved.shown(T0);
        assert!(!saved.due());
        assert!(saved.asked());
    }

    #[rstest]
    #[case::starred(Answer::Starred)]
    #[case::never(Answer::Never)]
    fn an_answer_means_it_is_never_due(#[case] answer: Answer) {
        let mut saved = after(5);
        saved.answered(answer);
        assert!(!saved.due());
    }
}

mod counting {
    use super::*;

    #[test]
    fn prompts_add_up_until_it_is_due() {
        let mut saved = Saved::default();
        assert!(saved.count(3));
        assert!(saved.count(2));
        assert_eq!(saved.prompts, 5);
        assert!(!saved.count(1));
        assert_eq!(saved.prompts, 5);
    }

    #[test]
    fn nothing_counts_once_shown_or_answered() {
        let mut shown = Saved::default();
        shown.shown(T0);
        assert!(!shown.count(2));
        let mut answered = Saved::default();
        answered.answered(Answer::Never);
        assert!(!answered.count(2));
        assert_eq!((shown.prompts, answered.prompts), (0, 0));
    }

    #[test]
    fn no_prompts_change_nothing() {
        let mut saved = Saved::default();
        assert!(!saved.count(0));
        assert_eq!(saved, Saved::default());
    }
}

mod file {
    use super::*;

    #[test]
    fn the_fixture_pins_the_format() {
        let text = crate::test_util::fixture!("star/star.json");
        let saved = serde_json::from_str::<Saved>(text).expect("fixture");
        assert_eq!(saved, Saved { version: 2, prompts: 5, shown: Some(1_761_210_000), answer: Some(Answer::Starred) });
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
    #[case::a_newer_version(r#"{"version": 3, "prompts": 20}"#)]
    fn a_file_that_cannot_be_used_gives_the_default(#[case] text: &str) {
        let dir = TempDir::new();
        let path = dir.path().join("star.json");
        std::fs::write(&path, text).expect("written");
        assert_eq!(load(&path), Saved::default());
    }

    #[rstest]
    #[case::later(r#""later""#, None)]
    #[case::unanswered("null", None)]
    #[case::starred(r#""starred""#, Some(Answer::Starred))]
    #[case::never(r#""never""#, Some(Answer::Never))]
    fn a_version_1_file_keeps_only_a_permanent_answer(#[case] answer: &str, #[case] kept: Option<Answer>) {
        let text = crate::test_util::fixture!("star/v1.json").replace(r#""later""#, answer);
        let dir = TempDir::new();
        let path = dir.path().join("star.json");
        std::fs::write(&path, text).expect("written");
        assert_eq!(load(&path), Saved { answer: kept, ..Saved::default() });
    }
}
