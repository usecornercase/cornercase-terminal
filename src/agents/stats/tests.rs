use std::os::unix::fs::PermissionsExt;

use rstest::rstest;

use super::*;
use crate::test_util::TempDir;

fn at(hours: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(hours * 3_600)
}
fn working(pane: u64, kind: Known) -> Sample {
    Sample {
        pane,
        kind,
        pid: i32::try_from(pane).expect("pid"),
        status: Some(Status::Working),
        background: false,
        resuming: false,
    }
}
fn prompts(stats: &mut Stats, hours: u64, number: usize) {
    stats.counted(Known::Claude, Counted { prompts: vec![at(hours); number], tokens: Vec::new() });
}

#[rstest]
#[case::current_hour(1_000, 3, 7)]
#[case::next_hour(1_001, 1, 3)]
fn windows_include_the_current_hour_and_their_previous_buckets(#[case] now: u64, #[case] day: u64, #[case] week: u64) {
    let mut stats = Stats::new(at(800));
    for (hour, n) in [(832, 8), (833, 4), (977, 2), (1_000, 1)] {
        prompts(&mut stats, hour, n);
    }
    let view = stats.view(at(now));
    assert_eq!(
        (view.windows["24h"].counts.prompts, view.windows["7d"].counts.prompts, view.windows["total"].counts.prompts),
        (day, week, 15)
    );
}
#[test]
fn pruning_keeps_the_total_and_sparse_retention() {
    let mut stats = Stats::new(at(100));
    prompts(&mut stats, 100, 2);
    prompts(&mut stats, 5_000, 3);
    stats.prune(at(5_000));
    assert_eq!((stats.hours.len(), stats.view(at(5_000)).windows["total"].counts.prompts), (1, 5));
    assert_eq!(stats.saved(at(5_000)).hours.len(), 1);
}
#[test]
fn parallelism_is_a_maximum_and_time_keeps_half_seconds_and_caps_stalls() {
    let mut stats = Stats::new(at(1_000));
    let samples = [working(1, Known::Claude), working(2, Known::Codex)];
    stats.sample(at(1_000), Duration::ZERO, &samples);
    stats.sample(at(1_000), Duration::from_millis(500), &samples);
    stats.sample(at(1_000), Duration::from_secs(100), &samples[..1]);
    let w = &stats.view(at(1_000)).windows["total"];
    assert_eq!((w.counts.agents, w.counts.agent_seconds, w.busy_seconds, w.most_at_once), (2, 3.0, 2.5, 2));
    assert_eq!(w.by_agent["codex"].agent_seconds, 0.5);
}
#[test]
fn a_removed_or_replaced_agent_cannot_extend_busy_time() {
    let mut stats = Stats::new(at(1_000));
    let sample = working(1, Known::Claude);
    stats.sample(at(1_000), Duration::ZERO, &[sample]);
    stats.sample(at(1_000), Duration::from_secs(1), &[]);
    let w = &stats.view(at(1_000)).windows["total"];
    assert_eq!((w.counts.agent_seconds, w.busy_seconds), (0.0, 0.0));
    let mut replaced = sample;
    replaced.pid += 1;
    stats.sample(at(1_000), Duration::ZERO, &[sample]);
    stats.sample(at(1_000), Duration::from_secs(1), &[replaced]);
    let w = &stats.view(at(1_000)).windows["total"];
    assert_eq!((w.counts.agent_seconds, w.busy_seconds), (0.0, 0.0));
}

#[test]
fn shells_do_not_work_and_waiting_and_unseen_done_need_you() {
    let mut stats = Stats::new(at(1_000));
    let mut sample = working(1, Known::Claude);
    stats.sample(at(1_000), Duration::ZERO, &[sample]);
    sample.background = true;
    stats.sample(at(1_000), Duration::from_secs(1), &[sample]);
    stats.sample(at(1_000), Duration::from_secs(1), &[sample]);
    sample.background = false;
    sample.status = Some(Status::Waiting);
    stats.sample(at(1_000), Duration::from_secs(1), &[sample]);
    sample.status = Some(Status::Done);
    stats.sample(at(1_000), Duration::from_secs(1), &[sample]);
    stats.sample(at(1_000), Duration::from_secs(1), &[sample]);
    let c = &stats.view(at(1_000)).windows["total"].counts;
    assert_eq!((c.agent_seconds, c.turns, c.waited_seconds), (1.0, 2, 2.0));
}
#[test]
fn automatic_resumes_stay_suppressed_after_a_foreground_absence() {
    let mut stats = Stats::new(at(1_000));
    let mut s = working(1, Known::Claude);
    s.resuming = true;
    stats.sample(at(1_000), Duration::ZERO, &[s]);
    s.resuming = false;
    stats.sample(at(1_000), Duration::ZERO, &[s]);
    assert_eq!(stats.view(at(1_000)).windows["total"].counts.agents, 0);
    stats.sample(at(1_000), Duration::ZERO, &[]);
    assert!(stats.previous.is_empty());
    stats.sample(at(1_000), Duration::ZERO, &[s]);
    assert_eq!(stats.view(at(1_000)).windows["total"].counts.agents, 0);
}

#[test]
fn an_agent_returning_from_an_editor_counts_only_when_its_pid_changes() {
    let mut stats = Stats::new(at(1_000));
    let mut sample = working(1, Known::Claude);
    stats.sample(at(1_000), Duration::ZERO, &[sample]);
    stats.sample(at(1_000), Duration::ZERO, &[]);
    stats.sample(at(1_000), Duration::ZERO, &[sample]);
    assert_eq!(stats.view(at(1_000)).windows["total"].counts.agents, 1);
    sample.pid = 2;
    stats.sample(at(1_000), Duration::ZERO, &[sample]);
    assert_eq!(stats.view(at(1_000)).windows["total"].counts.agents, 2);
}
#[test]
fn heatmap_blocks_go_back_from_the_hour_and_use_nonzero_quartiles() {
    let mut stats = Stats::new(at(1_000 - 5 * 24));
    for (i, ms) in [(0, 1_000), (1, 2_000), (2, 3_000), (3, 4_000)] {
        stats.add(at(1_000 - i * 24), Known::Claude, Counts { agent_ms: ms, ..Counts::default() });
    }
    let view = stats.view(at(1_000));
    assert_eq!(view.blocks.iter().take(5).map(|b| b.agent_seconds).collect::<Vec<_>>(), [1.0, 2.0, 3.0, 4.0, 0.0]);
    assert_eq!(view.levels(1), [1, 2, 3, 4, 0, 0, 0]);
}
#[test]
fn the_first_incomplete_day_has_a_heatmap_square() {
    let mut stats = Stats::new(at(1_000));
    stats.add(at(1_000), Known::Claude, Counts { agent_ms: 1000, ..Counts::default() });
    let view = stats.view(at(1_000));
    assert_eq!(view.days, 0);
    assert_eq!(view.levels(1), [1, 0, 0, 0, 0, 0, 0]);
    assert!(view.share_link().ends_with("&days=0&map=IA"));
}
#[test]
fn blocks_before_counting_began_are_drawn_empty() {
    let mut stats = Stats::new(at(1_000 - 30));
    stats.add(at(1_000), Known::Claude, Counts { agent_ms: 1000, ..Counts::default() });
    stats.add(at(1_000 - 24), Known::Claude, Counts { agent_ms: 3000, ..Counts::default() });
    let view = stats.view(at(1_000));
    assert_eq!(view.levels(WEEKS).len(), BLOCKS);
    assert!(view.levels(WEEKS)[2..].iter().all(|level| *level == 0));
    let text = view.text();
    let map: Vec<&str> = text.lines().skip(13).take(7).collect();
    assert!(map.iter().all(|row| row.chars().count() == WEEKS), "{map:?}");
    assert_eq!(map.iter().map(|row| row.matches('·').count()).sum::<usize>(), BLOCKS - 2);
    assert!(text.ends_with("counting for 1 day"));
}

#[test]
fn saving_observes_revisions_until_a_minute_passes_and_flushes_the_latest_numbers() {
    let dir = TempDir::new();
    let path = dir.path().join("stats.json");
    let mut stats = Stats::new(at(1_000));
    let mut saver = Saver::new(path.clone(), stats.revision());
    let now = Instant::now();
    prompts(&mut stats, 1_000, 1);
    assert!(!saver.observe(&stats, at(1_000), now).expect("observe"));
    for second in 1..60 {
        prompts(&mut stats, 1_000, 1);
        assert!(!saver.observe(&stats, at(1_000), now + Duration::from_secs(second)).expect("observe"));
    }
    assert!(!path.exists());
    assert!(saver.observe(&stats, at(1_000), now + Duration::from_secs(60)).expect("save"));
    assert_eq!(load(&path, at(1_000)).view(at(1_000)).windows["total"].counts.prompts, 60);
    prompts(&mut stats, 1_000, 1);
    saver.flush(&stats, at(1_000)).expect("flush");
    assert_eq!(load(&path, at(1_000)).view(at(1_000)).windows["total"].counts.prompts, 61);
    assert!(!saver.observe(&stats, at(1_000), now + Duration::from_secs(120)).expect("unchanged"));
}

#[test]
fn unchanged_samples_and_repeated_pruning_keep_the_revision() {
    let mut stats = Stats::new(at(1_000));
    let mut sample = working(1, Known::Claude);
    sample.status = Some(Status::Idle);
    stats.sample(at(1_000), Duration::ZERO, &[sample]);
    let revision = stats.revision();
    stats.sample(at(1_000), Duration::from_secs(1), &[sample]);
    stats.prune(at(1_000));
    stats.prune(at(1_000));
    assert_eq!(stats.revision(), revision);
    prompts(&mut stats, 1_000, 1);
    assert!(stats.revision() > revision);
    stats.prune(at(6_000));
    let revision = stats.revision();
    stats.prune(at(6_000));
    assert_eq!(stats.revision(), revision);
    assert!(stats.hours.is_empty());
}

#[test]
fn a_private_file_round_trips_without_transient_process_state() {
    let dir = TempDir::new();
    let path = crate::paths::stats(&dir.path().join("session.json"));
    let mut stats = Stats::new(at(1_000));
    prompts(&mut stats, 1_000, 2);
    let saved = stats.saved(at(1_000));
    save(&path, &saved).expect("save");
    assert_eq!(load(&path, at(1_000)).saved(at(1_000)), saved);
    assert_eq!(std::fs::metadata(&path).expect("metadata").permissions().mode() & 0o777, 0o600);
    assert!(!std::fs::read_to_string(&path).expect("read").contains('\n'));
}
#[rstest]
#[case::corrupt("{")]
#[case::other_version("other-version")]
fn bad_files_start_counting_now(#[case] text: &str) {
    let dir = TempDir::new();
    let path = dir.path().join("stats.json");
    let text = if text == "other-version" {
        let mut saved = Stats::new(at(1)).saved(at(1));
        saved.version = 2;
        serde_json::to_string(&saved).expect("other version")
    } else {
        text.into()
    };
    std::fs::write(&path, text).expect("write");
    let saved = load(&path, at(1_000)).saved(at(1_000));
    assert_eq!((saved.since, saved.hours.len()), (3_600_000, 0));
}
#[test]
fn a_missing_file_starts_counting_now() {
    let stats = load(Path::new("/nonexistent/stats.json"), at(1_000));
    assert_eq!(stats.saved(at(1_000)).since, 3_600_000);
}
#[rstest]
#[case::minutes(1_320.0, "22m")]
#[case::hours(22_320.0, "6h 12m")]
#[case::whole_hours(1_116_000.0, "310h")]
fn durations_use_the_dialog_format(#[case] seconds: f64, #[case] expected: &str) {
    assert_eq!(duration(seconds), expected);
}
#[test]
fn count_and_token_formats_fit_the_table() {
    assert_eq!(
        [count(9_999), count(12_300), tokens(410_000), tokens(3_100_000), tokens(2_400_000_000)],
        ["9999", "12.3k", "410k", "3.1M", "2.4B"]
    );
}
fn first_agent(counts: Counts) -> Vec<Counts> {
    let mut by_agent = vec![Counts::default(); Known::ALL.len()];
    by_agent[0] = counts;
    by_agent
}
pub(crate) fn example(now: SystemTime) -> Stats {
    let fixture: serde_json::Value =
        serde_json::from_str(crate::test_util::fixture!("stats/share-v2.txt")).expect("fixture");
    let mut stats = Stats::new(now.checked_sub(Duration::from_hours(2640)).expect("counting start"));
    let bucket = |name: &str| {
        let a: Vec<u64> =
            fixture[name].as_array().expect("array").iter().map(|v| v.as_u64().expect("integer")).collect();
        Bucket {
            by_agent: first_agent(Counts {
                agents: a[0],
                prompts: a[1],
                turns: a[6],
                agent_ms: a[2] * 1000,
                waited_ms: a[7] * 1000,
                tokens: Tokens { input: a[8], out: a[5], cache_read: a[9], ..Tokens::default() },
            }),
            busy_ms: a[3] * 1000,
            most_at_once: a[4],
        }
    };
    let day = bucket("24h");
    let week = bucket("7d");
    let mut rest = week.clone();
    rest.by_agent[0] = Counts::unpacked(std::array::from_fn(|i| {
        week.by_agent[0].packed()[i].saturating_sub(day.by_agent[0].packed()[i])
    }));
    rest.busy_ms -= day.busy_ms;
    stats.hours.insert(hour(now), day);
    stats.hours.insert(hour(now) - 24, rest);
    for (i, n) in fixture["block_seconds"].as_array().expect("blocks").iter().enumerate().skip(7) {
        stats.hours.insert(
            hour(now) - i as u64 * 24,
            Bucket {
                by_agent: first_agent(Counts { agent_ms: n.as_u64().expect("seconds") * 1000, ..Counts::default() }),
                ..Bucket::default()
            },
        );
    }
    stats.total = bucket("all");
    stats
}
#[test]
fn sharing_matches_the_site_contract() {
    let fixture: serde_json::Value =
        serde_json::from_str(crate::test_util::fixture!("stats/share-v2.txt")).expect("fixture");
    let now = at(10_000);
    let stats = example(now);
    assert_eq!(share_link(&stats, now), fixture["link"].as_str().expect("link"));
}

mod agent_counts {
    use super::*;

    fn counts(entries: usize) -> String {
        let one = r#"{"agents":1,"prompts":2,"turns":3,"agent_ms":4,"waited_ms":5,"tokens":{"in":6,"out":7,"cache_read":8,"cache_write":9}}"#;
        let list = |n: usize| vec![one; n].join(",");
        format!("[{}]", list(entries))
    }

    fn file(entries: usize) -> String {
        let hour = vec!["[1,2,3,4,5,6,7,8,9]"; entries].join(",");
        format!(
            r#"{{"version":1,"since":3600000,"total":{{"by_agent":{},"busy_ms":10,"most_at_once":2}},"hours":[[1000,[{hour}],10,2]]}}"#,
            counts(entries)
        )
    }

    fn loaded(text: &str) -> Saved {
        let dir = TempDir::new();
        let path = dir.path().join("stats.json");
        std::fs::write(&path, text).expect("write");
        load(&path, at(1_000)).saved(at(1_000))
    }

    #[test]
    fn a_file_with_every_agent_is_written_back_unchanged() {
        let text = file(Known::ALL.len());
        let dir = TempDir::new();
        let path = dir.path().join("stats.json");
        std::fs::write(&path, &text).expect("write");
        let saved = load(&path, at(1_000)).saved(at(1_000));
        save(&path, &saved).expect("save");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), text);
    }

    #[test]
    fn missing_agents_load_as_zeros() {
        let saved = loaded(&file(Known::ALL.len() - 1));
        let last = Known::ALL.len() - 1;
        assert_eq!(saved.hours[0].1.len(), Known::ALL.len());
        assert_eq!(saved.hours[0].1[last], [0; 9]);
        assert_eq!(saved.hours[0].1[0], [1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(saved.total.by_agent.len(), Known::ALL.len());
        assert_eq!(saved.total.by_agent[last], Counts::default());
    }

    #[test]
    fn entries_past_the_last_known_agent_are_ignored() {
        let saved = loaded(&file(Known::ALL.len() + 1));
        assert_eq!(saved.hours[0].1.len(), Known::ALL.len());
        assert_eq!(saved.total.by_agent.len(), Known::ALL.len());
        assert_eq!(saved.since, 3_600_000);
    }
}

mod merged_pull_requests {
    use super::*;

    fn row(day: u64, week: u64, total: u64, complete: bool) -> MergedRow {
        MergedRow { day, week, total, complete, accounts: Vec::new() }
    }

    fn view(merged: Option<MergedRow>) -> View {
        let now = at(10_000);
        let mut view = example(now).view(now);
        view.merged = merged;
        view
    }

    #[rstest]
    #[case::the_current_hour(10_000, [1, 1, 1])]
    #[case::the_oldest_hour_of_24(9_977, [1, 1, 1])]
    #[case::just_over_24h(9_976, [0, 1, 1])]
    #[case::the_oldest_hour_of_7d(9_833, [0, 1, 1])]
    #[case::just_over_7d(9_832, [0, 0, 1])]
    #[case::before_counting_began(799, [0, 0, 0])]
    #[case::in_the_future(10_001, [0, 0, 1])]
    fn windows_follow_the_hour_aligned_rule_and_start_at_counting(#[case] hour: u64, #[case] expected: [u64; 3]) {
        assert_eq!(windows([at(hour)].into_iter(), at(800), at(10_000)), expected);
    }

    #[test]
    fn the_row_is_the_last_one_and_complete_counts_have_no_plus() {
        let rows = view(Some(row(2, 9, 61, true))).rows();

        assert_eq!(rows.len(), 10);
        assert_eq!(rows[9], ("merged PRs", ["2".into(), "9".into(), "61".into()]));
    }

    #[test]
    fn a_partial_count_shows_a_plus() {
        let rows = view(Some(row(2, 9, 61, false))).rows();

        assert_eq!(rows[9].1, ["2+", "9+", "61+"]);
        assert!(view(Some(row(2, 9, 61, false))).text().contains("merged PRs"));
    }

    #[test]
    fn without_it_nothing_changes() {
        let view = view(None);

        assert_eq!(view.rows().len(), 9);
        assert!(!view.text().contains("merged PRs"));
    }

    #[test]
    fn json_has_the_key_even_when_empty() {
        let none = serde_json::to_value(view(None)).expect("json");
        let some = serde_json::to_value(view(Some(row(2, 9, 61, true)))).expect("json");

        assert_eq!(none["merged_pull_requests"], serde_json::Value::Null);
        assert_eq!(
            some["merged_pull_requests"],
            serde_json::json!({"24h": 2, "7d": 9, "total": 61, "complete": true, "accounts": []})
        );
    }

    #[test]
    fn an_older_answer_without_the_key_reads_as_none() {
        let mut json = serde_json::to_value(view(None)).expect("json");
        json.as_object_mut().expect("object").remove("merged_pull_requests");

        let back: View = serde_json::from_value(json).expect("view");

        assert_eq!(back.merged, None);
    }

    #[test]
    fn sharing_matches_the_site_contract_with_the_row() {
        let fixture: serde_json::Value =
            serde_json::from_str(crate::test_util::fixture!("stats/share-v3.txt")).expect("fixture");

        let link = view(Some(row(2, 9, 61, true))).share_link();

        assert_eq!(link, fixture["link"].as_str().expect("link"));
    }

    #[test]
    fn a_partial_or_missing_count_never_goes_on_the_card() {
        let v2 = crate::test_util::fixture!("stats/share-v2.txt");
        let expected = serde_json::from_str::<serde_json::Value>(v2).expect("fixture")["link"].clone();

        assert_eq!(view(Some(row(2, 9, 61, false))).share_link(), expected.as_str().expect("link"));
        assert_eq!(view(None).share_link(), expected.as_str().expect("link"));
    }
}
