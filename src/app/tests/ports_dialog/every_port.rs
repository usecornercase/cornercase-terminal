use super::*;
use crate::ports::dialog::{Action, Scope};

fn outside(pid: i32, number: u16) -> Port {
    Port { place: Place::default(), ..port(pid, number) }
}

fn view(app: &App) -> Box<ui::Ports> {
    let Some(Overlay::Ports(dialog)) = &app.overlay else { panic!("the ports dialog is not open") };
    app.ports_view(dialog, AREA)
}

fn from_more(app: &mut App) {
    render(app, AREA);
    let more = app_areas(app).more.as_position();
    click(app, more);
    pick(app, "ports");
}

fn every(app: &mut App, ports: Vec<Port>) {
    opened(app);
    app.ports_act(Action::Scope(Scope::Every));
    let generation = dialog(app).generation;
    listed(app, generation, ports);
}

#[test]
fn more_opens_it_on_the_workspace_on_screen() {
    let (mut app, _rx, _dirs) = app_with(1);
    from_more(&mut app);
    let workspace = app.projects[0].workspaces[0].id;
    assert_eq!((dialog(&mut app).scope, dialog(&mut app).workspace), (Scope::Workspace, Some(workspace)));
}

#[test]
fn more_opens_every_port_with_no_workspace_on_screen_and_this_workspace_says_so() {
    let (mut app, _rx) = empty_app();
    from_more(&mut app);
    assert_eq!((dialog(&mut app).scope, dialog(&mut app).workspace), (Scope::Every, None));
    assert_eq!(view(&app).title, "Ports");
    app.ports_act(Action::Scope(Scope::Workspace));
    app.watch_ports_dialog(Instant::now());
    assert_eq!(view(&app).empty, Some("No workspace on screen. Show one, or list every port."));
    assert!(!app.ports_scanning());
}

#[test]
fn every_port_names_the_place_of_each_port_and_stops_only_a_workspace_s() {
    let (mut app, _rx, _dirs) = app_with(1);
    let id = app.projects[0].workspaces[0].id;
    let mine = Port { place: Place { workspace: Some(id), ..port(10, 3000).place }, ..port(10, 3000) };
    every(&mut app, vec![mine, outside(11, 5432)]);
    let view = view(&app);
    let project = app.project_label(&app.projects[0]);
    let workspace = app.projects[0].workspaces[0].label();
    assert_eq!(view.title, "Ports");
    assert_eq!(view.rows[0].place, format!("{project} › {workspace} · in the worktree, no tab"));
    assert_eq!(
        (view.rows[1].place.as_str(), view.rows[0].tick.is_some(), view.rows[1].tick.is_some()),
        ("no workspace", true, false)
    );
    assert_eq!(view.note, Some(ui::Note::Info("Only a workspace's programs can be stopped here.".into())));
}

#[test]
fn a_new_scope_drops_an_answer_for_the_last_one_and_scans_at_once() {
    let (mut app, rx, _dirs) = app_with(1);
    let earlier = opened(&mut app);
    wait_until("the first scan answers", || {
        drain(&mut app, &rx);
        dialog(&mut app).loaded
    });
    app.ports_act(Action::Scope(Scope::Every));
    assert!(app.ports_scanning());
    listed(&mut app, earlier, vec![port(10, 3000)]);
    assert!(!dialog(&mut app).loaded);
    wait_until("every port is listed", || {
        drain(&mut app, &rx);
        dialog(&mut app).loaded
    });
}

#[test]
fn a_workspace_that_closes_closes_this_workspace_but_not_every_port() {
    let (mut app, _rx, _dirs) = app_with(1);
    opened(&mut app);
    dialog(&mut app).workspace = Some(u64::MAX);
    app.watch_ports_dialog(Instant::now());
    assert!(app.overlay.is_none());
    every(&mut app, Vec::new());
    dialog(&mut app).workspace = Some(u64::MAX);
    app.watch_ports_dialog(Instant::now() + Duration::from_secs(10));
    assert_eq!(dialog(&mut app).workspace, None);
}
