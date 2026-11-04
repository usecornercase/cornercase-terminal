use rstest::rstest;

use super::*;

fn prompt_at(time: &str) -> Entry {
    let mut entry = Entry::blank("claude");
    entry.at = time.to_string();
    entry.event = Event::Prompt;
    entry
}

fn seconds(time: &str) -> i64 {
    prompt_at(time).seconds().expect("a time")
}

const MADRID: &str = "Europe/Madrid";

#[rstest]
#[case::monday_finds_friday("2026-11-02T09:00:00.000Z", "2026-10-30T15:00:00.000Z", "2026-10-30")]
#[case::after_a_holiday("2026-11-10T09:00:00.000Z", "2026-11-06T15:00:00.000Z", "2026-11-06")]
#[case::late_night_is_next_utc_day("2026-11-04T09:00:00.000Z", "2026-11-03T22:30:00.000Z", "2026-11-03")]
fn yesterday_is_the_last_day_with_agent_activity(#[case] now: &str, #[case] active: &str, #[case] expected: &str) {
    let entries = [prompt_at(active)];
    let range = resolve(Choice::Yesterday, &entries, seconds(now), MADRID).expect("a day");
    assert_eq!(range.day.as_deref(), Some(expected));
    assert!(range.from <= seconds(active) && seconds(active) < range.to);
}

#[test]
fn activity_only_today_leaves_yesterday_without_a_day() {
    let entries = [prompt_at("2026-11-04T08:00:00.000Z")];
    assert!(resolve(Choice::Yesterday, &entries, seconds("2026-11-04T09:00:00.000Z"), MADRID).is_none());
}

#[test]
fn a_status_that_is_not_working_is_not_activity() {
    let mut entry = prompt_at("2026-11-02T08:00:00.000Z");
    entry.event = Event::Status;
    entry.to = Some("waiting".into());
    assert!(resolve(Choice::Yesterday, &[entry], seconds("2026-11-04T09:00:00.000Z"), MADRID).is_none());
}

#[rstest]
#[case::tuesday("2026-11-03T08:48:00.000Z", MADRID, "tue 3 nov 09:48")]
#[case::los_angeles("2026-11-03T08:48:00.000Z", "America/Los_Angeles", "tue 3 nov 00:48")]
fn now_is_labelled_with_the_local_time(#[case] now: &str, #[case] zone: &str, #[case] label: &str) {
    let range = resolve(Choice::Now, &[], seconds(now), zone).expect("now");
    assert_eq!(range.label, label);
    assert_eq!(range.to - range.from, 86_400);
}

#[test]
fn the_yesterday_label_names_the_real_date() {
    let entries = [prompt_at("2026-10-31T10:00:00.000Z")];
    let range = resolve(Choice::Yesterday, &entries, seconds("2026-11-03T09:00:00.000Z"), MADRID).expect("a day");
    assert_eq!(range.label, "yesterday · sat 31 oct");
}

#[test]
fn the_week_is_seven_local_days_ending_now() {
    let now = seconds("2026-11-04T09:00:00.000Z");
    let range = resolve(Choice::Week, &[], now, MADRID).expect("a week");
    assert_eq!(range.to, now);
    let days = (range.to - range.from) as f64 / 86_400.0;
    assert!((6.0..7.0).contains(&days), "{days}");
    assert_eq!(range.label, "7 days");
}
