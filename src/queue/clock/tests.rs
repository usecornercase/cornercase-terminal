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

#[test]
fn a_bare_time_already_past_today_skips_a_daylight_saving_hour_of_today() {
    assert_eq!(
        parse("02:30", "Europe/Madrid", stamp("2026-10-25T09:00:00Z")).expect("parse"),
        stamp("2026-10-26T01:30:00Z")
    );
}

#[test]
fn a_bare_time_inside_todays_repeated_hour_is_still_ambiguous() {
    assert!(parse("02:30", "Europe/Madrid", stamp("2026-10-24T22:00:00Z")).is_err());
}

#[rstest]
#[case::iana(Some("Europe/Madrid"), None, Some("Europe/Madrid"))]
#[case::colon_path(Some(":/usr/share/zoneinfo/America/New_York"), None, Some("America/New_York"))]
#[case::posix(Some("EST5EDT,M3.2.0,M11.1.0"), None, Some("EST5EDT,M3.2.0,M11.1.0"))]
#[case::etc_timezone(None, Some("Europe/Lisbon\n"), Some("Europe/Lisbon"))]
#[case::invalid_tz_falls_back(Some("Not/AZone"), Some("Asia/Tokyo"), Some("Asia/Tokyo"))]
#[case::nothing(None, None, None)]
fn finds_a_zone_with_its_rules_when_the_system_zone_has_no_name(
    #[case] tz: Option<&str>,
    #[case] etc: Option<&str>,
    #[case] expected: Option<&str>,
) {
    assert_eq!(named([tz, etc]).as_deref(), expected);
}

#[test]
fn a_posix_zone_keeps_its_daylight_saving_rules() {
    let zone = "EST5EDT,M3.2.0,M11.1.0";
    assert_eq!(
        parse("2026-12-01 09:00", zone, stamp("2026-10-14T10:00:00Z")).expect("parse"),
        stamp("2026-12-01T14:00:00Z")
    );
    assert_eq!(
        parse("2026-10-20 09:00", zone, stamp("2026-10-14T10:00:00Z")).expect("parse"),
        stamp("2026-10-20T13:00:00Z")
    );
}
