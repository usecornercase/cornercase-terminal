use super::*;
use crate::queue::clock;
use crate::time;

const MORNING: i64 = 1_794_528_000;

fn not_utcs_day(now: i64) -> &'static str {
    if now.rem_euclid(86_400) < 11 * 3_600 { "-12:00" } else { "+14:00" }
}

#[test]
fn the_zone_this_test_picks_never_has_utcs_day_nor_a_midnight_near() {
    for step in 0..288 {
        let now = MORNING + step * 300;
        let zone = not_utcs_day(now);
        assert_ne!(clock::day(now, zone), time::date(now * 1000), "{now}");
        assert_eq!(clock::day(now + 3_000, zone), clock::day(now, zone), "{now}");
    }
}

#[test]
fn a_run_folder_is_named_after_the_day_in_the_windows_zone() {
    let mut team = Team::new();
    let now = time::now();
    let zone = not_utcs_day(now);
    team.app.set_queue_zone(zone.into());

    let run = team.started();

    let name = run.run_dir.as_deref().and_then(Path::file_name).and_then(|n| n.to_str()).expect("a name").to_string();
    assert!(name.starts_with(&format!("{}-", clock::day(now, zone))), "{name}");
}

#[test]
fn without_a_window_the_zone_is_this_machines() {
    let team = Team::new();
    assert_eq!(team.app.shown_zone(), clock::local_zone());
}
