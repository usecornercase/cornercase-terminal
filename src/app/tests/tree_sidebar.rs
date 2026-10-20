use super::*;
use crate::config;
use crate::project::group;
use crate::state;
use crate::ui::TreeRow;

const TALL: Rect = Rect { x: 0, y: 0, width: 100, height: 30 };

fn drawn(app: &mut App) {
    render(app, TALL);
}

fn tree(n: usize) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, dirs) = app_with(n);
    app.config.sidebar = config::Sidebar::Tree.id().into();
    drawn(&mut app);
    (app, rx, dirs)
}

fn list() -> Rect {
    ui::layout_with(TALL, state::Widths::default(), false, config::Sidebar::Tree).list
}

fn row(app: &App, row: TreeRow) -> Rect {
    ui::tree_row(list(), &app.tree_shape(), app.scroll.of(List::Projects), row)
}

fn click_at(app: &mut App, pos: Position) {
    click_in(app, pos, TALL);
    drawn(app);
}

fn click_name(app: &mut App, r: TreeRow) {
    let r = row(app, r);
    click_at(app, Position::new(r.x + 10, r.y));
}

fn click_arrow(app: &mut App, r: TreeRow) {
    let arrow = ui::tree_arrow(list(), &app.tree_shape(), app.scroll.of(List::Projects), r);
    click_at(app, arrow.as_position());
}

fn ids(app: &App, p: usize) -> (u64, u64, u64) {
    let project = &app.projects[p];
    let workspace = &project.workspaces[0];
    (project.id, workspace.id, workspace.tabs[0].id)
}

#[test]
fn the_arrow_folds_a_project_and_its_name_opens_it() {
    let (mut app, _rx, _dirs) = tree(2);
    click_arrow(&mut app, TreeRow::Project(0));
    let folded = (app.projects[0].collapsed, app.active);

    click_name(&mut app, TreeRow::Project(0));

    assert_eq!((folded, app.projects[0].collapsed, app.active), ((true, 1), false, 0));
}

#[test]
fn the_arrow_folds_a_workspace() {
    let (mut app, _rx, _dirs) = tree(1);
    click_arrow(&mut app, TreeRow::Workspace(0, 0));
    assert!(app.projects[0].workspaces[0].collapsed);
}

#[test]
fn a_tab_of_another_project_is_one_click_away() {
    let (mut app, _rx, _dirs) = tree(2);
    click_name(&mut app, TreeRow::Tab(0, 0, 0));
    assert_eq!(app.active, 0);
}

#[test]
fn adding_a_tab_under_another_project_switches_to_it() {
    let (mut app, _rx, _dirs) = tree(2);
    click_name(&mut app, TreeRow::NewTab(0, 0));
    let workspace = &app.projects[0].workspaces[0];
    assert_eq!((app.active, workspace.tabs.len(), workspace.active), (0, 2, 1));
}

#[test]
fn a_new_workspace_under_another_project_is_asked_for_that_project() {
    let (mut app, _rx, _dirs) = tree(2);
    click_name(&mut app, TreeRow::NewWorkspace(0));
    pick(&mut app, "new workspace");
    let asked = matches!(app.overlay, Some(Overlay::NewWorkspace { project, .. }) if project == app.projects[0].id);
    assert_eq!((asked, app.active), (true, 1));
}

#[test]
fn a_flow_started_under_another_project_is_for_that_project() {
    let (mut app, _rx, _dirs) = tree(2);
    click_name(&mut app, TreeRow::NewWorkspace(0));
    assert_eq!(menu_labels(&app), ["new workspace", "start a flow…"]);
    pick(&mut app, "start a flow…");
    let asked = matches!(&app.overlay, Some(Overlay::StartFlow { start, .. }) if start.project == app.projects[0].id);
    assert_eq!((asked, app.active), (true, 1));
}

#[test]
fn a_plain_workspace_created_under_another_project_is_shown() {
    let (mut app, _rx, _dirs) = tree(2);
    click_name(&mut app, TreeRow::NewWorkspace(0));
    pick(&mut app, "new workspace");

    submit_text(&mut app, "spike");

    assert_eq!((app.active, app.projects[0].workspaces.len(), app.projects[0].active), (0, 2, 1));
}

#[test]
fn closing_a_project_asks_first() {
    let (mut app, _rx, _dirs) = tree(2);
    let close = ui::tree_close(list(), &app.tree_shape(), 0, TreeRow::Project(0));
    click_at(&mut app, close.as_position());
    assert!(matches!(app.overlay, Some(Overlay::CloseProject { project }) if project == app.projects[0].id));
}

#[test]
fn the_menu_button_of_a_project_opens_its_menu() {
    let (mut app, _rx, _dirs) = tree(2);
    let menu = ui::row_menu_button(row(&app, TreeRow::Project(1)), 1);
    click_at(&mut app, menu.as_position());
    assert_eq!(menu_labels(&app), ["rename project", "copy path"]);
}

#[test]
fn a_tab_of_another_project_can_be_renamed_from_its_menu() {
    let (mut app, _rx, _dirs) = tree(2);
    let r = row(&app, TreeRow::Tab(0, 0, 0));
    mouse_in(&mut app, MouseEventKind::Down(MouseButton::Right), Position::new(r.x + 10, r.y), TALL);
    assert_eq!(menu_labels(&app), ["rename tab"]);
}

#[test]
fn a_tab_dragged_in_another_project_moves_there() {
    let (mut app, _rx, _dirs) = tree(2);
    app.add_tab(0, 0, AREA).expect("add a tab");
    app.active = 1;
    drawn(&mut app);
    let tabs = |app: &App| -> Vec<u64> { app.projects[0].workspaces[0].tabs.iter().map(|t| t.id).collect() };
    let before = tabs(&app);
    let (from, to) = (row(&app, TreeRow::Tab(0, 0, 1)), row(&app, TreeRow::Tab(0, 0, 0)));

    mouse_in(&mut app, MouseEventKind::Down(MouseButton::Left), Position::new(from.x + 10, from.y), TALL);
    mouse_in(&mut app, MouseEventKind::Drag(MouseButton::Left), Position::new(to.x + 10, to.y), TALL);
    mouse_in(&mut app, MouseEventKind::Up(MouseButton::Left), Position::new(to.x + 10, to.y), TALL);

    assert_eq!(tabs(&app), [before[1], before[0]]);
}

#[test]
fn a_search_result_unfolds_the_way_to_it() {
    let (mut app, _rx, _dirs) = tree(2);
    let id = app.take_id();
    app.groups
        .push(Group { id, entry: group::GroupEntry { name: "work".into(), icon: '●', colour: 4, collapsed: true } });
    app.projects[0].group = Some(id);
    app.projects[0].collapsed = true;
    app.projects[0].workspaces[0].collapsed = true;
    let (project, workspace, tab) = ids(&app, 0);

    app.goto(Goto::Place { project, workspace: Some(workspace), tab: Some(tab) });

    let folds = (app.groups[0].entry.collapsed, app.projects[0].collapsed, app.projects[0].workspaces[0].collapsed);
    assert_eq!(folds, (false, false, false));
}

#[test]
fn a_search_result_for_the_folded_active_project_unfolds_it() {
    let (mut app, _rx, _dirs) = tree(1);
    app.projects[0].collapsed = true;
    let project = app.projects[0].id;

    app.goto(Goto::Place { project, workspace: None, tab: None });

    assert!(!app.projects[0].collapsed);
}

#[test]
fn switching_to_the_tree_shows_the_active_tab() {
    let (mut app, _rx, _dirs) = app_with(10);
    drawn(&mut app);
    app.config.sidebar = config::Sidebar::Tree.id().into();

    drawn(&mut app);

    assert!(!row(&app, TreeRow::Tab(9, 0, 0)).is_empty());
}
