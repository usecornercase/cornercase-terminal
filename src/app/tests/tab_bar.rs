use super::*;
use crate::config;
use crate::state;
use crate::ui::tab_bar::Strip;

fn drawn_in(app: &mut App, area: Rect) {
    render(app, area);
}

fn on_top(tabs: usize) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, dirs) = app_with(1);
    app.config.tabs = config::Tabs::Top.id().into();
    for _ in 1..tabs {
        app.add_tab(workspace_at(&app, 0, 0), AREA).expect("add a tab");
    }
    for (t, tab) in app.projects[0].workspaces[0].tabs.iter_mut().enumerate() {
        tab.name = Some(format!("t{t}"));
    }
    drawn_in(&mut app, AREA);
    (app, rx, dirs)
}

fn strip(app: &App) -> Strip {
    app.tab_strip(&app.layout(AREA))
}

fn ids(app: &App) -> Vec<u64> {
    app.projects[0].workspaces[0].tabs.iter().map(|t| t.id).collect()
}

#[test]
fn the_pane_starts_below_the_bar() {
    let (app, _rx, _dirs) = on_top(1);
    let areas = app.layout(AREA);
    assert_eq!((areas.tab_bar.height, areas.pane.y), (ui::tab_bar::HEIGHT, areas.tab_bar.bottom()));
    assert_eq!(areas.pane.bottom(), ui::layout(AREA, state::Widths::default()).pane.bottom());
}

#[test]
fn the_workspaces_list_leaves_the_tabs_out() {
    let (app, _rx, _dirs) = on_top(2);
    assert_eq!(ui::workspace_rows(&app.tab_lines()), [WorkspaceRow::Workspace(0)]);
}

#[test]
fn the_tree_ends_at_the_workspaces() {
    let (mut app, _rx, _dirs) = on_top(2);
    app.config.sidebar = config::Sidebar::Tree.id().into();
    drawn_in(&mut app, AREA);
    let rows = ui::tree_rows(&app.tree_shape());
    assert!(!rows.iter().any(|r| matches!(r, ui::TreeRow::Tab(..) | ui::TreeRow::NewTab(..))), "{rows:?}");
}

#[test]
fn compact_mode_keeps_the_tabs_in_its_menu() {
    let (mut app, _rx, _dirs) = on_top(2);
    drawn_in(&mut app, Rect { width: 80, ..AREA });
    assert!(ui::workspace_rows(&app.tab_lines()).contains(&WorkspaceRow::Tab(0, 1)));
}

#[test]
fn a_click_on_a_tab_shows_it() {
    let (mut app, _rx, _dirs) = on_top(2);
    let pos = strip(&app).item(0).as_position();
    click(&mut app, pos);
    assert_eq!(app.projects[0].workspaces[0].active, 0);
}

#[test]
fn plus_opens_a_tab() {
    let (mut app, _rx, _dirs) = on_top(1);
    let pos = strip(&app).new_button().as_position();
    click(&mut app, pos);
    let workspace = &app.projects[0].workspaces[0];
    assert_eq!((workspace.tabs.len(), workspace.active), (2, 1));
}

#[test]
fn the_close_button_closes_that_tab() {
    let (mut app, rx, _dirs) = on_top(2);
    let pos = strip(&app).close(0).as_position();
    click(&mut app, pos);
    pump_until(&mut app, &rx, "the first tab closes", |a| a.projects[0].workspaces[0].tabs.len() == 1);
}

#[test]
fn the_menu_button_opens_the_tab_menu() {
    let (mut app, _rx, _dirs) = on_top(2);
    let pos = strip(&app).menu(1).as_position();
    press(&mut app, pos);
    let tab = app.projects[0].workspaces[0].tabs[1].id;
    let Some(Overlay::Menu { actions, .. }) = &app.overlay else { panic!("no menu: {:?}", app.overlay.is_some()) };
    assert!(matches!(actions[..], [MenuAction::Rename(Target::Tab(_, _, id))] if id == tab));
}

#[test]
fn dragging_a_tab_moves_it() {
    let (mut app, _rx, _dirs) = on_top(3);
    let before = ids(&app);
    let (from, to) = (strip(&app).item(0).as_position(), strip(&app).item(2).as_position());
    press(&mut app, from);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), to);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), to);
    assert_eq!(ids(&app), [before[1], before[2], before[0]]);
    assert_eq!(app.projects[0].workspaces[0].tab().map(|t| t.id), Some(before[2]));
}

#[test]
fn the_wheel_scrolls_tabs_that_do_not_fit() {
    let (mut app, _rx, _dirs) = on_top(12);
    app.projects[0].workspaces[0].active = 0;
    app.scroll.set(List::TabBar, 0);
    let at = app.layout(AREA).tab_bar.as_position();
    mouse(&mut app, MouseEventKind::ScrollDown, at);
    assert_eq!(app.scroll.of(List::TabBar), 1);
}

#[test]
fn the_active_tab_is_scrolled_into_view() {
    let (mut app, _rx, _dirs) = on_top(12);
    assert!(!strip(&app).item(11).is_empty());
    app.projects[0].workspaces[0].active = 0;
    drawn_in(&mut app, AREA);
    assert_eq!(app.scroll.of(List::TabBar), 0);
}

#[test]
fn a_shell_tab_has_no_details() {
    let (app, _rx, _dirs) = on_top(1);
    assert_eq!(app.tab_bar_view().details, ui::Details::default());
}
