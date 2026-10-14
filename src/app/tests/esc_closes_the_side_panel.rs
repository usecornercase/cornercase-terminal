use rstest::rstest;

use super::*;

fn with_cat() -> (App, Receiver<AppEvent>, TempDir) {
    let repo = git_repo(&[("a.txt", "one\n")]);
    let (mut app, rx) = app_in(repo.path(), no_config());
    type_line(&mut app, "echo cat-\"\"starts; cat -v");
    shows(&mut app, &rx, "cat-starts");
    (app, rx, repo)
}

fn open(app: &mut App, side: Side) {
    match side {
        Side::Changes => app.toggle_changes(),
        Side::Files => app.toggle_files(),
        Side::Todo => app.toggle_todo(),
        Side::Flow => app.toggle_flow(),
    }
    assert!(app.panel_shown());
}

fn focus_field(app: &mut App, side: Side) {
    let area = app.layout(AREA).changes;
    let field = match side {
        Side::Changes => ui::changes::filter_button(area),
        Side::Files => ui::files::field(area),
        Side::Todo => ui::todo::rows(area, &app.todo_view(area)).button(),
        Side::Flow => unreachable!("the flow panel has no field"),
    };
    click(app, field.as_position());
    assert!(typing(app));
}

fn typing(app: &App) -> bool {
    app.todo_typing() || app.files_typing() || app.filtering()
}

fn esc(app: &mut App) {
    send_key(app, KeyCode::Esc, KeyModifiers::NONE);
}

fn only_the_next_esc_reaches_the_pane(app: &mut App, rx: &Receiver<AppEvent>) {
    type_text(app, "a");
    esc(app);
    type_text(app, "b");
    send_key(app, KeyCode::Enter, KeyModifiers::NONE);
    shows(app, rx, "a^[b");
    assert!(!screen(app).contains("^[a"), "{}", screen(app));
}

#[rstest]
#[case::changes(Side::Changes)]
#[case::files(Side::Files)]
#[case::todo(Side::Todo)]
fn closes_the_panel_without_reaching_the_pane(#[case] side: Side) {
    let (mut app, rx, _repo) = with_cat();
    open(&mut app, side);
    esc(&mut app);
    assert!(!app.panel_shown());
    only_the_next_esc_reaches_the_pane(&mut app, &rx);
}

#[rstest]
#[case::changes(Side::Changes)]
#[case::files(Side::Files)]
#[case::todo(Side::Todo)]
fn a_field_with_text_clears_first_and_closes_on_the_next(#[case] side: Side) {
    let (mut app, rx, _repo) = with_cat();
    open(&mut app, side);
    focus_field(&mut app, side);
    type_text(&mut app, "x");
    esc(&mut app);
    assert_eq!((app.panel_shown(), typing(&app)), (true, false));
    esc(&mut app);
    assert!(!app.panel_shown());
    only_the_next_esc_reaches_the_pane(&mut app, &rx);
}

#[rstest]
#[case::changes(Side::Changes)]
#[case::files(Side::Files)]
#[case::todo(Side::Todo)]
fn an_empty_field_closes_at_once(#[case] side: Side) {
    let (mut app, rx, _repo) = with_cat();
    open(&mut app, side);
    focus_field(&mut app, side);
    esc(&mut app);
    assert_eq!((app.panel_shown(), typing(&app)), (false, false));
    only_the_next_esc_reaches_the_pane(&mut app, &rx);
}

#[test]
fn the_changes_panel_keeps_its_filter() {
    let (mut app, _rx, _repo) = with_cat();
    open(&mut app, Side::Changes);
    focus_field(&mut app, Side::Changes);
    submit_text(&mut app, "*.txt");
    esc(&mut app);
    open(&mut app, Side::Changes);
    assert_eq!(app.changes.filter.as_ref().map(changes::filter::Filter::query), Some("*.txt"));
}

#[test]
fn without_a_panel_esc_reaches_the_pane() {
    let (mut app, rx, _repo) = with_cat();
    esc(&mut app);
    type_text(&mut app, "b");
    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    shows(&mut app, &rx, "^[b");
}

#[test]
fn a_changes_panel_that_is_not_shown_leaves_esc_to_the_pane() {
    let plain = TempDir::new();
    let (mut app, rx) = app_in(plain.path(), no_config());
    app.side = Some(Side::Changes);
    type_line(&mut app, "echo cat-\"\"starts; cat -v");
    shows(&mut app, &rx, "cat-starts");
    esc(&mut app);
    type_text(&mut app, "b");
    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    shows(&mut app, &rx, "^[b");
    assert_eq!(app.side, Some(Side::Changes));
}

#[test]
fn in_compact_mode_too() {
    const SMALL: Rect = Rect { x: 0, y: 0, width: 80, height: 40 };
    let (mut app, rx, _repo) = with_cat();
    open(&mut app, Side::Files);
    app.handle_event(AppEvent::Input(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))), SMALL)
        .expect("handle key");
    assert!(!app.panel_shown());
    only_the_next_esc_reaches_the_pane(&mut app, &rx);
}

#[test]
fn a_drag_takes_esc_first() {
    let (mut app, _rx, _repo) = with_cat();
    open(&mut app, Side::Todo);
    press(&mut app, entry_pos());
    esc(&mut app);
    assert_eq!((app.drag.is_none(), app.panel_shown()), (true, true));
    esc(&mut app);
    assert!(!app.panel_shown());
}

#[test]
fn a_drag_of_the_panel_border_takes_esc_first_and_puts_the_pointer_back() {
    let (mut app, _rx, _repo) = with_cat();
    open(&mut app, Side::Todo);
    let r = app.layout(AREA).border(ui::Border::Changes);
    press(&mut app, Position::new(r.x, r.y + 1));
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), Position::new(AREA.width - 1, r.y + 1));
    assert_eq!(app.pointer(AREA), pointer::Shape::Columns);
    esc(&mut app);
    assert_eq!((app.drag.as_ref(), app.panel_shown(), app.pointer(AREA)), (None, true, pointer::Shape::Default));
}

#[test]
fn a_menu_takes_esc_first() {
    let (mut app, _rx, _repo) = with_cat();
    open(&mut app, Side::Todo);
    right_click(&mut app, entry_pos());
    assert!(matches!(app.overlay, Some(Overlay::Menu { .. })));
    esc(&mut app);
    assert_eq!((app.overlay.is_none(), app.panel_shown()), (true, true));
    esc(&mut app);
    assert!(!app.panel_shown());
}
