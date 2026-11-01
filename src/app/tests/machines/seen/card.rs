use super::*;
use crate::agents::activity::Status;
use crate::app::machines::RemoteItem;

fn pick(app: &mut App, item: RemoteItem) {
    assert!(app.select_remote("vps", item), "the machine's workspace is picked");
}

fn fallen(rig: &Rig) -> (Instant, Duration) {
    let held = &rig.app.machines.held()[0];
    let (at, failed) = held.down_at.expect("when the machine fell");
    (at, held.status.down.as_ref().expect("why it fell").wait(failed))
}

#[test]
fn a_machine_s_workspace_picked_hides_the_local_panes_and_says_where_it_runs() {
    let Seeing { mut rig, .. } = seeing_claude();
    assert!(rig.app.tab_view(AREA).is_some());

    pick(&mut rig.app, RemoteItem::Tab(11));

    let card = ui::RemoteCard {
        machine: "vps".into(),
        agent: Some(("claude".into(), Some(Status::Working))),
        folder: "~/web-shop".into(),
        down: None,
    };
    assert_eq!((rig.app.tab_view(AREA).is_none(), rig.app.remote_card(Instant::now())), (true, Some(card)));
    rig.app.leave_remote();
    assert_eq!((rig.app.tab_view(AREA).is_some(), rig.app.remote_card(Instant::now())), (true, None));
}

#[test]
fn a_fallen_machine_s_card_counts_down_to_the_next_try_each_second() {
    let mut rig = seeing_fall();
    pick(&mut rig.app, RemoteItem::Workspace(10));
    let (at, wait) = fallen(&rig);
    let later = at + Duration::from_millis(300);

    let card = rig.app.remote_card(later).expect("the card");

    assert_eq!(card.down, Some(ui::Retry::In(wait.as_secs())));
    assert_eq!((card.agent, rig.app.card_wake(later)), (None, Some(Duration::from_millis(700))));
    let due = rig.app.remote_card(at + wait).expect("the card");
    assert_eq!((due.down, rig.app.card_wake(at + wait)), (Some(ui::Retry::Now), None));
}

#[test]
fn nothing_wakes_for_the_card_while_the_machine_is_linked_or_nothing_there_is_picked() {
    let Seeing { mut rig, .. } = seeing_two();
    pick(&mut rig.app, RemoteItem::Workspace(10));
    assert_eq!(rig.app.card_wake(Instant::now()), None);

    let mut rig = seeing_fall();
    let (at, _) = fallen(&rig);
    assert_eq!(rig.app.card_wake(at), None);
    pick(&mut rig.app, RemoteItem::Workspace(10));
    assert!(rig.app.card_wake(at).is_some());
}

#[test]
fn a_working_agent_there_spins_only_while_its_machine_is_linked() {
    let Seeing { mut rig, theirs, .. } = seeing_claude();
    assert!(rig.app.agent_working());

    drop(theirs);
    rig.until("the machine falls", |app| app.machines.held()[0].status.state == State::Unreachable);

    assert!(!rig.app.agent_working());
}
