use super::*;

fn needles() -> (TempDir, App, Receiver<AppEvent>) {
    let source = (1..=200)
        .map(|n| {
            if [10, 60, 120].contains(&n) {
                format!("let line_{n} = needle;\n")
            } else {
                format!("let line_{n} = {n};\n")
            }
        })
        .collect::<Vec<_>>()
        .concat();
    file_with(&source)
}

fn searched_needles() -> (TempDir, App, Receiver<AppEvent>) {
    let (repo, mut app, rx) = needles();
    let at = panel::back(panel_area(&app)).as_position();
    click(&mut app, at);
    search(&mut app, &rx, files::Mode::Text, "needle");
    (repo, app, rx)
}

fn field(app: &App) -> Option<Find> {
    file(app).find
}

fn current(app: &App) -> Option<(usize, u32)> {
    field(app)?.current().map(|(i, o)| (i, o.line))
}

fn open_field(app: &mut App) {
    let button = panel::find_button(panel_area(app)).as_position();
    click(app, button);
}

fn key(app: &mut App, code: KeyCode) {
    send_key(app, code, KeyModifiers::NONE);
}

fn workspace(app: &App) -> u64 {
    app.projects[0].workspaces[0].id
}

#[test]
fn the_button_opens_the_field_and_a_press_outside_gives_the_keys_back() {
    let (_repo, mut app, _rx) = needles();
    assert!(field(&app).is_none());
    open_field(&mut app);
    type_text(&mut app, "needle");
    assert_eq!(field(&app).map(|f| (f.text().to_string(), f.focused)), Some(("needle".to_string(), true)));
    assert!(app.files_typing());
    click(&mut app, areas().pane.as_position());
    type_text(&mut app, "x");
    assert_eq!(field(&app).map(|f| (f.text().to_string(), f.focused)), Some(("needle".to_string(), false)));
    let row = panel::find_field(panel_area(&app));
    click(&mut app, Position::new(row.x + 3, row.y));
    assert_eq!(field(&app).map(|f| f.focused), Some(true));
    open_field(&mut app);
    assert_eq!(field(&app).map(|f| f.text().to_string()), Some("needle".to_string()), "the button keeps the field");
}

#[test]
fn typing_goes_to_the_first_match_from_the_top_and_scrolls_only_if_needed() {
    let (_repo, mut app, _rx) = needles();
    open_field(&mut app);
    let at = line_pos(&app, 1);
    for _ in 0..13 {
        mouse(&mut app, MouseEventKind::ScrollDown, at);
    }
    assert_eq!(file(&app).scroll, 39);
    let top = file(&app).scroll;
    type_text(&mut app, "needle");
    assert_eq!(current(&app), Some((1, 60)));
    let shown = usize::from(panel::body(panel_area(&app), &view(&app)).height);
    assert!(60 > top + shown, "line 60 starts off screen");
    assert_eq!(file(&app).scroll, 56);
    let workspace = workspace(&app);
    if let Some(viewer) = app.panels.files.viewer_mut(workspace) {
        viewer.scroll = 0;
    }
    for _ in 0..6 {
        key(&mut app, KeyCode::Backspace);
    }
    type_text(&mut app, "needle");
    assert_eq!((current(&app), file(&app).scroll), (Some((0, 10)), 0));
}

#[test]
fn arrows_enter_and_the_buttons_step_and_wrap() {
    let (_repo, mut app, _rx) = needles();
    open_field(&mut app);
    type_text(&mut app, "needle");
    assert_eq!(current(&app), Some((0, 10)));
    key(&mut app, KeyCode::Down);
    assert_eq!(current(&app), Some((1, 60)));
    key(&mut app, KeyCode::Enter);
    assert_eq!(current(&app), Some((2, 120)));
    key(&mut app, KeyCode::Down);
    assert_eq!(current(&app), Some((0, 10)), "wraps at the end");
    key(&mut app, KeyCode::Up);
    assert_eq!(current(&app), Some((2, 120)), "wraps at the start");
    let area = panel_area(&app);
    click(&mut app, panel::find_previous(area).as_position());
    assert_eq!(current(&app), Some((1, 60)));
    click(&mut app, panel::find_next(area).as_position());
    click(&mut app, panel::find_next(area).as_position());
    assert_eq!(current(&app), Some((0, 10)));
    assert_eq!(field(&app).map(|f| f.count()), Some(3));
}

#[test]
fn escape_clears_then_closes_then_ends_full_screen_then_closes_the_panel() {
    let (_repo, mut app, _rx) = needles();
    let at = panel::full_screen_button(panel_area(&app)).as_position();
    click(&mut app, at);
    open_field(&mut app);
    type_text(&mut app, "needle");
    key(&mut app, KeyCode::Esc);
    assert_eq!(field(&app).map(|f| (f.text().to_string(), f.focused, f.count())), Some((String::new(), true, 0)));
    key(&mut app, KeyCode::Esc);
    assert_eq!((field(&app).is_none(), file(&app).full_screen), (true, true));
    key(&mut app, KeyCode::Esc);
    assert_eq!((file(&app).full_screen, app.screen.side == Some(Side::Files)), (false, true));
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.screen.side, None);
}

#[test]
fn a_text_result_opens_the_field_at_its_match() {
    let (_repo, mut app, rx) = searched_needles();
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Enter);
    settle(&mut app, &rx, "the file opens", |v| matches!(&v.screen, Screen::File(f) if f.content.is_some()));
    assert_eq!(file(&app).selection, Some((60, 60)));
    assert_eq!(field(&app).map(|f| (f.text().to_string(), f.focused)), Some(("needle".to_string(), true)));
    settle(
        &mut app,
        &rx,
        "the matches are counted",
        |v| matches!(&v.screen, Screen::File(f) if f.find.as_ref().is_some_and(|f| f.count() == 3)),
    );
    assert_eq!(current(&app), Some((1, 60)));
    key(&mut app, KeyCode::Down);
    assert_eq!((current(&app), file(&app).selection), (Some((2, 120)), Some((60, 60))));
    let at = panel::back(panel_area(&app)).as_position();
    click(&mut app, at);
    assert!(!results(&view(&app)).is_empty(), "back returns to the results");
}

#[test]
fn a_click_on_a_result_while_the_keys_were_elsewhere_does_not_take_them() {
    let (_repo, mut app, _rx) = searched_needles();
    click(&mut app, areas().pane.as_position());
    let found = results(&view(&app));
    let row = found.iter().position(|r| matches!(r, panel::Found::Line { number: 60, .. })).expect("line 60");
    let body = panel::parts(panel_area(&app)).body;
    click(&mut app, Position::new(body.x + 4, body.y + u16::try_from(row).expect("on screen")));
    assert_eq!(field(&app).map(|f| f.focused), Some(false));
    assert!(!app.files_typing());
}

#[test]
fn a_reread_keeps_the_current_match_near_where_it_was() {
    let (repo, mut app, rx) = needles();
    open_field(&mut app);
    type_text(&mut app, "needle");
    key(&mut app, KeyCode::Down);
    let scroll = file(&app).scroll;
    let before = std::fs::read_to_string(repo.path().join("src/main.rs")).expect("read");
    std::fs::write(repo.path().join("src/main.rs"), format!("// top\n{before}")).expect("edit");
    settle(
        &mut app,
        &rx,
        "the new text is counted",
        |v| matches!(&v.screen, Screen::File(f) if f.find.as_ref().and_then(Find::current).is_some_and(|(_, o)| o.line == 61)),
    );
    assert_eq!((current(&app), file(&app).scroll), (Some((1, 61)), scroll));
}

#[test]
fn the_selection_made_by_hand_stays() {
    let (_repo, mut app, _rx) = needles();
    let at = line_pos(&app, 3);
    click(&mut app, at);
    assert_eq!(file(&app).selection, Some((3, 3)));
    open_field(&mut app);
    type_text(&mut app, "needle");
    key(&mut app, KeyCode::Down);
    assert_eq!(file(&app).selection, Some((3, 3)));
}

#[test]
fn the_x_closes_the_field_and_another_file_or_closing_the_panel_drops_it() {
    let (_repo, mut app, _rx) = needles();
    open_field(&mut app);
    type_text(&mut app, "needle");
    let at = panel::find_close(panel_area(&app)).as_position();
    click(&mut app, at);
    assert!(field(&app).is_none());
    open_field(&mut app);
    let at = panel::back(panel_area(&app)).as_position();
    click(&mut app, at);
    let ws = workspace(&app);
    app.panels.files.open_file(ws, "src/main.rs");
    assert!(field(&app).is_none());
    open_field(&mut app);
    app.show_side(None);
    assert!(app.panels.files.viewer(ws).is_some_and(|v| v.find.is_none()));
}

#[test]
fn a_paste_goes_to_the_field_without_control_characters() {
    let (_repo, mut app, _rx) = needles();
    open_field(&mut app);
    app.handle_event(AppEvent::Input(Event::Paste("nee\ndle".into())), AREA).expect("paste");
    assert_eq!(field(&app).map(|f| f.text().to_string()), Some("needle".to_string()));
    assert_eq!(current(&app), Some((0, 10)));
}

#[test]
fn keys_that_are_not_text_do_not_edit_the_query() {
    let (_repo, mut app, _rx) = needles();
    open_field(&mut app);
    type_text(&mut app, "needle");
    send_key(&mut app, KeyCode::Char('x'), KeyModifiers::CONTROL);
    key(&mut app, KeyCode::Tab);
    assert_eq!(field(&app).map(|f| f.text().to_string()), Some("needle".to_string()));
}
