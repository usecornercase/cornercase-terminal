use super::*;
use crate::config;
use crate::state;
use crate::ui::TreeRow;

const TALL: Rect = Rect { x: 0, y: 0, width: 100, height: 40 };

fn in_tree() -> Seeing {
    let mut seeing = seeing_two();
    seeing.rig.app.config.sidebar = config::Sidebar::Tree.id().into();
    render(&mut seeing.rig.app, TALL);
    seeing
}

fn tree_list() -> Rect {
    ui::layout_with(TALL, state::Widths::default(), false, config::Sidebar::Tree).list
}

fn tree_rect(app: &App, row: TreeRow) -> Rect {
    ui::tree_row(tree_list(), &app.tree_shape(), app.scroll.of(List::Projects), row)
}

fn name_of(app: &App, row: TreeRow) -> Position {
    let r = tree_rect(app, row);
    Position::new(r.x + 10, r.y)
}

fn tap(app: &mut App, pos: Position, button: MouseButton) {
    mouse_in(app, MouseEventKind::Down(button), pos, TALL);
    mouse_in(app, MouseEventKind::Up(button), pos, TALL);
}

#[test]
fn lists_a_machine_s_workspaces_under_their_project_and_its_own_projects_last() {
    let Seeing { rig, .. } = in_tree();
    let w = theirs_at(&rig.app);

    let rows = ui::tree_rows(&rig.app.tree_shape());

    let tail = [TreeRow::Project(1), TreeRow::Workspace(1, 0), TreeRow::Tab(1, 0, 0), TreeRow::Tab(1, 0, 1)];
    assert!(rows.ends_with(&tail), "{rows:?}");
    let theirs = [TreeRow::Workspace(0, w), TreeRow::Tab(0, w, 0), TreeRow::Tab(0, w, 1), TreeRow::NewWorkspace(0)];
    assert!(rows.windows(4).any(|window| window == theirs), "{rows:?}");
    let tree = rig.app.tree_view();
    let tag = |p: usize, w: usize| tree.workspaces[p].get(w).and_then(|e| e.machine.as_ref()).map(|m| m.name.clone());
    assert_eq!((tag(0, w), tag(1, 0), tag(0, 0)), (Some("vps".into()), Some("vps".into()), None));
}

#[test]
fn a_machine_s_row_clicked_picks_it_and_marks_it() {
    let Seeing { mut rig, .. } = in_tree();
    let w = theirs_at(&rig.app);

    let pos = name_of(&rig.app, TreeRow::Tab(0, w, 0));
    tap(&mut rig.app, pos, MouseButton::Left);

    let view = rig.app.lists_view(TALL, Instant::now());
    assert_eq!(picked(&rig.app), Some((10, Some(11))));
    assert_eq!((view.active, view.active_workspace, view.active_tab), (0, w, Some(0)));
}

#[test]
fn a_machine_s_row_clicked_from_another_project_makes_its_pair_the_active_one() {
    let Seeing { mut rig, .. } = in_tree();
    let w = theirs_at(&rig.app);
    let dir = TempDir::new();
    rig.app.open_project(dir.path().to_path_buf(), TALL).expect("open project");
    render(&mut rig.app, TALL);
    assert_eq!(rig.app.active, 1);

    let pos = name_of(&rig.app, TreeRow::Workspace(0, w));
    tap(&mut rig.app, pos, MouseButton::Left);

    let view = rig.app.lists_view(TALL, Instant::now());
    assert_eq!((picked(&rig.app), rig.app.active), (Some((10, Some(13))), 0));
    assert_eq!((view.active, view.active_workspace), (0, w));
}

#[test]
fn a_project_only_the_machine_has_clicked_picks_its_first_workspace() {
    let Seeing { mut rig, .. } = in_tree();

    let pos = name_of(&rig.app, TreeRow::Project(1));
    tap(&mut rig.app, pos, MouseButton::Left);
    tap(&mut rig.app, pos, MouseButton::Right);

    assert_eq!((picked(&rig.app), rig.app.active, rig.app.overlay.is_none()), (Some((20, Some(23))), 0, true));
    let view = rig.app.lists_view(TALL, Instant::now());
    assert_eq!((view.active, view.active_workspace, view.remote_project), (1, 0, true));
}

#[test]
fn a_machine_s_row_folds_with_its_arrow() {
    let Seeing { mut rig, .. } = in_tree();
    let w = theirs_at(&rig.app);

    let shape = rig.app.tree_shape();
    let arrow = ui::tree_arrow(tree_list(), &shape, rig.app.scroll.of(List::Projects), TreeRow::Workspace(0, w));
    tap(&mut rig.app, arrow.as_position(), MouseButton::Left);

    let rows = ui::tree_rows(&rig.app.tree_shape());
    assert!(rows.contains(&TreeRow::Workspace(0, w)) && !rows.contains(&TreeRow::Tab(0, w, 0)), "{rows:?}");
    assert_eq!(picked(&rig.app), None);
}
