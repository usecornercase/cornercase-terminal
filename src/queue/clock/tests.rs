use rstest::rstest;

use super::*;

fn stamp(text: &str) -> i64 {
    text.parse::<Timestamp>().expect("timestamp").as_second()
}

#[rstest]
#[case::clock_today("14:30", "2026-10-14T12:30:00Z")]
#[case::clock_tomorrow("09:00", "2026-10-15T07:00:00Z")]
#[case::tomorrow("tomorrow 9:00", "2026-10-15T07:00:00Z")]
#[case::date("2026-10-20 09:00", "2026-10-20T07:00:00Z")]
#[case::hours("in 2h", "2026-10-14T12:00:00Z")]
#[case::minutes("in 30m", "2026-10-14T10:30:00Z")]
fn parses_a_client_time_into_an_absolute_instant(#[case] text: &str, #[case] expected: &str) {
    assert_eq!(parse(text, "Europe/Madrid", stamp("2026-10-14T10:00:00Z")).expect("parse"), stamp(expected));
}

#[rstest]
#[case::nonexistent("2026-03-29 02:30")]
#[case::ambiguous("2026-10-25 02:30")]
#[case::bad_day("2026-02-30 09:00")]
#[case::bad_clock("25:00")]
#[case::past("2025-10-14 09:00")]
#[case::zero("in 0m")]
#[case::negative("in -1h")]
#[case::overflow("in 9223372036854775807h")]
#[case::unicode("in ⏰")]
fn rejects_invalid_dates_delays_and_dst_ambiguities(#[case] text: &str) {
    assert!(parse(text, "Europe/Madrid", stamp("2026-01-01T00:00:00Z")).is_err());
}

#[test]
fn uses_tomorrows_offset_across_a_daylight_saving_change() {
    assert_eq!(
        parse("tomorrow 9:00", "Europe/Madrid", stamp("2026-10-24T12:00:00Z")).expect("parse"),
        stamp("2026-10-25T08:00:00Z")
    );
}

#[test]
fn accepts_a_fixed_client_offset_and_displays_it() {
    let now = stamp("2026-10-14T10:00:00Z");
    let at = parse("14:30", "+03:00", now).expect("parse");
    assert_eq!(at, stamp("2026-10-14T11:30:00Z"));
    assert_eq!(display(at, "+03:00"), "2026-10-14 14:30");
}
