use super::*;

fn in_pane() -> Position {
    Position::new(areas().pane.x + 2, areas().pane.y + 2)
}

fn below(app: &App) -> usize {
    app.tab().and_then(Tab::pane).expect("a pane").emulator.below()
}

fn scrolled_up() -> (App, Receiver<AppEvent>) {
    let (mut app, rx) = app();
    numbered(app.term_mut().expect("a pane"), 100);
    mouse(&mut app, MouseEventKind::ScrollUp, in_pane());
    assert_eq!(below(&app), 3, "the wheel scrolled the pane up");
    (app, rx)
}

#[test]
fn the_wheel_scrolls_a_shell_back_through_its_history() {
    let (mut app, _rx) = scrolled_up();

    let first = 100 - usize::from(areas().pane.height) + 1 - 3;
    assert_eq!(below(&app), 3);
    assert_eq!(screen(&mut app).lines().next(), Some(format!("line-{first}").as_str()));
}

#[test]
fn the_wheel_down_returns_to_the_bottom() {
    let (mut app, _rx) = scrolled_up();

    mouse(&mut app, MouseEventKind::ScrollDown, in_pane());

    assert_eq!(below(&app), 0);
}

#[test]
fn a_key_returns_to_the_bottom() {
    let (mut app, _rx) = scrolled_up();

    send_key(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);

    assert_eq!(below(&app), 0);
}

#[test]
fn a_paste_returns_to_the_bottom() {
    let (mut app, _rx) = scrolled_up();

    app.handle_event(AppEvent::Input(Event::Paste("x".into())), AREA).expect("handle paste");

    assert_eq!(below(&app), 0);
}

#[test]
fn a_click_on_the_badge_returns_to_the_bottom() {
    let (mut app, _rx) = scrolled_up();
    let badge = ui::scrollback_badge(areas().pane, below(&app)).expect("a badge");

    click(&mut app, badge.as_position());

    assert_eq!((below(&app), app.take_host_writes(), toast(&app)), (0, Vec::<Vec<u8>>::new(), None));
}

#[test]
fn a_drag_on_the_scrolled_view_copies_what_is_shown() {
    let (mut app, _rx) = scrolled_up();
    let first = format!("line-{}", 100 - usize::from(areas().pane.height) + 1 - 3);
    let start = areas().pane.as_position();
    let end = Position::new(start.x + u16::try_from(first.len()).expect("short") - 1, start.y);

    press(&mut app, start);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), end);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), end);

    assert_eq!(app.take_host_writes(), [clipboard::osc52(&first)]);
}

#[cfg(target_os = "linux")]
#[test]
fn a_full_pane_that_refuses_the_wheels_arrows_says_so() {
    let (mut app, rx) = app();
    type_line(&mut app, "printf '\\033[?1049h'; stty -echo; sleep 30");
    pump_until(&mut app, &rx, "sleep runs on the alternate screen", |a| {
        a.tab()
            .and_then(Tab::pane)
            .is_some_and(|t| t.emulator.alternate_screen() && t.program(&a.config).as_deref() == Some("sleep"))
    });
    let full = vec![b'x'; crate::term::MAX_QUEUED + 1024 * 1024];
    let filled = app.term_mut().expect("a pane").write(&full);

    mouse(&mut app, MouseEventKind::ScrollUp, in_pane());

    assert_eq!((filled, toast(&app)), (true, Some(NOT_READING)));
}

#[test]
fn a_program_that_reads_the_mouse_keeps_the_wheel() {
    let (mut app, rx) = app();
    type_line(&mut app, "printf '\\033[?1000h\\033[?1006h'; cat -v");
    pump_until(&mut app, &rx, "the program reads the mouse", |a| {
        a.tab().and_then(Tab::pane).is_some_and(|t| t.emulator.mouse_mode() != mouse::MouseMode::None)
    });

    mouse(&mut app, MouseEventKind::ScrollUp, in_pane());

    shows(&mut app, &rx, "^[[<64;");
}

#[test]
fn a_wheel_over_an_inactive_pane_scrolls_it_without_focusing_it() {
    let (mut app, _rx) = app();
    right_click(&mut app, in_pane());
    pick(&mut app, "split right");
    let tab = app.tab().expect("a tab");
    let (active, (left, rect)) = (tab.active, tab.shown(areas().pane)[0]);
    let tab = app.tab_mut().expect("a tab");
    numbered(tab.panes.iter_mut().find(|t| t.id == left).expect("the left pane"), 100);

    mouse(&mut app, MouseEventKind::ScrollUp, Position::new(rect.x + 1, rect.y + 1));

    let tab = app.tab().expect("a tab");
    let left_below = tab.panes.iter().find(|t| t.id == left).expect("the left pane").emulator.below();
    assert_eq!((left_below, tab.active), (3, active));
}

#[test]
fn on_the_alternate_screen_the_wheel_sends_arrows() {
    let (mut app, rx) = app();
    type_line(&mut app, "printf '\\033[?1049h'; cat -v");
    pump_until(&mut app, &rx, "the alternate screen", |a| {
        a.tab().and_then(Tab::pane).is_some_and(|t| t.emulator.alternate_screen())
    });

    mouse(&mut app, MouseEventKind::ScrollUp, in_pane());

    shows(&mut app, &rx, "^[[A^[[A^[[A");
}
