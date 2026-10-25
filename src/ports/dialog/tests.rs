use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{Action, Dialog, Hit, Step};
use crate::ports::{Place, Port};

fn port(pid: i32, number: u16) -> Port {
    Port {
        number,
        addresses: vec!["127.0.0.1".parse().expect("an address")],
        pid,
        program: Some("node".into()),
        args: Vec::new(),
        started: Some(7),
        place: Place::default(),
    }
}

fn dialog() -> Dialog {
    let mut dialog = Dialog::new(1, 1);
    dialog.listed(Ok(vec![port(10, 3000), port(11, 5173), port(12, 6006)]));
    dialog
}

fn stopped(action: &Action) -> Vec<u16> {
    match action {
        Action::Stop(targets) => targets.iter().map(|target| target.port).collect(),
        other => panic!("not a stop: {other:?}"),
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn a_refresh_keeps_the_ticks_of_the_ports_still_there() {
    let mut dialog = dialog();
    dialog.click(Hit::Tick(1));
    dialog.click(Hit::Tick(2));
    dialog.listed(Ok(vec![port(11, 5173), port(13, 6006)]));
    let ticked: Vec<bool> = dialog.rows.iter().map(|row| row.ticked).collect();
    assert_eq!((ticked, dialog.cursor.selected), (vec![true, false], Some(0)));
}

#[test]
fn nothing_stops_before_the_confirmation() {
    let mut dialog = dialog();
    assert_eq!(dialog.click(Hit::StopRow(1)), Action::None);
    assert!(matches!(&dialog.step, Step::Confirm(ports) if ports.len() == 1 && ports[0].number == 5173));
    assert_eq!(stopped(&dialog.click(Hit::Stop)), [5173]);
    assert_eq!(dialog.step, Step::List);
}

#[test]
fn the_stop_button_takes_the_ticked_ports_else_every_port() {
    let mut dialog = dialog();
    dialog.click(Hit::Stop);
    assert_eq!(stopped(&dialog.click(Hit::Stop)), [3000, 5173, 6006]);
    dialog.click(Hit::Tick(0));
    dialog.click(Hit::Tick(2));
    dialog.click(Hit::Stop);
    assert_eq!(stopped(&dialog.key(key(KeyCode::Enter), 10)), [3000, 6006]);
    assert_eq!(dialog.ticked(), 0);
}

#[test]
fn going_back_stops_nothing() {
    let mut dialog = dialog();
    dialog.click(Hit::Stop);
    assert_eq!(dialog.key(key(KeyCode::Esc), 10), Action::None);
    assert_eq!((dialog.step.clone(), dialog.click(Hit::Back)), (Step::List, Action::None));
    assert_eq!(dialog.key(key(KeyCode::Esc), 10), Action::Close);
}

#[test]
fn enter_asks_only_for_ticked_ports() {
    let mut dialog = dialog();
    assert_eq!(dialog.key(key(KeyCode::Enter), 10), Action::None);
    assert_eq!(dialog.step, Step::List);
    dialog.key(key(KeyCode::Down), 10);
    dialog.key(key(KeyCode::Char(' ')), 10);
    dialog.key(key(KeyCode::Enter), 10);
    assert!(matches!(&dialog.step, Step::Confirm(ports) if ports[0].number == 5173));
}

#[test]
fn a_failed_scan_keeps_the_last_rows() {
    let mut dialog = dialog();
    dialog.listed(Err("no".into()));
    assert_eq!((dialog.rows.len(), dialog.failed.as_deref()), (3, Some("no")));
}
