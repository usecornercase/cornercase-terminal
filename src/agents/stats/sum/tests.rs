use std::time::{Duration, UNIX_EPOCH};

use rstest::rstest;

use super::*;
use crate::agents::Known;
use crate::agents::stats::{Counts, MergedAccount, Stats};

fn at(hours: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(hours * 3_600)
}

const NOW: u64 = 1_000;

fn counted(since: u64, adds: &[(u64, Known, Counts)]) -> Stats {
    let mut stats = Stats::new(at(since));
    for &(hour, kind, counts) in adds {
        stats.add(at(hour), kind, counts);
    }
    stats
}

fn work(prompts: u64, agent_ms: u64) -> Counts {
    Counts { prompts, agent_ms, ..Counts::default() }
}

fn with_peak(stats: &Stats, peak: u64) -> View {
    let mut view = stats.view(at(NOW));
    for window in view.windows.values_mut() {
        window.most_at_once = peak;
    }
    view
}

fn this_computer() -> Stats {
    counted(800, &[(NOW, Known::Claude, work(2, 3_000)), (NOW - 24, Known::Claude, work(1, 1_000))])
}

fn vps() -> Stats {
    counted(500, &[(NOW, Known::Codex, work(5, 1_000)), (NOW - 200, Known::Codex, work(4, 2_000))])
}

mod a_sum {
    use super::*;

    #[test]
    fn adds_every_window_and_agent_and_keeps_the_largest_peak() {
        let sum = View::sum(&[with_peak(&this_computer(), 4), with_peak(&vps(), 2)], at(NOW));

        let day = &sum.windows["24h"];
        assert_eq!((day.counts.prompts, day.counts.agent_seconds, day.most_at_once), (7, 4.0, 4));
        assert_eq!((day.by_agent["claude"].prompts, day.by_agent["codex"].prompts), (2, 5));
        assert_eq!((sum.windows["7d"].counts.prompts, sum.windows["7d"].counts.agent_seconds), (8, 5.0));
        assert_eq!((sum.windows["total"].counts.prompts, sum.windows["total"].most_at_once), (12, 4));
    }

    #[test]
    fn counts_from_the_oldest_start() {
        let sum = View::sum(&[this_computer().view(at(NOW)), vps().view(at(NOW))], at(NOW));

        assert_eq!(sum.since, vps().view(at(NOW)).since);
        assert_eq!(sum.days, 20);
        assert_eq!(sum.counting(), "counting for 20 days");
    }

    #[test]
    fn adds_the_seconds_of_each_block_and_grades_the_sum() {
        let sum = View::sum(&[this_computer().view(at(NOW)), vps().view(at(NOW))], at(NOW));

        let seconds: Vec<f64> = sum.blocks.iter().take(9).map(|b| b.agent_seconds).collect();
        assert_eq!(seconds, [4.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0]);
        assert_eq!(sum.blocks.len(), BLOCKS);
        assert_eq!(sum.levels(2)[..9], [3, 1, 0, 0, 0, 0, 0, 0, 2]);
    }

    #[test]
    fn shares_as_one_computer_that_counted_everything() {
        let everything = counted(
            500,
            &[
                (NOW, Known::Claude, work(2, 3_000)),
                (NOW - 24, Known::Claude, work(1, 1_000)),
                (NOW, Known::Codex, work(5, 1_000)),
                (NOW - 200, Known::Codex, work(4, 2_000)),
            ],
        );

        let sum = View::sum(&[this_computer().view(at(NOW)), vps().view(at(NOW))], at(NOW));

        assert_eq!(sum.share_link(), everything.view(at(NOW)).share_link());
        assert!(sum.share_link().contains("&24h=0.7.4.0.0.0.0.0.0.0&"), "{}", sum.share_link());
    }

    #[rstest]
    #[case::all_empty(&[true, true], true)]
    #[case::one_counted(&[true, false], false)]
    #[case::nothing_to_sum(&[], true)]
    fn is_empty_only_when_every_view_is(#[case] empty: &[bool], #[case] expected: bool) {
        let views: Vec<View> = empty.iter().map(|&empty| View { empty, ..View::default() }).collect();

        let sum = View::sum(&views, at(NOW));

        assert_eq!(sum.empty, expected);
        assert_eq!(sum.rows().len(), 9);
    }
}

mod merged_pull_requests {
    use super::*;

    fn account(provider: &str, login: &str, error: Option<&str>) -> MergedAccount {
        MergedAccount {
            provider: provider.into(),
            host: "github.com".into(),
            login: login.into(),
            error: error.map(Into::into),
        }
    }

    fn row(total: u64, accounts: Vec<MergedAccount>) -> MergedRow {
        MergedRow { day: 1, week: 2, total, complete: true, accounts }
    }

    fn view(merged: Option<MergedRow>) -> View {
        View { merged, ..View::default() }
    }

    #[rstest]
    #[case::the_same_accounts(
        Some(row(9, vec![account("github", "ana", None), account("gitlab", "ana", None)])),
        Some(61)
    )]
    #[case::in_another_order_or_failing(
        Some(row(9, vec![account("gitlab", "ana", Some("offline")), account("github", "ana", None)])),
        Some(61)
    )]
    #[case::another_login(Some(row(9, vec![account("github", "bob", None), account("gitlab", "ana", None)])), None)]
    #[case::one_account_more(
        Some(row(
            9,
            vec![account("github", "ana", None), account("gitlab", "ana", None), account("github", "bob", None)]
        )),
        None
    )]
    #[case::a_machine_without_the_row(None, Some(61))]
    fn keep_the_first_row_only_when_every_machine_lists_the_same_accounts(
        #[case] other: Option<MergedRow>,
        #[case] expected: Option<u64>,
    ) {
        let first = Some(row(61, vec![account("github", "ana", None), account("gitlab", "ana", None)]));

        let sum = View::sum(&[view(first), view(other)], at(NOW));

        assert_eq!(sum.merged.map(|m| m.total), expected);
    }

    #[test]
    fn are_missing_when_no_machine_gives_them() {
        assert_eq!(View::sum(&[view(None), view(None)], at(NOW)).merged, None);
    }
}

mod a_view_from_another_machine {
    use super::*;

    #[rstest]
    #[case::counted(this_computer(), false)]
    #[case::nothing_counted(counted(800, &[]), true)]
    fn is_empty_only_when_nothing_was_counted_there(#[case] stats: Stats, #[case] empty: bool) {
        let text = serde_json::to_string(&stats.view(at(NOW))).expect("a view");
        let view: View = serde_json::from_str(&text).expect("a view");

        let settled = view.settled(at(NOW));

        assert_eq!((settled.empty, settled.days), (empty, 8));
    }
}

mod a_copy_read_days_ago {
    use super::*;

    #[rstest]
    #[case::within_a_day(NOW + 23, [3.0, 1.0, 0.0, 0.0])]
    #[case::two_days_on(NOW + 50, [0.0, 0.0, 3.0, 1.0])]
    #[case::longer_than_the_heatmap(NOW + 24 * 400, [0.0; 4])]
    fn moves_its_blocks_by_the_whole_days_since_and_keeps_its_windows(#[case] now: u64, #[case] first: [f64; 4]) {
        let moved = this_computer().view(at(NOW)).moved_on(at(NOW), at(now));

        let seconds: Vec<f64> = moved.blocks.iter().take(4).map(|b| b.agent_seconds).collect();
        assert_eq!((seconds, moved.blocks.len()), (first.to_vec(), BLOCKS));
        assert_eq!(moved.windows["24h"].counts.prompts, 2);
    }
}
