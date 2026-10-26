use std::time::Duration;

use super::*;
use crate::app::ports_dialog::PortsEvent;
use crate::host::open::{Browse, Opener};
use crate::ports::dialog::{Dialog, Hit};
use crate::ports::{Place, Port, Via};
use crate::process;
use crate::test_util::Running;

fn port(pid: i32, number: u16) -> Port {
    Port {
        number,
        addresses: vec!["0.0.0.0".parse().expect("an address")],
        pid,
        program: Some("sleep".into()),
        args: Vec::new(),
        started: process::start_stamp(pid),
        place: Place { workspace: Some(1), pane: None, via: Some(Via::Cwd), agent: None },
    }
}

fn dialog(app: &mut App) -> &mut Dialog {
    match &mut app.overlay {
        Some(Overlay::Ports(dialog)) => dialog,
        _ => panic!("the ports dialog is not open"),
    }
}

fn opened(app: &mut App) -> u64 {
    let workspace = app.projects[0].workspaces[0].id;
    app.open_ports(workspace);
    dialog(app).generation
}

fn listed(app: &mut App, generation: u64, ports: Vec<Port>) {
    app.ports_answered(PortsEvent::Listed { generation, result: Ok(ports) });
}

#[test]
fn a_workspace_menu_offers_its_ports() {
    let (mut app, _rx, _dirs) = app_with(1);
    right_click_row(&mut app, WorkspaceRow::Workspace(0));
    assert!(menu_labels(&app).contains(&"ports".to_string()), "{:?}", menu_labels(&app));
    pick(&mut app, "ports");
    assert!(matches!(app.overlay, Some(Overlay::Ports(_))));
}

#[test]
fn the_list_comes_from_a_scan_and_is_asked_again_every_two_seconds_while_open() {
    let (mut app, rx, _dirs) = app_with(1);
    opened(&mut app);
    assert!(app.ports_scanning());
    wait_until("the first scan answers", || {
        drain(&mut app, &rx);
        dialog(&mut app).loaded
    });
    let now = Instant::now();
    app.watch_ports_dialog(now);
    assert!(!app.ports_scanning());
    app.watch_ports_dialog(now + Duration::from_secs(2));
    assert!(app.ports_scanning());
    wait_until("the second scan answers", || {
        drain(&mut app, &rx);
        !app.ports_scanning()
    });
    app.overlay = None;
    app.watch_ports_dialog(now + Duration::from_secs(10));
    assert!(!app.ports_scanning());
}

#[test]
fn an_answer_for_an_earlier_opening_is_dropped() {
    let (mut app, _rx, _dirs) = app_with(1);
    let earlier = opened(&mut app);
    let now = opened(&mut app);
    listed(&mut app, earlier, vec![port(10, 3000)]);
    assert!(!dialog(&mut app).loaded);
    listed(&mut app, now, vec![port(10, 3000)]);
    assert_eq!(dialog(&mut app).rows.len(), 1);
}

#[test]
fn a_local_window_opens_the_url_and_another_copies_it() {
    let (mut app, _rx, _dirs) = app_with(1);
    let generation = opened(&mut app);
    listed(&mut app, generation, vec![port(10, 5173)]);
    app.set_window(7, &[Opener::Default]);
    let action = dialog(&mut app).click(Hit::Open(0));
    app.ports_act(action);
    assert_eq!(app.take_browses(), [(7, Browse { ip: None, port: 5173 })]);
    app.set_window(8, &[]);
    let action = dialog(&mut app).click(Hit::Open(0));
    app.ports_act(action);
    assert_eq!(app.take_browses(), []);
    let toast = app.messages.toast.as_ref().map(|t| t.message.clone()).unwrap_or_default();
    assert!(toast.starts_with("copied http://localhost:5173: the server is on another machine"), "{toast}");
}

#[test]
fn a_program_that_no_longer_holds_its_port_is_left_running_after_the_confirmation() {
    let (mut app, rx, _dirs) = app_with(1);
    let sleeper = Running::spawn(std::process::Command::new("/bin/sleep").arg("30"));
    let generation = opened(&mut app);
    listed(&mut app, generation, vec![port(sleeper.pid(), 1)]);
    let action = dialog(&mut app).click(Hit::StopRow(0));
    app.ports_act(action);
    assert!(app.messages.toast.is_none());
    let action = dialog(&mut app).click(Hit::Stop);
    app.ports_act(action);
    wait_until("the stop answers", || {
        drain(&mut app, &rx);
        app.messages.toast.is_some()
    });
    let toast = app.messages.toast.as_ref().map(|t| t.message.clone()).unwrap_or_default();
    assert!(toast.contains("no longer listens on port 1"), "{toast}");
    assert!(process::alive(sleeper.pid()));
}

#[test]
fn a_confirmation_longer_than_the_dialog_says_to_scroll_until_its_end() {
    let (mut app, _rx, _dirs) = app_with(1);
    let generation = opened(&mut app);
    listed(&mut app, generation, (0..30).map(|i| port(10 + i, 3000 + u16::try_from(i).expect("small"))).collect());
    dialog(&mut app).click(Hit::Stop);
    let area = Rect::new(0, 0, 60, 20);
    let note = |app: &mut App| {
        let Some(Overlay::Ports(dialog)) = &app.overlay else { panic!("the ports dialog is not open") };
        app.ports_view(dialog, area).note
    };
    assert_eq!(note(&mut app), Some(ui::Note::Info("↓ scroll for more".into())));
    dialog(&mut app).scroll_by(100, 15, 200);
    assert_eq!(note(&mut app), None);
}
