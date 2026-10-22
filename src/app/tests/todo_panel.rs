use super::*;
use crate::state;
use crate::ui::todo as panel;

fn toggle_button(app: &mut App) -> String {
    tap(app, |a| panel::check(item_row(a, 0)));
    todo_button(app)
}

fn edited_typo() -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, dirs) = opened();
    add(&mut app, &["fix logn", "b"]);
    let row = item_row(&app, 0);
    click(&mut app, Position::new(row.x + panel::TEXT_X + 7, row.y));
    type_text(&mut app, "i");
    (app, rx, dirs)
}

fn tap(app: &mut App, at: impl Fn(&App) -> Rect) {
    let pos = at(app).as_position();
    click(app, pos);
}

fn open(app: &mut App) {
    tap(app, |a| a.layout(AREA).todo_button);
}

fn view(app: &App) -> panel::View {
    app.todo_view(panel_area(app))
}

fn rows(app: &App) -> ui::Rows {
    panel::rows(panel_area(app), &view(app))
}

fn item_row(app: &App, i: usize) -> Rect {
    rows(app).item(i)
}

fn new_todo(app: &mut App) {
    tap(app, |a| rows(a).button());
}

fn add(app: &mut App, texts: &[&str]) {
    new_todo(app);
    for text in texts {
        submit_text(app, text);
    }
    send_key(app, KeyCode::Enter, KeyModifiers::NONE);
}

fn texts(app: &App) -> Vec<String> {
    app.panels.todos.items().iter().map(|i| if i.done { format!("[x] {}", i.text) } else { i.text.clone() }).collect()
}

fn undo(app: &mut App, message: &str) {
    click(
        app,
        ui::toast_button(AREA, ui::Toast { message, icon: ui::ToastIcon::Check, button: Some(ui::ToastButton::Undo) })
            .as_position(),
    );
}

fn opened() -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, dirs) = app_with(1);
    open(&mut app);
    (app, rx, dirs)
}

#[test]
fn the_button_opens_the_list_beside_the_pane() {
    let (mut app, _rx, _dirs) = app_with(1);
    let before = app.layout(AREA).pane.width;
    open(&mut app);
    assert_eq!((app.screen.side == Some(Side::Todo), app.layout(AREA).pane.width < before), (true, true));
}

#[test]
fn the_close_button_hides_the_panel() {
    let (mut app, _rx, _dirs) = opened();
    tap(&mut app, |a| panel::close(panel_area(a)));
    assert_ne!(app.screen.side, Some(Side::Todo));
}

#[test]
fn the_todo_and_changes_panels_take_turns() {
    let repo = git_repo(&[("README", "hi")]);
    let (mut app, _rx) = app_in(repo.path(), no_config());
    open(&mut app);
    tap(&mut app, |a| ui::changes_button(a.layout(AREA).issues, &a.changes_label().expect("changes")));
    assert_eq!((app.screen.side == Some(Side::Todo), app.screen.side == Some(Side::Changes)), (false, true));
    open(&mut app);
    assert_eq!((app.screen.side == Some(Side::Todo), app.screen.side == Some(Side::Changes)), (true, false));
}

#[test]
fn the_compact_bar_has_a_button_too() {
    let small = Rect { width: 80, ..AREA };
    let (mut app, _rx, _dirs) = app_with(1);
    click_in(&mut app, ui::layout(small, state::Widths::default()).todo_button.as_position(), small);
    assert_eq!(app.screen.side, Some(Side::Todo));
}

#[test]
fn enter_adds_an_item_and_keeps_the_field_open_for_the_next() {
    let (mut app, _rx, _dirs) = opened();
    new_todo(&mut app);
    submit_text(&mut app, "fix login");
    submit_text(&mut app, "write docs");
    let field = app.panels.todo.field().map(|f| f.editor.text());
    assert_eq!((texts(&app), field), (vec!["fix login".into(), "write docs".into()], Some(String::new())));
}

#[test]
fn enter_on_an_empty_field_closes_it() {
    let (mut app, _rx, _dirs) = opened();
    new_todo(&mut app);
    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!((app.panels.todo.field().is_none(), texts(&app)), (true, Vec::<String>::new()));
}

#[test]
fn esc_drops_what_was_typed() {
    let (mut app, _rx, _dirs) = opened();
    new_todo(&mut app);
    type_text(&mut app, "renew the domain");
    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!((app.panels.todo.field().is_none(), texts(&app)), (true, Vec::<String>::new()));
}

#[test]
fn a_pasted_text_goes_into_the_field() {
    let (mut app, _rx, _dirs) = opened();
    new_todo(&mut app);
    app.handle_event(AppEvent::Input(Event::Paste("call\nthe bank".into())), AREA).expect("paste");
    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(texts(&app), ["call the bank"]);
}

#[test]
fn a_press_on_the_pane_saves_the_field_and_gives_the_keys_back() {
    let (mut app, rx, _dirs) = opened();
    new_todo(&mut app);
    type_text(&mut app, "renew the domain");
    click(&mut app, areas().pane.as_position());
    assert_eq!((app.panels.todo.field().is_none(), texts(&app)), (true, vec!["renew the domain".into()]));
    type_in_pane(&mut app, &rx, "shell");
}

#[test]
fn clicking_the_text_edits_it_with_the_cursor_where_clicked() {
    let (mut app, _rx, _dirs) = opened();
    add(&mut app, &["fix logn"]);
    let row = item_row(&app, 0);
    click(&mut app, Position::new(row.x + panel::TEXT_X + 7, row.y));
    assert_eq!(app.panels.todo.field().map(|f| f.editor.cursor()), Some(7));
    submit_text(&mut app, "i");
    assert_eq!((app.panels.todo.field().is_none(), texts(&app)), (true, vec!["fix login".into()]));
}

#[test]
fn arrows_move_the_cursor_in_the_field() {
    let (mut app, _rx, _dirs) = opened();
    new_todo(&mut app);
    type_text(&mut app, "fix logn");
    send_key(&mut app, KeyCode::Left, KeyModifiers::NONE);
    submit_text(&mut app, "i");
    type_text(&mut app, "x");
    send_key(&mut app, KeyCode::Home, KeyModifiers::NONE);
    send_key(&mut app, KeyCode::Delete, KeyModifiers::NONE);
    submit_text(&mut app, "F");
    assert_eq!(texts(&app), ["fix login", "F"]);
}

#[test]
fn ctrl_and_the_arrows_move_the_cursor_by_word() {
    let (mut app, _rx, _dirs) = opened();
    new_todo(&mut app);
    type_text(&mut app, "fix the login");
    send_key(&mut app, KeyCode::Left, KeyModifiers::CONTROL);
    submit_text(&mut app, "x");
    assert_eq!(texts(&app), ["fix the xlogin"]);
}

#[test]
fn checking_an_item_moves_it_to_the_bottom() {
    let (mut app, _rx, _dirs) = opened();
    add(&mut app, &["a", "b"]);
    tap(&mut app, |a| panel::check(item_row(a, 0)));
    assert_eq!(texts(&app), ["b", "[x] a"]);
}

#[test]
fn checking_an_item_being_edited_saves_the_text_and_checks_it() {
    let (mut app, _rx, _dirs) = edited_typo();
    tap(&mut app, |a| panel::check(item_row(a, 0)));
    assert_eq!((app.panels.todo.field().is_none(), texts(&app)), (true, vec!["b".into(), "[x] fix login".into()]));
}

#[test]
fn deleting_an_item_being_edited_saves_the_text_and_undo_brings_it_back() {
    let (mut app, _rx, _dirs) = edited_typo();
    tap(&mut app, |a| panel::delete(item_row(a, 0)));
    assert_eq!((app.panels.todo.field().is_none(), texts(&app)), (true, vec!["b".into()]));
    undo(&mut app, "deleted");
    assert_eq!(texts(&app), ["fix login", "b"]);
}

#[test]
fn deleting_an_item_shows_a_toast_that_undoes_it() {
    let (mut app, _rx, _dirs) = opened();
    add(&mut app, &["a", "b"]);
    tap(&mut app, |a| panel::delete(item_row(a, 0)));
    assert_eq!((texts(&app), toast(&app)), (vec!["b".into()], Some("deleted")));
    undo(&mut app, "deleted");
    assert_eq!((texts(&app), toast(&app)), (vec!["a".into(), "b".into()], None));
}

#[test]
fn the_undo_toast_lasts_longer_than_the_others() {
    let (mut app, _rx, _dirs) = opened();
    add(&mut app, &["a"]);
    tap(&mut app, |a| panel::delete(item_row(a, 0)));
    assert_eq!(app.messages.toast.as_ref().map(Toast::lasts), Some(UNDO_FOR));
}

#[test]
fn clearing_done_items_can_be_undone() {
    let (mut app, _rx, _dirs) = opened();
    add(&mut app, &["a", "b", "c"]);
    tap(&mut app, |a| panel::check(item_row(a, 0)));
    tap(&mut app, |a| panel::check(item_row(a, 0)));
    tap(&mut app, |a| panel::clear_done(panel_area(a), &view(a)));
    assert_eq!((texts(&app), toast(&app)), (vec!["c".into()], Some("cleared 2 done")));
    undo(&mut app, "cleared 2 done");
    assert_eq!(texts(&app), ["c", "[x] a", "[x] b"]);
}

fn todo_button(app: &mut App) -> String {
    let side = app.screen.side.take();
    let r = app.layout(AREA).todo_button;
    let t = render(app, AREA);
    app.screen.side = side;
    (r.x - 2..r.right()).map(|x| t.backend().buffer()[(x, r.y)].symbol().to_string()).collect()
}

#[test]
fn the_button_shows_a_dot_while_items_are_pending() {
    let (mut app, _rx, _dirs) = opened();
    let mut seen = vec![todo_button(&mut app)];
    add(&mut app, &["a"]);
    seen.push(todo_button(&mut app));
    seen.push(toggle_button(&mut app));
    seen.push(toggle_button(&mut app));
    tap(&mut app, |a| panel::check(item_row(a, 0)));
    tap(&mut app, |a| panel::clear_done(panel_area(a), &view(a)));
    seen.push(todo_button(&mut app));
    assert_eq!(seen, ["   TODO ", " TODO ● ", "   TODO ", " TODO ● ", "   TODO "]);
}

#[test]
fn switched_off_the_button_shows_no_dot() {
    let (mut app, _rx, _dirs) = opened();
    add(&mut app, &["a"]);
    app.config.todo_dot = false;
    assert_eq!(todo_button(&mut app), "   TODO ");
}

#[test]
fn dragging_an_item_reorders_the_list() {
    let (mut app, _rx, _dirs) = opened();
    add(&mut app, &["a", "b", "c"]);
    let (from, to) = (item_row(&app, 0), item_row(&app, 2).as_position());
    press(&mut app, from.as_position());
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), to);
    assert_eq!(view(&app).drag, Some((app.panels.todos.items()[0].id, Some(3))));
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), to);
    assert_eq!((texts(&app), app.panels.todo.field().is_none()), (vec!["b".into(), "c".into(), "a".into()], true));
}

#[test]
fn the_list_is_the_same_in_every_project() {
    let (mut app, _rx, _dirs) = app_with(2);
    open(&mut app);
    add(&mut app, &["anywhere"]);
    app.active = 0;
    assert_eq!(view(&app).items.len(), 1);
}

#[test]
fn the_panel_and_the_list_come_back_after_a_restart() {
    let (mut app, _rx, _dirs) = opened();
    add(&mut app, &["a"]);
    let (state, saved) = (app.state(), app.todos_saved());
    let (mut back, _rx2) = empty_app();
    back.set_todos(Todos::from(saved));
    back.restore(&state, AREA);
    assert_eq!((state.todo, back.screen.side == Some(Side::Todo), texts(&back)), (true, true, vec!["a".into()]));
}
