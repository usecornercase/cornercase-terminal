use rstest::rstest;

use super::*;

fn with_bar() -> (App, Receiver<AppEvent>) {
    let (mut app, rx) = app();
    app.config.icon_bar = true;
    (app, rx)
}

fn button(app: &App, item: ui::ActivityItem) -> Position {
    let bar = app.layout(AREA).activity;
    ui::activity_items(bar, app.activity_shown())
        .into_iter()
        .find_map(|(i, r)| (i == item).then(|| Position::new(r.x + 1, r.y)))
        .expect("the button shows")
}

fn drag_projects_border(app: &mut App, by: u16) -> (u16, u16) {
    let r = app.layout(AREA).border(ui::Border::Projects);
    let from = Position::new(r.x, r.y + 2);
    let to = Position::new(r.x + by, from.y);
    press(app, from);
    mouse(app, MouseEventKind::Drag(MouseButton::Left), to);
    mouse(app, MouseEventKind::Up(MouseButton::Left), to);
    (to.x, app.layout(AREA).border(ui::Border::Projects).x)
}

#[rstest]
#[case::files(ui::ActivityItem::Files, Side::Files)]
#[case::todo(ui::ActivityItem::Todo, Side::Todo)]
fn a_panel_button_opens_and_closes_its_panel(#[case] item: ui::ActivityItem, #[case] side: Side) {
    let (mut app, _rx) = with_bar();
    let at = button(&app, item);
    click(&mut app, at);
    let opened = app.screen.side;
    click(&mut app, at);
    assert_eq!((opened, app.screen.side), (Some(side), None));
}

#[test]
fn changes_shows_in_a_repository() {
    let repo = git_repo(&[("a.txt", "one\n")]);
    let (mut app, _rx) = app_in(repo.path(), no_config());
    app.config.icon_bar = true;
    let at = button(&app, ui::ActivityItem::Changes);
    click(&mut app, at);
    assert_eq!(app.screen.side, Some(Side::Changes));
}

#[test]
fn changes_is_left_out_of_a_plain_folder() {
    let (app, _rx, _dirs) = app_with(1);
    assert!(!app.activity_shown().changes);
}

#[test]
fn the_gear_opens_the_more_menu() {
    let (mut app, _rx) = with_bar();
    let at = button(&app, ui::ActivityItem::More);
    click(&mut app, at);
    let labels = menu_labels(&app);
    pick(&mut app, "settings");
    assert_eq!(
        (labels, matches!(app.overlay, Some(Overlay::Settings(_)))),
        (["settings", "usage", "processes", "quit"].map(String::from).to_vec(), true)
    );
}

#[test]
fn the_worktrees_slot_opens_the_worktrees_dialog() {
    let (mut app, _rx) = with_bar();
    let at = button(&app, ui::ActivityItem::Worktrees);
    click(&mut app, at);
    assert!(matches!(app.overlay, Some(Overlay::Worktrees(_))));
}

#[test]
fn quit_in_the_gear_menu_detaches() {
    let (mut app, _rx) = with_bar();
    let at = button(&app, ui::ActivityItem::More);
    click(&mut app, at);
    pick(&mut app, "quit");
    assert!(app.take_detach());
}

#[test]
fn a_press_beside_the_icons_does_nothing() {
    let (mut app, _rx) = with_bar();
    let more = button(&app, ui::ActivityItem::More);
    click(&mut app, Position::new(more.x, more.y - 1));
    assert_eq!((app.overlay.is_none(), app.screen.side), (true, None));
}

#[rstest]
#[case::side_by_side(config::Sidebar::SideBySide)]
#[case::projects_on_top(config::Sidebar::ProjectsOnTop)]
fn the_projects_border_follows_the_pointer(#[case] sidebar: config::Sidebar) {
    let (mut app, _rx) = with_bar();
    app.config.sidebar = sidebar.id().into();
    let (pointer, border) = drag_projects_border(&mut app, 3);
    assert_eq!(border, pointer);
}

#[test]
fn the_stack_line_follows_the_pointer() {
    let (mut app, _rx) = with_bar();
    app.config.sidebar = config::Sidebar::ProjectsOnTop.id().into();
    let r = app.layout(AREA).border(ui::Border::Stack);
    let from = Position::new(r.x + 2, r.y);
    let to = Position::new(from.x, from.y + 2);
    press(&mut app, from);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), to);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), to);
    assert_eq!(app.layout(AREA).border(ui::Border::Stack).y, to.y);
}
