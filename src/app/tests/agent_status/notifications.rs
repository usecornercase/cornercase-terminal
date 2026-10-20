use super::*;
use crate::app::agent_watch::Notice;
use crate::app::toast::UNDO_FOR;

const WINDOW: u64 = 1;

fn go(app: &App) -> String {
    app.toast.as_ref().and_then(|toast| toast.go.clone()).expect("a go")
}

fn notify_for(app: &mut App, p: usize, panes: Vec<u64>) {
    let (project, workspace) = (app.projects[p].id, app.projects[p].workspaces[0].id);
    app.notify(Notice { agent: "claude", status: Status::Waiting, at: (project, workspace), panes, principal: true });
}

fn pane_of(app: &App, p: usize, t: usize) -> u64 {
    app.projects[p].workspaces[0].tabs[t].panes[0].id
}

pub(super) fn active_tab(app: &App) -> usize {
    app.projects[app.active].workspaces[0].active
}

#[test]
fn the_toasts_go_shows_the_agent_and_lasts_six_seconds() {
    let mut w = Watched::start(agents::Known::Claude, true);
    w.report("waiting", 6);
    assert_eq!(active_tab(&w.app), 1);

    let toast = w.app.toast.as_ref().expect("a toast");
    assert_eq!((toast.lasts(), toast.view().button), (UNDO_FOR, Some(ui::ToastButton::Go)));
    let at = ui::toast_button(AREA, toast.view()).as_position();
    press(&mut w.app, at);

    assert_eq!((active_tab(&w.app), w.app.toast.is_none()), (0, true));
}

#[test]
fn a_click_on_a_notification_shows_its_pane_in_another_project() {
    let mut w = Watched::start(agents::Known::Claude, false);
    let other = TempDir::new();
    w.app.open_project(other.path().to_path_buf(), AREA).expect("open another project");
    assert_eq!(w.app.active, 1);
    let pane = pane_of(&w.app, 0, 0);
    notify_for(&mut w.app, 0, vec![pane]);

    let token = go(&w.app);
    w.app.notification_clicked(&token);

    assert_eq!((w.app.active, w.app.tab().and_then(Tab::pane).map(|t| t.id)), (0, Some(pane)));
}

#[test]
fn with_two_panes_behind_one_notification_it_goes_to_the_one_still_waiting() {
    let (mut app, _rx, _dirs) = app_with(1);
    app.add_tab(0, 0, AREA).expect("add a tab");
    app.add_tab(0, 0, AREA).expect("add a tab");
    let (a, b) = (pane_of(&app, 0, 0), pane_of(&app, 0, 1));
    let now = Instant::now();
    for (pane, activity) in [(a, Activity::Idle), (b, Activity::Waiting)] {
        let term = app.projects.pane_by_mut(pane).expect("the pane");
        term.agent.follow(Some("claude"));
        term.agent.update(Some(activity), false, now);
    }
    assert_eq!(active_tab(&app), 2);
    notify_for(&mut app, 0, vec![a, b]);

    let token = go(&app);
    app.notification_clicked(&token);

    assert_eq!(active_tab(&app), 1);
}

#[test]
fn a_closed_pane_falls_back_to_its_workspace_then_to_a_toast() {
    let mut w = Watched::start(agents::Known::Claude, true);
    let pane = pane_of(&w.app, 0, 0);
    notify_for(&mut w.app, 0, vec![pane]);
    let token = go(&w.app);
    let project = w.app.projects[0].id;
    w.app.close_tab(0, 0, 0);
    pump_until(&mut w.app, &w.rx, "the tab is gone", |a| a.projects[0].workspaces[0].tabs.len() == 1);
    let other = TempDir::new();
    w.app.open_project(other.path().to_path_buf(), AREA).expect("open another project");
    assert_eq!(w.app.active, 1);

    w.app.notification_clicked(&token);
    assert_eq!(w.app.active, 0);

    w.app.close_project(project);
    w.app.toast = None;
    w.app.notification_clicked(&token);
    assert!(toast(&w.app).is_some_and(|text| text.starts_with("claude in ") && text.ends_with(" is gone")));
}

#[test]
fn a_token_from_another_server_run_does_nothing() {
    let mut w = Watched::start(agents::Known::Claude, true);
    w.report("waiting", 6);
    let token = go(&w.app);
    let other = token.replacen("cc-", "cc-zz", 1);
    let before = (active_tab(&w.app), w.app.toast.clone());

    w.app.notification_clicked(&other);

    assert_eq!((active_tab(&w.app), w.app.toast.clone()), before);
}

#[test]
fn a_window_coming_back_shows_the_toast_of_what_it_missed() {
    let mut w = Watched::start(agents::Known::Claude, true);
    w.app.window_focus(WINDOW, false);
    w.report("waiting", 6);
    let text = toast(&w.app).map(str::to_owned);
    w.app.toast = None;

    w.app.window_focus(WINDOW, true);

    assert_eq!(toast(&w.app).map(str::to_owned), text);
    assert_eq!(w.app.toast.as_ref().map(|t| t.view().button), Some(Some(ui::ToastButton::Go)));
}

#[test]
fn a_window_coming_back_shows_nothing_once_the_agent_no_longer_needs_you() {
    let mut w = Watched::start(agents::Known::Claude, true);
    w.app.window_focus(WINDOW, false);
    w.report("waiting", 6);
    w.app.toast = None;
    w.report("busy", 6);

    w.app.window_focus(WINDOW, true);

    assert_eq!(w.app.toast, None);
}

#[test]
fn a_window_coming_back_shows_nothing_for_a_notification_that_came_while_it_was_focused() {
    let mut w = Watched::start(agents::Known::Claude, true);
    w.app.window_focus(WINDOW, true);
    w.report("waiting", 6);
    w.app.toast = None;

    w.app.window_focus(WINDOW, false);
    w.app.window_focus(WINDOW, true);

    assert_eq!(w.app.toast, None);
}
