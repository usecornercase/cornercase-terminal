use std::time::{Duration, SystemTime};

use rstest::rstest;

use super::*;
use crate::agents::stats::View;
use crate::app::machine_links::Held;
use crate::app::machine_usage::NOT_READ;
use crate::machines::link::Down;
use crate::machines::{Machine, State, machine};

const VPS: &str = "vps";
const SEEN: i64 = 1_793_000_000;

fn linked() -> (App, Receiver<AppEvent>) {
    let (mut app, rx) = empty_app();
    app.config.machines = vec![machine(VPS, "vps.lan")];
    app.follow_machines();
    let held = held(&mut app);
    (held.status.state, held.status.seen) = (State::Linked, Some(SEEN));
    (app, rx)
}

fn held(app: &mut App) -> &mut Held {
    app.machines.named(VPS).expect("the machine")
}

fn keep_a_copy(app: &mut App, age: Duration) {
    let mut view = View::default();
    view.windows.get_mut("total").expect("the total window").counts.prompts = 5;
    view.blocks[0].agent_seconds = 60.0;
    held(app).cache.stats.read = Some((view, SystemTime::now() - age));
}

fn prompts(answer: &Value) -> &Value {
    &answer["windows"]["total"]["prompts"]
}

fn asked_nothing(app: &mut App) -> bool {
    let cache = &held(app).cache;
    !cache.stats.asking && !cache.usage.asking
}

mod one_machine {
    use super::*;

    #[test]
    fn answers_its_last_copy_at_once_and_asks_nothing() {
        let (mut app, _rx) = linked();
        keep_a_copy(&mut app, Duration::from_mins(4));

        let there = ok(&mut app, "stats", &json!({"machine": "VPS", "cached": true}));

        assert_eq!(prompts(&there), 5);
        let machine = &there["machine"];
        assert_eq!(
            (&machine["name"], &machine["state"], &machine["error"]),
            (&json!(VPS), &json!("linked"), &json!(null))
        );
        assert!(machine["read_at"].is_string() && machine["seen"].is_string(), "{machine}");
        assert!(there.get("machines").is_none(), "{there}");
        assert!(asked_nothing(&mut app));
    }

    #[test]
    fn that_is_down_answers_its_last_copy_and_why_without_its_destination() {
        let (mut app, _rx) = linked();
        keep_a_copy(&mut app, Duration::from_mins(4));
        let refused = "ssh: connect to host vps.lan port 22: Connection refused";
        let held = held(&mut app);
        (held.status.state, held.status.down) = (State::Unreachable, Some(Down::Unreachable(refused.into())));

        let there = ok(&mut app, "stats", &json!({"machine": VPS, "cached": true}));

        assert_eq!(prompts(&there), 5);
        let hidden = "ssh: connect to host vps port 22: Connection refused";
        assert_eq!((&there["machine"]["state"], &there["machine"]["error"]), (&json!("unreachable"), &json!(hidden)));
        assert!(there["machine"]["read_at"].is_string(), "{there}");
    }

    #[test]
    fn with_no_copy_fails_naming_it_and_why_and_asks_nothing() {
        let (mut app, _rx) = linked();

        let refused = refused(&mut app, "stats", &json!({"machine": VPS, "cached": true}));

        assert_eq!(refused.code, "failed");
        assert_eq!(refused.message, format!("no stats from `vps`: {NOT_READ}"));
        assert!(asked_nothing(&mut app));
    }

    #[test]
    fn that_is_not_listed_is_not_found() {
        let (mut app, _rx) = linked();

        let refused = refused(&mut app, "stats", &json!({"machine": "pi", "cached": true}));

        assert_eq!(refused.code, "not-found");
    }
}

mod every_machine {
    use super::*;

    #[test]
    fn sums_the_last_copies_at_once_and_asks_nothing() {
        let (mut app, _rx) = linked();
        keep_a_copy(&mut app, Duration::from_mins(4));

        let all = ok(&mut app, "stats", &json!({"machine": "ALL", "cached": true}));

        assert_eq!(prompts(&all), 5);
        let there = &all["machines"][0];
        assert_eq!((&there["name"], &there["state"], &there["error"]), (&json!(VPS), &json!("linked"), &json!(null)));
        assert!(there["read_at"].is_string() && there["seen"].is_string(), "{there}");
        assert!(all.get("machine").is_none(), "{all}");
        assert!(asked_nothing(&mut app));
    }

    #[test]
    fn leaves_a_machine_with_no_copy_out_and_says_why() {
        let (mut app, _rx) = linked();

        let all = ok(&mut app, "stats", &json!({"machine": "all", "cached": true}));

        assert_eq!(prompts(&all), 0);
        let there = &all["machines"][0];
        assert_eq!(
            (&there["name"], &there["state"], &there["read_at"], &there["error"]),
            (&json!(VPS), &json!("linked"), &json!(null), &json!(NOT_READ))
        );
        assert!(there["seen"].is_string(), "{there}");
        assert!(asked_nothing(&mut app));
    }
}

#[rstest]
#[case::one_machine(VPS)]
#[case::every_machine("all")]
fn a_copy_read_days_ago_lands_on_its_days_in_the_heatmap(#[case] machine: &str) {
    let (mut app, _rx) = linked();
    keep_a_copy(&mut app, Duration::from_hours(72));

    let answer = ok(&mut app, "stats", &json!({"machine": machine, "cached": true}));

    assert_eq!(
        (&answer["blocks"][0]["agent_seconds"], &answer["blocks"][3]["agent_seconds"]),
        (&json!(0.0), &json!(60.0))
    );
}

mod a_machine_no_longer_accepted {
    use super::*;

    #[rstest]
    fn is_not_found_by_its_name_and_not_asked_before_its_link_goes(
        #[values(Vec::new(), vec![machine(VPS, "")])] listed: Vec<Machine>,
        #[values("stats", "usage")] command: &str,
        #[values(true, false)] cached: bool,
    ) {
        let (mut app, _rx) = linked();
        keep_a_copy(&mut app, Duration::from_mins(4));
        app.config.machines = listed;

        let refused = refused(&mut app, command, &json!({"machine": VPS, "cached": cached}));

        assert_eq!(refused.code, "not-found", "{}", refused.message);
        assert!(asked_nothing(&mut app));
    }

    #[test]
    fn is_left_out_of_every_machine() {
        let (mut app, _rx) = linked();
        keep_a_copy(&mut app, Duration::from_mins(4));
        app.config.machines.clear();

        let all = ok(&mut app, "stats", &json!({"machine": "all", "cached": true}));

        assert_eq!(prompts(&all), 0);
        assert!(all.get("machines").is_none(), "{all}");
    }
}

mod without_a_machine {
    use super::*;

    #[rstest]
    #[case::asking_for_the_copy(true)]
    #[case::leaving_it_out(false)]
    fn it_is_this_computer_s_stats_and_no_machine_is_read(#[case] cached: bool) {
        let (mut app, _rx) = linked();
        keep_a_copy(&mut app, Duration::from_mins(4));

        let here = ok(&mut app, "stats", &json!({"cached": cached}));

        assert_eq!(prompts(&here), 0);
        assert!(here.get("machine").is_none() && here.get("machines").is_none(), "{here}");
        assert!(asked_nothing(&mut app));
    }
}
