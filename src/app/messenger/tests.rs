use std::time::{Duration, Instant};

use rstest::rstest;

use super::{ASK_FOR, Messenger, Route};
use crate::messenger::Via;
use crate::test_util::TempDir;

mod open_ask {
    use super::*;

    fn asked(dir: &TempDir, at: Instant) -> Messenger {
        let mut messenger =
            Messenger::new(Via::Discord, &dir.path().join("config.json"), dir.path().join("secrets.json"));
        messenger.open_ask = Some((Route::Prompt(1), at));
        messenger
    }

    #[rstest]
    #[case::a_second_before_it_expires(ASK_FOR.saturating_sub(Duration::from_secs(1)), Some(Route::Prompt(1)))]
    #[case::once_ten_minutes_passed(ASK_FOR, None)]
    fn an_open_question_lasts_ten_minutes(#[case] later: Duration, #[case] expected: Option<Route>) {
        let dir = TempDir::new();
        let at = Instant::now();
        assert_eq!(asked(&dir, at).take_open_ask(at + later), expected);
    }

    #[test]
    fn an_open_question_is_answered_once() {
        let dir = TempDir::new();
        let at = Instant::now();
        let mut messenger = asked(&dir, at);
        messenger.take_open_ask(at);
        assert_eq!(messenger.take_open_ask(at), None);
    }
}

mod note {
    use super::*;
    use crate::discord::{NO_USER_INSTALL, PUBLIC};

    #[rstest]
    #[case::connected(None, false, "connected")]
    #[case::away(None, true, "connected · away: agents message you there")]
    #[case::advice(Some(PUBLIC), false, PUBLIC)]
    #[case::away_and_advice(Some(PUBLIC), true, "away · public app: Install Link to None, then Public Bot off")]
    #[case::away_and_user_install(Some(NO_USER_INSTALL), true, "away · turn on User Install in the Developer Portal")]
    fn the_bot_row_says_away_and_the_advice_together(
        #[case] advice: Option<&'static str>,
        #[case] away: bool,
        #[case] expected: &str,
    ) {
        let dir = TempDir::new();
        let mut messenger =
            Messenger::new(Via::Discord, &dir.path().join("config.json"), dir.path().join("secrets.json"));
        messenger.token = Some("t".into());
        messenger.advice = advice;
        messenger.away = away;

        let room = 102 - 28 - "cornercase".len() - 2;
        assert_eq!(messenger.note(), expected);
        assert!(expected.chars().count() <= room, "{expected}");
    }
}
