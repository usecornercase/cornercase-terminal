use rstest::rstest;

use super::{Row, Shelf, merge, section};
use crate::control::usage::{LimitInfo, PlanInfo};

const CLOCK: i64 = 1_791_158_400;

fn claude(identity: Option<&str>, percent: u16, age: u64) -> PlanInfo {
    PlanInfo {
        agent: "claude".into(),
        title: "Claude Code".into(),
        age: Some(age),
        limits: vec![LimitInfo { label: "week".into(), percent, severity: "normal".into(), ..LimitInfo::default() }],
        identity: identity.map(str::to_string),
        ..PlanInfo::default()
    }
}

fn shelf(name: &str, rows: Vec<PlanInfo>) -> Shelf {
    Shelf { name: name.into(), rows, ..Shelf::default() }
}

fn on(rows: &[Vec<Row>]) -> Vec<Vec<Vec<&str>>> {
    rows.iter().map(|rows| rows.iter().map(|row| row.on.iter().map(String::as_str).collect()).collect()).collect()
}

fn titles(rows: &[Row]) -> Vec<(String, Option<String>)> {
    rows.iter().map(|row| row.section(CLOCK)).map(|section| (section.title, section.on)).collect()
}

fn titled(title: &str, on: Option<&str>) -> (String, Option<String>) {
    (title.into(), on.map(str::to_string))
}

mod rows_of_two_machines {
    use super::*;

    #[test]
    fn merge_under_the_first_when_both_give_the_same_identity() {
        let shelves =
            [shelf("this computer", vec![claude(Some("a"), 40, 300)]), shelf("vps", vec![claude(Some("a"), 55, 60)])];

        let rows = merge(&shelves);

        assert_eq!(on(&rows), [vec![vec!["this computer", "vps"]], Vec::new()]);
        assert_eq!(titles(&rows[0]), [titled("Claude Code", Some("on this computer, vps"))]);
        assert_eq!(rows[0][0].section(CLOCK).status, "updated 1m ago");
    }

    #[rstest]
    #[case::a_different_identity(Some("a"), Some("b"))]
    #[case::no_identity_on_one_side(Some("a"), None)]
    #[case::no_identity_on_the_other(None, Some("a"))]
    #[case::no_identity_at_all(None, None)]
    fn stay_apart_otherwise(#[case] here: Option<&str>, #[case] there: Option<&str>) {
        let shelves = [shelf("this computer", vec![claude(here, 40, 300)]), shelf("vps", vec![claude(there, 55, 60)])];

        let rows = merge(&shelves);

        assert_eq!(on(&rows), [vec![vec!["this computer"]], vec![vec!["vps"]]]);
        assert_eq!(titles(&rows[1]), [titled("Claude Code", None)]);
    }

    #[test]
    fn keep_the_percentages_of_the_latest_reading_without_adding_them() {
        let shelves =
            [shelf("this computer", vec![claude(Some("a"), 40, 60)]), shelf("vps", vec![claude(Some("a"), 55, 300)])];

        let rows = merge(&shelves);

        let section = rows[0][0].section(CLOCK);
        assert_eq!((section.windows[0].percent, section.status.as_str()), (40, "updated 1m ago"));
    }

    #[test]
    fn keep_the_label_of_the_first_machine() {
        let labelled = PlanInfo { label: Some("work".into()), ..claude(Some("a"), 40, 300) };
        let shelves = [shelf("this computer", vec![labelled]), shelf("vps", vec![claude(Some("a"), 55, 60)])];

        assert_eq!(titles(&merge(&shelves)[0]), [titled("Claude Code · work", Some("on this computer, vps"))]);
    }
}

mod rows_of_one_machine {
    use super::*;

    #[test]
    fn never_merge_even_with_the_same_identity() {
        let shelves = [shelf("vps", vec![claude(Some("a"), 40, 60), claude(Some("a"), 55, 60)])];

        assert_eq!(on(&merge(&shelves)), [vec![vec!["vps"], vec!["vps"]]]);
    }

    #[test]
    fn each_merge_with_a_different_row_of_another_machine() {
        let twice = vec![claude(Some("a"), 40, 60), claude(Some("a"), 55, 60)];
        let shelves = [shelf("this computer", vec![claude(Some("a"), 10, 60)]), shelf("vps", twice)];

        assert_eq!(on(&merge(&shelves)), [vec![vec!["this computer", "vps"]], vec![vec!["vps"]]]);
    }
}

mod a_section {
    use super::*;

    #[test]
    fn says_when_its_limits_reset_and_how_severe_they_are() {
        let limit = LimitInfo {
            label: "session (5h)".into(),
            percent: 95,
            severity: "critical".into(),
            resets_at: Some("2026-10-05T01:00:00Z".into()),
            ..LimitInfo::default()
        };
        let info = PlanInfo { plan: Some("max".into()), limits: vec![limit], ..claude(None, 0, 30) };

        let section = section(&info, CLOCK);

        let window = &section.windows[0];
        assert_eq!(
            (section.title.as_str(), section.status.as_str(), window.resets.as_str(), window.severity),
            ("Claude Code · max plan", "updated just now", "resets in 1h 0m", crate::agents::usage::Severity::Critical)
        );
    }
}
