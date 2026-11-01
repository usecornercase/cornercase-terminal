use rstest::rstest;

use super::*;
use crate::control::surface::{Modes, Size};
use crate::control::terminal::{Button, MouseKind};
use crate::project::split::{Dir, Divider};
use crate::term::emulator::Snapshot;

const AREA: Rect = Rect { x: 2, y: 1, width: 30, height: 4 };
const ACTIVE: u64 = 1;
const OTHER: u64 = 2;
const ACTIVE_SIZE: Size = Size { cols: 13, rows: 4 };
const OTHER_SIZE: Size = Size { cols: 14, rows: 4 };
const TAB: u64 = 13;

fn screen(mouse: surface::Mouse, alternate_screen: bool, alternate_scroll: bool) -> RemoteScreen {
    let modes = Modes { mouse, alternate_screen, alternate_scroll, ..Modes::default() };
    let active = RemotePane {
        pane: ACTIVE,
        rect: Rect::new(0, 0, 15, 4),
        size: ACTIVE_SIZE,
        active: true,
        modes,
        ..RemotePane::default()
    };
    let other = RemotePane {
        pane: OTHER,
        rect: Rect::new(16, 0, 14, 4),
        size: OTHER_SIZE,
        snapshot: Snapshot { below: 5, ..Snapshot::default() },
        modes,
        ..RemotePane::default()
    };
    let divider = Divider::seen(Vec::new(), Dir::Right, Rect::new(0, 0, 30, 4), Rect::new(15, 0, 1, 4), None);
    RemoteScreen { area: Rect::new(0, 0, 30, 4), panes: vec![active, other], dividers: vec![divider] }
}

fn at(kind: MouseEventKind, column: u16, row: u16) -> (MouseEvent, Position) {
    (MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE }, Position::new(column, row))
}

fn mouse(pane: u64, size: Size, kind: MouseKind, x: u16, y: u16) -> Order {
    pressed_with(Button::Left, pane, size, kind, (x, y))
}

fn pressed_with(button: Button, pane: u64, size: Size, kind: MouseKind, (x, y): (u16, u16)) -> Order {
    let events = vec![InputEvent::Mouse(Mouse { kind, button, x, y, mods: Vec::new() })];
    Order::Input(Input { pane: Some(pane), popup: false, size: Some(size), events })
}

fn keys(code: &str, times: usize) -> Order {
    let key = InputEvent::Key(Key { code: code.into(), ..Key::default() });
    Order::Input(Input { pane: Some(OTHER), popup: false, size: None, events: vec![key; times] })
}

fn scroll(lines: Option<i64>, to: Option<End>) -> Order {
    Order::Scroll(Scroll { pane: Some(OTHER), lines, to })
}

const NONE: surface::Mouse = surface::Mouse::None;
const PRESS: surface::Mouse = surface::Mouse::PressRelease;
const ANY: surface::Mouse = surface::Mouse::AnyMotion;
const LEFT_DOWN: MouseEventKind = MouseEventKind::Down(MouseButton::Left);
const RIGHT_DOWN: MouseEventKind = MouseEventKind::Down(MouseButton::Right);

#[rstest]
#[case::wheel_scrolls_the_history_under_it(screen(NONE, false, false), MouseEventKind::ScrollUp, 20, 2,
    vec![scroll(Some(-3), None)])]
#[case::wheel_sends_arrows_to_an_alternate_screen(
    screen(NONE, true, true),
    MouseEventKind::ScrollDown,
    20,
    2,
    vec![keys("down", 3)]
)]
#[case::wheel_does_nothing_on_an_alternate_screen_without_arrows(
    screen(NONE, true, false),
    MouseEventKind::ScrollUp,
    20,
    2,
    vec![]
)]
#[case::wheel_goes_to_a_program_reading_the_mouse(
    screen(PRESS, true, true),
    MouseEventKind::ScrollUp,
    20,
    2,
    vec![mouse(OTHER, OTHER_SIZE, MouseKind::ScrollUp, 2, 1)]
)]
#[case::press_goes_to_a_program_reading_the_mouse(
    screen(PRESS, false, false),
    LEFT_DOWN,
    5,
    3,
    vec![mouse(ACTIVE, ACTIVE_SIZE, MouseKind::Down, 3, 2)]
)]
#[case::press_past_the_pane_s_size_is_cut_to_it(
    screen(PRESS, false, false),
    LEFT_DOWN,
    16,
    4,
    vec![mouse(ACTIVE, ACTIVE_SIZE, MouseKind::Down, 12, 3)]
)]
#[case::release_goes_to_the_active_pane(
    screen(PRESS, false, false),
    MouseEventKind::Up(MouseButton::Left),
    25,
    2,
    vec![mouse(ACTIVE, ACTIVE_SIZE, MouseKind::Up, 12, 1)]
)]
#[case::press_without_the_mouse_read_sends_nothing(screen(NONE, false, false), LEFT_DOWN, 5, 3, vec![])]
#[case::press_on_another_pane_focuses_it(screen(PRESS, false, false), LEFT_DOWN, 20, 2, vec![Order::Focus(OTHER)])]
#[case::right_press_on_another_pane_without_the_mouse_read_does_nothing(screen(NONE, false, false), RIGHT_DOWN, 20, 2,
    vec![])]
#[case::right_press_on_another_pane_reading_the_mouse_focuses_it_then_goes_to_it(
    screen(PRESS, false, false),
    RIGHT_DOWN,
    20,
    2,
    vec![Order::Focus(OTHER), pressed_with(Button::Right, OTHER, OTHER_SIZE, MouseKind::Down, (2, 1))]
)]
#[case::press_on_the_badge_goes_to_the_bottom(
    screen(NONE, false, false),
    LEFT_DOWN,
    21,
    4,
    vec![scroll(None, Some(End::Bottom))]
)]
#[case::right_press_on_the_divider_does_nothing(screen(PRESS, false, false), RIGHT_DOWN, 17, 2, vec![])]
#[case::moving_goes_to_a_program_that_asks_for_every_motion(
    screen(ANY, false, false),
    MouseEventKind::Moved,
    5,
    3,
    vec![mouse(ACTIVE, ACTIVE_SIZE, MouseKind::Moved, 3, 2)]
)]
#[case::moving_is_left_out_for_a_program_that_reads_only_presses(
    screen(PRESS, false, false),
    MouseEventKind::Moved,
    5,
    3,
    vec![]
)]
#[case::moving_over_another_pane_is_left_out(screen(ANY, false, false), MouseEventKind::Moved, 20, 2, vec![])]
fn a_pointer_on_a_machine_s_tab_becomes(
    #[case] screen: RemoteScreen,
    #[case] kind: MouseEventKind,
    #[case] column: u16,
    #[case] row: u16,
    #[case] expected: Vec<Order>,
) {
    let (ev, pos) = at(kind, column, row);

    assert_eq!(pointed(&screen, ev, pos, AREA, TAB).orders, expected);
}

#[rstest]
#[case::press_on_the_divider_grabs_it(screen(PRESS, false, false), LEFT_DOWN, 17, 2,
    Some(RemoteDrag::Divider { tab: TAB, path: Vec::new() }))]
#[case::press_without_the_mouse_read_starts_a_selection(screen(NONE, false, false), LEFT_DOWN, 5, 3,
    Some(RemoteDrag::Selection { pane: ACTIVE, from: Position::new(3, 2), to: None }))]
#[case::press_reading_the_mouse_holds_the_pane(screen(PRESS, false, false), LEFT_DOWN, 5, 3,
    Some(RemoteDrag::Mouse { pane: ACTIVE }))]
#[case::right_press_on_another_pane_reading_the_mouse_holds_it(screen(PRESS, false, false), RIGHT_DOWN, 20, 2,
    Some(RemoteDrag::Mouse { pane: OTHER }))]
#[case::right_press_without_the_mouse_read_starts_nothing(screen(NONE, false, false), RIGHT_DOWN, 5, 3, None)]
#[case::press_on_another_pane_starts_nothing(screen(NONE, false, false), LEFT_DOWN, 20, 2, None)]
#[case::press_on_the_badge_starts_nothing(screen(NONE, false, false), LEFT_DOWN, 21, 4, None)]
fn a_press_on_a_machine_s_tab_starts(
    #[case] screen: RemoteScreen,
    #[case] kind: MouseEventKind,
    #[case] column: u16,
    #[case] row: u16,
    #[case] expected: Option<RemoteDrag>,
) {
    let (ev, pos) = at(kind, column, row);

    assert_eq!(pointed(&screen, ev, pos, AREA, TAB).drag, expected);
}
