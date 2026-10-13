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
    assert_eq!(view.levels(1), [Some(1), Some(2), Some(3), Some(4), Some(0), Some(0), None]);
}
#[test]
fn the_first_incomplete_day_has_a_heatmap_square() {
    let mut stats = Stats::new(at(1_000));
    stats.add(at(1_000), Known::Claude, Counts { agent_ms: 1000, ..Counts::default() });
    let view = stats.view(at(1_000));
    assert_eq!(view.days, 0);
    assert_eq!(view.levels(1), [Some(1), None, None, None, None, None, None]);
    assert!(view.share_link().ends_with("&days=0&map=IA"));
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
pub(crate) fn example(now: SystemTime) -> Stats {
    let fixture: serde_json::Value =
        serde_json::from_str(crate::test_util::fixture!("stats/share-v1.txt")).expect("fixture");
    let mut stats = Stats::new(now.checked_sub(Duration::from_hours(2640)).expect("counting start"));
    let bucket = |name: &str| {
        let a: Vec<u64> =
            fixture[name].as_array().expect("array").iter().map(|v| v.as_u64().expect("integer")).collect();
        Bucket {
            by_agent: [
                Counts {
                    agents: a[0],
                    prompts: a[1],
                    turns: a[1].saturating_sub(3),
                    agent_ms: a[2] * 1000,
                    tokens: Tokens { input: a[5] * 3, out: a[5], cache_read: a[5] * 100, ..Tokens::default() },
                    ..Counts::default()
                },
                Counts::default(),
                Counts::default(),
                Counts::default(),
            ],
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
                by_agent: [
                    Counts { agent_ms: n.as_u64().expect("seconds") * 1000, ..Counts::default() },
                    Counts::default(),
                    Counts::default(),
                    Counts::default(),
                ],
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
        serde_json::from_str(crate::test_util::fixture!("stats/share-v1.txt")).expect("fixture");
    let now = at(10_000);
    let stats = example(now);
    assert_eq!(share_link(&stats, now), fixture["link"].as_str().expect("link"));
}
