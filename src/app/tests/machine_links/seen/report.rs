use std::time::SystemTime;

use super::*;
use crate::machines::Info;
use crate::time::Stamp;

mod machines_in_status {
    use super::*;

    #[test]
    fn list_what_each_machine_saw_with_its_pairs() {
        let Seeing { rig, ours, .. } = seeing_two();

        let status = rig.app.report(None);

        let [vps] = &status.machines[..] else { panic!("one machine: {:?}", status.machines) };
        let icon = Info::of(&machine("vps", "vps.lan"), None, None, None).icon;
        assert_eq!(
            (vps.name.as_str(), vps.icon.as_str(), vps.state, vps.boot.as_str()),
            ("vps", icon.as_str(), State::Linked, BOOT)
        );
        assert!(vps.seen.is_some());
        let paired: Vec<_> = vps.projects.iter().map(|p| (p.name.as_str(), p.machine.as_deref(), p.paired)).collect();
        assert_eq!(paired, [("web-shop", Some("vps"), Some(Some(ours))), ("blog", Some("vps"), Some(None))]);
        assert!(vps.projects.iter().flat_map(|p| &p.workspaces).all(|w| w.machine.as_deref() == Some("vps")));
    }

    #[test]
    fn leave_out_an_entry_that_repeats_a_machine_s_destination() {
        let Seeing { mut rig, .. } = seeing_two();
        rig.app.config.machines.push(machine("box", "vps.lan"));

        let status = rig.app.report(None);

        assert_eq!(status.machines.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["vps"]);
        assert_eq!(rig.app.remote_projects().len(), 2);
    }

    #[test]
    fn leave_the_projects_and_the_panes_to_this_computer() {
        let Seeing { rig, ours, .. } = seeing_two();

        let status = rig.app.report(None);

        assert_eq!(
            status.projects.iter().map(|p| (p.id, p.machine.as_deref(), p.paired)).collect::<Vec<_>>(),
            [(ours, None, None)]
        );
        let (rows, live) = (status.panes(), rig.app.pane_ids());
        assert!(!rows.is_empty() && rows.iter().all(|row| row.project == ours && live.contains(&row.pane)), "{rows:?}");
    }

    #[test]
    fn are_left_out_until_a_machine_sends_what_it_has() {
        let (mut rig, ends) = watched();
        let _theirs = linked(&ends);
        rig.until("the machine links", |app| app.machines.held()[0].status.state == State::Linked);

        let written = serde_json::to_value(rig.app.report(None)).expect("json");

        assert!(written.get("machines").is_none() && written.get("shown_remote").is_none(), "{written}");
    }

    #[test]
    fn say_when_the_last_snapshot_arrived_not_when_the_link_came_up() {
        let Seeing { mut rig, mut theirs, .. } = seeing_two();
        let held = &mut rig.app.machines.held_mut()[0];
        (held.status.seen, held.seen_at) = (Some(0), Some(0));
        let before = SystemTime::now() - Duration::from_secs(1);

        serve(&mut theirs, 2, worked(report(&[("docs", None)])));
        seeing(&mut rig, &["docs"]);

        let status = rig.app.report(None);
        let seen = status.machines[0].seen.as_deref().and_then(Stamp::parse).expect("a time");
        assert!(seen >= before, "{seen:?}");
        assert_eq!(rig.app.machines.held()[0].status.seen, Some(0));
    }

    #[test]
    fn keep_a_fallen_machine_with_its_state_and_when_it_was_seen() {
        let rig = seeing_fall();

        let status = rig.app.report(None);
        let [vps] = &status.machines[..] else { panic!("one machine: {:?}", status.machines) };
        let names: Vec<&str> = vps.projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!((vps.state, vps.seen.is_some(), names), (State::Unreachable, true, vec!["web-shop", "blog"]));
    }
}

mod machines_in_watch {
    use std::time::Instant;

    use super::*;
    use crate::control::{Command, Request, Response, Snapshot, Watch};

    const CLIENT: u64 = 9;

    fn watch(app: &mut App) {
        app.clients.events.snapshots.set_every(Duration::ZERO);
        let request = Request {
            caller: None,
            server: None,
            contract: None,
            boot: None,
            view: None,
            command: Command::Watch(Watch {}),
        };
        app.request(CLIENT, &serde_json::to_string(&request).expect("json"), None, Instant::now());
        assert_eq!(app.take_answers().len(), 1, "the first snapshot");
    }

    fn snapshots(app: &mut App) -> Vec<Snapshot> {
        app.take_events()
            .into_iter()
            .filter_map(|(_, streamed)| match streamed {
                Streamed::Snapshot(text) => match serde_json::from_str(&text) {
                    Ok(Response::Ok(value)) => Some(serde_json::from_value(value).expect("a snapshot")),
                    other => panic!("not a snapshot: {other:?}"),
                },
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_new_snapshot_from_a_machine_sends_one_to_the_watchers_here() {
        let Seeing { mut rig, mut theirs, .. } = seeing_two();
        watch(&mut rig.app);
        rig.app.trace();
        snapshots(&mut rig.app);

        serve(&mut theirs, 2, worked(report(&[("web-shop", Some(SHOP)), ("docs", None)])));
        seeing(&mut rig, &["web-shop", "docs"]);
        rig.app.trace();

        let got = snapshots(&mut rig.app);
        let last = got.last().expect("a new snapshot");
        let names: Vec<&str> = last.report.machines[0].projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["web-shop", "docs"]);
    }
}
