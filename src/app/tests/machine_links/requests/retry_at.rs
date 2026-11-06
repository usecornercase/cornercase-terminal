use std::time::SystemTime;

use super::*;
use crate::time::Stamp;

const FIRST_WAIT: Duration = Duration::from_secs(1);
const STAMPED: Duration = Duration::from_millis(1);

fn fallen() -> (Rig, Sender<()>) {
    let (go, gate) = mpsc::channel();
    let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Refusing(Arc::new(Mutex::new(gate))));
    go.send(()).expect("the first try fails");
    rig.until("the machine falls", |app| app.machines.held()[0].status.state == State::Unreachable);
    (rig, go)
}

fn reported(there: There) -> Rig {
    let mut rig = Rig::new(vec![machine("vps", "vps.lan")], there);
    rig.until("the link reports", |app| app.machines.held()[0].status.state != State::Connecting);
    rig
}

fn next_try(rig: &mut Rig) -> Option<String> {
    rig.listed().machines.remove(0).retry_at
}

#[track_caller]
fn moved_to_now(next: Option<String>) {
    let next = next.expect("a next try");
    assert!(Stamp::parse(&next).is_some_and(|at| at <= SystemTime::now()), "{next} is still to come");
}

fn retried(rig: &mut Rig) {
    rig.ask(&json!({"action": "retry", "entry": "vps"})).expect("retried");
}

fn woken(rig: &mut Rig) {
    rig.woken();
}

fn on_a_new_network(rig: &mut Rig) {
    rig.routes_change_next();
    rig.refresh(Duration::from_secs(3));
}

fn edited(rig: &mut Rig) {
    rig.edit(vec![Machine { folder: "/srv".into(), ..machine("vps", "vps.lan") }], Duration::from_secs(2));
}

#[test]
fn a_fallen_link_says_when_it_tries_next_in_the_form_of_seen_in_the_list_and_the_app_stream() {
    let before = SystemTime::now();
    let (mut rig, _go) = fallen();
    let after = SystemTime::now();

    let listed = rig.listed().machines.remove(0);
    let row = serde_json::to_value(rig.app.app_state().machines).expect("json");

    let next = listed.retry_at.as_deref().expect("a next try");
    let at = Stamp::parse(next).expect("a time");
    assert!(at + STAMPED >= before + FIRST_WAIT && at <= after + FIRST_WAIT, "{next}");
    assert_eq!((listed.state, row[0]["retry_at"].as_str()), (State::Unreachable, Some(next)));
}

#[test]
fn a_link_still_connecting_says_no_time() {
    let (go, gate) = mpsc::channel::<()>();
    let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Refusing(Arc::new(Mutex::new(gate))));

    assert_eq!((rig.state("vps"), next_try(&mut rig)), (Some(State::Connecting), None));
    drop(go);
}

#[test]
fn a_linked_machine_says_no_time() {
    let mut rig = reported(There::Server("vps-1".into()));

    assert_eq!((rig.state("vps"), next_try(&mut rig)), (Some(State::Linked), None));
}

#[rstest]
#[case::older(There::Older)]
#[case::without_cornercase(There::Missing)]
#[case::this_computer(There::Server(control::server_token().into()))]
fn a_link_that_waits_on_a_person_says_no_time_even_after_a_retry(#[case] there: There) {
    let mut rig = reported(there);

    assert_eq!(next_try(&mut rig), None);
    retried(&mut rig);
    assert_eq!(next_try(&mut rig), None);
}

#[test]
fn a_link_on_the_same_server_as_another_says_no_time() {
    let both = vec![machine("vps", "vps.lan"), machine("pi", "pi.lan")];
    let mut rig = Rig::new(both, There::Server("shared-1".into()));

    rig.until("both links report", |app| app.machines.held().iter().all(|held| held.status.state != State::Connecting));

    let moments: Vec<Option<String>> = rig.listed().machines.into_iter().map(|row| row.retry_at).collect();
    assert_eq!(moments, [None, None]);
}

#[test]
fn stays_put_and_sends_no_line_until_the_next_try_fails() {
    let (mut rig, go) = fallen();
    rig.follow_the_app();
    let first = next_try(&mut rig);
    rig.app.trace_refresh();
    assert_eq!(app_lines(rig.app.take_events()), Vec::<Value>::new());

    wait_until("the next try starts", || rig.seen("connect vps.lan") == 2);
    assert_eq!(next_try(&mut rig), first, "the moment that passed, while its try runs");
    rig.app.trace_refresh();
    assert_eq!(app_lines(rig.app.take_events()), Vec::<Value>::new());

    go.send(()).expect("that try fails");
    let told = first.clone();
    rig.until("the try fails", |app| app.machine_list().machines[0].retry_at != told);
    rig.app.trace_refresh();

    let lines = app_lines(rig.app.take_events());
    let moved = next_try(&mut rig);
    assert!(moved > first, "{moved:?} is not after {first:?}");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["machines"][0]["retry_at"].as_str(), moved.as_deref());
}

#[test]
fn a_retry_answers_with_the_moment_it_moved_to() {
    let (mut rig, _go) = fallen();

    let answer = rig.ask(&json!({"action": "retry", "entry": "vps"})).expect("retried");

    moved_to_now(answer.machines.into_iter().next().and_then(|row| row.retry_at));
}

#[rstest]
#[case::a_retry(retried)]
#[case::a_wake(woken)]
#[case::a_new_network(on_a_new_network)]
#[case::an_edit(edited)]
fn a_try_at_once_moves_it_to_that_moment(#[case] try_now: fn(&mut Rig)) {
    let (mut rig, _go) = fallen();

    try_now(&mut rig);

    moved_to_now(next_try(&mut rig));
    wait_until("the link tries again", || rig.seen("connect vps.lan") == 2);
}
