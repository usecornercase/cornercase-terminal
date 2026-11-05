use rstest::rstest;

use super::*;

const MADRID: &str = "Europe/Madrid";

fn at(text: &str) -> i64 {
    text.parse::<Timestamp>().expect("timestamp").as_second()
}

fn repeat(text: &str) -> Repeat {
    Repeat::parse(text, MADRID, at("2026-10-14T08:03:00Z")).expect("repeat")
}

#[test]
fn captured_clock_drops_the_seconds() {
    let parsed = Repeat::parse("3d", MADRID, at("2026-10-14T08:03:27Z")).expect("repeat");
    assert_eq!(parsed.to_string(), "3d 10:03");
    assert!(parsed.accepts("3d 10:03"));
}

fn shown(seconds: i64) -> String {
    clock::display(seconds, MADRID)
}

#[rstest]
#[case::minutes("30m", "30m")]
#[case::hours("2h", "2h")]
#[case::captured_days("3d", "3d 10:03")]
#[case::weeks("2w 9:30", "2w 09:30")]
#[case::day("every day at 09:00", "day 09:00")]
#[case::weekday("Weekday 9:00", "weekday 09:00")]
#[case::named("every Monday 09:00", "monday 09:00")]
fn parses_and_shows_the_canonical_text(#[case] text: &str, #[case] canonical: &str) {
    let parsed = repeat(text);
    assert_eq!(parsed.to_string(), canonical);
    assert_eq!(Repeat::try_from(canonical.to_string()).expect("canonical"), parsed);
}

#[rstest]
#[case::too_often_29("29m")]
#[case::too_often_10("10m")]
#[case::zero("0h")]
#[case::unit("2x")]
#[case::hour("day 25:00")]
#[case::name("funday 09:00")]
#[case::clock_on_hours("2h 09:00")]
#[case::too_rare("400d")]
#[case::overflow("99999999999h")]
#[case::empty("")]
#[case::bare_day("day")]
fn refuses_what_is_not_a_repeat(#[case] text: &str) {
    assert!(Repeat::parse(text, MADRID, 0).is_err(), "{text}");
}

#[test]
fn says_why_it_refuses_a_short_or_a_long_gap() {
    assert!(Repeat::parse("29m", MADRID, 0).expect_err("short").contains("30 minutes"));
    assert!(Repeat::parse("400d", MADRID, 0).expect_err("long").contains("once a year"));
}

#[test]
fn counts_hours_as_absolute_time_across_a_clock_change() {
    let two_hours = repeat("2h");
    let first = at("2026-03-28T22:00:00Z");
    let second = two_hours.after(MADRID, first).expect("second");
    let third = two_hours.after(MADRID, second).expect("third");
    assert_eq!((second - first, third - second), (7200, 7200));
}

#[test]
fn keeps_the_local_clock_of_a_daily_routine_across_a_clock_change() {
    let day = repeat("day 09:00");
    let before = at("2026-10-24T07:00:00Z");
    let after = day.after(MADRID, before).expect("next");
    assert_eq!(after, at("2026-10-25T08:00:00Z"));
    assert_eq!(shown(after), "2026-10-25 09:00");
}

#[test]
fn skips_the_weekend_for_weekdays() {
    let friday = at("2026-10-16T07:00:00Z");
    let next = repeat("weekday 09:00").after(MADRID, friday).expect("next");
    assert_eq!(shown(next), "2026-10-19 09:00");
}

#[test]
fn goes_to_the_next_week_when_the_day_has_passed() {
    let monday_late = at("2026-10-12T08:00:00Z");
    let first = repeat("monday 09:00").first(MADRID, monday_late).expect("first");
    assert_eq!(shown(first), "2026-10-19 09:00");
}

#[test]
fn starts_a_daily_routine_today_when_its_time_is_still_ahead() {
    let first = repeat("day 09:00").first(MADRID, at("2026-10-14T06:00:00Z")).expect("first");
    assert_eq!(shown(first), "2026-10-14 09:00");
    let tomorrow = repeat("day 09:00").first(MADRID, at("2026-10-14T08:00:00Z")).expect("tomorrow");
    assert_eq!(shown(tomorrow), "2026-10-15 09:00");
}

#[test]
fn shifts_a_nonexistent_clock_for_one_day_and_comes_back() {
    let day = repeat("day 02:30");
    let eve = at("2026-03-28T01:30:00Z");
    let skipped = day.after(MADRID, eve).expect("spring");
    assert_eq!(shown(skipped), "2026-03-29 03:30");
    let back = day.after(MADRID, skipped).expect("next day");
    assert_eq!(shown(back), "2026-03-30 02:30");
}

#[test]
fn takes_the_earlier_of_a_repeated_clock() {
    let day = repeat("day 02:30");
    let eve = at("2026-10-24T00:30:00Z");
    let repeated = day.after(MADRID, eve).expect("autumn");
    assert_eq!(repeated, at("2026-10-25T00:30:00Z"));
}

#[test]
fn keeps_every_three_days_on_its_own_clock() {
    let three = repeat("3d 10:00");
    let first = at("2026-10-23T08:00:00Z");
    let next = three.after(MADRID, first).expect("next");
    assert_eq!(shown(next), "2026-10-26 10:00");
}

#[test]
fn jumps_over_a_week_of_downtime_with_one_skip() {
    let two_hours = repeat("2h");
    let start = at("2026-10-01T10:00:00Z");
    let now = start + 7 * 86_400 + 1000;
    let walk = two_hours.walk(MADRID, start, now).expect("walk");
    assert!(walk.skipped);
    assert!(walk.last <= now && walk.next > now);
    assert_eq!(walk.next - walk.last, 7200);
}

#[test]
fn walks_a_civil_routine_to_the_last_occurrence() {
    let day = repeat("day 09:00");
    let start = at("2026-10-14T07:00:00Z");
    let walk = day.walk(MADRID, start, start + 3 * 86_400 + 60).expect("walk");
    assert_eq!(walk.last, start + 3 * 86_400);
    assert!(walk.skipped);
    let same = day.walk(MADRID, start, start + 60).expect("same");
    assert_eq!((same.last, same.skipped), (start, false));
}

#[test]
fn accepts_the_text_it_was_made_from_with_or_without_its_clock() {
    let three = repeat("3d");
    assert!(three.accepts("3d"));
    assert!(three.accepts("3d 10:03"));
    assert!(!three.accepts("3d 11:00"));
    assert!(!three.accepts("4d"));
}

#[test]
fn a_gap_that_ends_after_midnight_does_not_skip_a_day() {
    let zone = "XXX0YYY,M3.5.0/23,M10.5.0/0";
    let day = Repeat::parse("day 23:30", zone, 0).expect("repeat");
    let eve = at("2026-03-28T23:30:00Z");
    let shifted = day.after(zone, eve).expect("the gap");
    assert_eq!(shifted, at("2026-03-29T23:30:00Z"));
    let next = day.after(zone, shifted).expect("the day after");
    assert_eq!(next, at("2026-03-30T22:30:00Z"));
}
