use rstest::rstest;

use super::*;

fn at(secs: u64, millis: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs) + Duration::from_millis(millis)
}

mod stamp {
    use super::*;

    #[rstest]
    #[case::the_epoch(0, "1970-01-01T00:00:00.000Z")]
    #[case::a_leap_day(951_782_400, "2000-02-29T00:00:00.000Z")]
    #[case::new_year(1_798_761_599, "2026-12-31T23:59:59.000Z")]
    #[case::far_ahead(4_102_444_800, "2100-01-01T00:00:00.000Z")]
    fn is_utc(#[case] secs: u64, #[case] shown: &str) {
        assert_eq!(Stamp(at(secs, 0)).to_string(), shown);
    }

    #[rstest]
    #[case::the_epoch(0, 0)]
    #[case::a_leap_day(951_782_400, 7)]
    #[case::new_year(1_798_761_599, 999)]
    #[case::far_ahead(4_102_444_800, 120)]
    fn reads_back_what_it_writes(#[case] secs: u64, #[case] millis: u64) {
        let shown = Stamp(at(secs, millis)).to_string();

        assert_eq!(Stamp::parse(&shown), Some(at(secs, millis)));
    }

    #[rstest]
    #[case::seconds_only("2026-10-08T20:23:20Z", Some(at(1_791_491_000, 0)))]
    #[case::microseconds("2026-10-08T20:23:20.000749Z", Some(at(1_791_491_000, 0) + Duration::from_micros(749)))]
    #[case::an_offset("2026-10-08T22:23:20.749+02:00", None)]
    #[case::no_time("2026-10-08", None)]
    #[case::a_thirteenth_month("2026-13-08T20:23:20.749Z", None)]
    #[case::a_sign("2026-10-08T20:23:+2.749Z", None)]
    #[case::too_many_parts("2026-10-08-01T20:23:20Z", None)]
    #[case::before_the_epoch("0000-01-01T00:00:00Z", None)]
    #[case::a_year_too_large_for_the_calendar("25252734927766554-01-01T00:00:00Z", None)]
    #[case::empty("", None)]
    fn reads_utc_times_of_other_programs(#[case] text: &str, #[case] expected: Option<SystemTime>) {
        assert_eq!(Stamp::parse(text), expected);
    }
}

mod parse_time {
    use super::*;

    #[rstest]
    #[case::utc("1970-01-02T00:00:10Z", 86_410)]
    #[case::ahead("1970-01-02T02:00:10.000+0200", 86_410)]
    #[case::behind_with_a_colon("1970-01-01T19:30:10-04:30", 86_410)]
    #[case::no_zone("1970-01-02T00:00:10", 86_410)]
    fn parses_the_time_zone_offset(#[case] text: &str, #[case] expected: i64) {
        assert_eq!(parse_time(text), Some(expected));
    }

    #[test]
    fn parses_timestamps_with_or_without_milliseconds() {
        assert_eq!(
            (parse_time("1970-01-02T00:00:10Z"), parse_time("1970-01-02T00:00:10.250Z")),
            (Some(86_410), Some(86_410))
        );
    }
}

mod ago {
    use super::*;

    #[rstest]
    #[case::now(0, "just now")]
    #[case::under_a_minute(59, "just now")]
    #[case::a_minute(60, "1 min ago")]
    #[case::an_hour_short(3599, "59 min ago")]
    #[case::an_hour(3600, "1h ago")]
    #[case::a_day_short(86_399, "23h ago")]
    #[case::a_day(86_400, "1 day ago")]
    #[case::days(86_400 * 3, "3 days ago")]
    fn reads_like_a_person(#[case] secs: u64, #[case] shown: &str) {
        assert_eq!(ago(Duration::from_secs(secs)), shown);
    }
}
