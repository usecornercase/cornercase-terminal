use rstest::rstest;

use super::*;
use crate::bots::sample;

#[rstest]
#[case::by_id("prs look", Some(("prs", "look")))]
#[case::by_name_ignoring_case("pRs look", Some(("prs", "look")))]
#[case::a_name_with_spaces("my reviews hi there", Some(("reviews", "hi there")))]
#[case::the_longest_name("my bot hi", Some(("my-bot", "hi")))]
#[case::only_the_name("prs", Some(("prs", "")))]
#[case::nobody("nobody hi", None)]
fn a_bot_is_found_by_the_longest_name_that_leads(#[case] args: &str, #[case] wanted: Option<(&str, &str)>) {
    let bots = [sample("prs", "PRs"), sample("reviews", "My reviews"), sample("my", "My"), sample("my-bot", "My bot")];
    let bots: Vec<&Bot> = bots.iter().collect();
    assert_eq!(named(args, &bots).map(|(bot, text)| (bot.id.as_str(), text)), wanted);
}

#[test]
fn an_answer_replies_to_the_newest_message_it_answers_and_forgets_them() {
    let mut asks = Asks::default();
    asks.push(1, 10, 100);
    asks.push(1, 11, 101);
    asks.push(2, 10, 200);
    assert_eq!(asks.take(1, &[10, 11]), Some(101));
    assert_eq!(asks.take(1, &[10, 11]), None);
    assert_eq!(asks.take(2, &[10]), Some(200));
}

#[test]
fn only_the_last_asks_are_kept() {
    let mut asks = Asks::default();
    for line in 0..=64 {
        asks.push(1, line, i64::try_from(line).expect("small"));
    }
    assert_eq!(asks.take(1, &[0]), None);
    assert_eq!(asks.take(1, &[64]), Some(64));
}

#[rstest]
#[case::away_while_away(ChatApps::Away, true, true)]
#[case::away_at_the_desk(ChatApps::Away, false, false)]
#[case::always(ChatApps::Always, false, true)]
#[case::off(ChatApps::Off, true, false)]
fn the_option_says_when_an_unasked_answer_is_sent(#[case] chat_apps: ChatApps, #[case] away: bool, #[case] sent: bool) {
    assert_eq!(wanted(chat_apps, away), sent);
}
