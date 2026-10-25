use ratatui::style::Modifier;

use super::*;

fn showing(text: &str) -> (App, Receiver<AppEvent>) {
    let (mut app, rx) = app();
    app.term_mut().expect("a pane").feed(format!("\x1b[2J\x1b[H{text}").as_bytes());
    (app, rx)
}

fn cell(col: u16, row: u16) -> Position {
    Position::new(areas().pane.x + col, areas().pane.y + row)
}

#[test]
fn dragging_over_text_copies_it_to_the_clipboard() {
    let (mut app, _rx) = showing("hello world");

    drag(&mut app, cell(0, 0), cell(4, 0));

    assert_eq!(app.take_host_writes(), [clipboard::osc52("hello")]);
}

#[test]
fn copying_shows_a_toast() {
    let (mut app, _rx) = showing("hello world");

    drag(&mut app, cell(0, 0), cell(4, 0));

    assert_eq!(toast(&app), Some(COPIED));
}

#[test]
fn a_click_copies_nothing() {
    let (mut app, _rx) = showing("hello world");

    click(&mut app, cell(2, 0));

    assert_eq!((app.take_host_writes(), toast(&app)), (Vec::<Vec<u8>>::new(), None));
}

#[test]
fn a_drag_past_the_pane_stops_at_its_edge() {
    let (mut app, _rx) = showing("hello world");

    drag(&mut app, cell(6, 0), Position::new(2, areas().pane.y));

    assert_eq!(app.take_host_writes(), [clipboard::osc52("hello w")]);
}

#[test]
fn the_text_is_highlighted_while_dragging() {
    let (mut app, _rx) = showing("hello world");
    press(&mut app, cell(0, 0));

    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), cell(4, 0));

    let screen = app.term_mut().expect("a pane").emulator.snapshot().expect("snapshot");
    assert!(screen.rows[0][4].style.add_modifier.contains(Modifier::REVERSED));
}

#[test]
fn escape_does_not_end_a_text_selection() {
    let (mut app, _rx) = showing("hello world");
    press(&mut app, cell(0, 0));
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), cell(4, 0));

    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), cell(10, 0));
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), cell(10, 0));

    assert_eq!(app.take_host_writes(), [clipboard::osc52("hello world")]);
}

#[test]
fn a_program_that_wants_the_mouse_gets_the_drag_instead() {
    let (mut app, _rx) = showing("\x1b[?1002hhello world");

    drag(&mut app, cell(0, 0), cell(4, 0));

    assert_eq!((app.screen.drag.clone(), app.take_host_writes()), (None, Vec::<Vec<u8>>::new()));
}

#[test]
fn a_program_that_copies_reaches_the_clipboard_with_a_toast() {
    let (mut app, _rx) = app();
    let id = term(&app, 0).id;

    app.handle_event(AppEvent::Output(id, 0, b"\x1b]52;c;aGVsbG8=\x07".to_vec().into()), AREA).expect("handle output");

    assert_eq!((app.take_host_writes(), toast(&app)), (vec![clipboard::osc52("hello")], Some(COPIED)));
}

#[test]
fn the_toast_goes_away_after_a_while() {
    let (mut app, _rx) = showing("hello world");
    app.messages.toast =
        Instant::now().checked_sub(TOAST_FOR).map(|at| Toast { at, ..Toast::new(COPIED, ui::ToastIcon::Check) });
    render(&mut app, AREA);

    assert_eq!(app.messages.toast, None);
}
