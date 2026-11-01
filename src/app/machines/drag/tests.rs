use crossterm::event::KeyModifiers;
use rstest::rstest;

use super::*;
use crate::control::surface::{self, Modes, Size};
use crate::control::terminal::InputEvent;
use crate::control::terminal::{Button, Mouse, MouseKind};
use crate::control::{Input, Layout};
use crate::project::split::Node;

const AREA: Rect = Rect { x: 2, y: 1, width: 40, height: 6 };
const TAB: u64 = 13;
const PANE: u64 = 1;
const SIZE: Size = Size { cols: 12, rows: 6 };

fn screen(mouse: surface::Mouse) -> RemoteScreen {
    let layout = serde_json::json!({"split": {"dir": "right", "ratio": 0.5, "first": {"leaf": 1},
        "second": {"split": {"dir": "right", "ratio": 0.5, "first": {"leaf": 2}, "second": {"leaf": 3}}}}});
    let node = Node::from(&serde_json::from_value::<Layout>(layout).expect("a layout"));
    let area = Rect::new(0, 0, 40, 6);
    let modes = Modes { mouse, ..Modes::default() };
    let rect = node.panes(area)[0].1;
    let pane = RemotePane { pane: PANE, rect, size: SIZE, active: true, modes, ..RemotePane::default() };
    RemoteScreen { area, panes: vec![pane], dividers: node.dividers(area) }
}

fn on(kind: MouseEventKind, column: u16) -> MouseEvent {
    MouseEvent { kind, column, row: 3, modifiers: KeyModifiers::NONE }
}

fn picked(from: (u16, u16), to: (u16, u16)) -> Selection {
    Selection {
        pane: Some(PANE),
        size: Some(SIZE),
        from: Some(Point { x: from.0, y: from.1 }),
        to: Some(Point { x: to.0, y: to.1 }),
        ..Selection::default()
    }
}

fn selecting(to: Option<(u16, u16)>) -> RemoteDrag {
    RemoteDrag::Selection { pane: PANE, from: Position::new(1, 1), to: to.map(|(x, y)| Position::new(x, y)) }
}

fn mouse(kind: MouseKind, x: u16, y: u16) -> Order {
    let events = vec![InputEvent::Mouse(Mouse { kind, button: Button::Left, x, y, mods: Vec::new() })];
    Order::Input(Input { pane: Some(PANE), popup: false, size: Some(SIZE), events })
}

fn ratio(path: Vec<bool>, ratio: f32) -> Order {
    Order::Ratio(Ratio { tab: Some(TAB), path, ratio: f64::from(ratio) })
}

fn divider(path: Vec<bool>) -> RemoteDrag {
    RemoteDrag::Divider { tab: TAB, path }
}

const DRAG: MouseEventKind = MouseEventKind::Drag(MouseButton::Left);
const UP: MouseEventKind = MouseEventKind::Up(MouseButton::Left);

#[rstest]
#[case::a_divider_moves_to_the_pointer(surface::Mouse::None, divider(Vec::new()), on(DRAG, 14),
    vec![ratio(Vec::new(), 12.0 / 38.0)], Some(divider(Vec::new())))]
#[case::a_divider_keeps_the_least_its_sides_take_there(surface::Mouse::None, divider(Vec::new()), on(DRAG, 40),
    vec![ratio(Vec::new(), 16.0 / 38.0)], Some(divider(Vec::new())))]
#[case::a_divider_gone_there_sends_nothing(surface::Mouse::None, divider(vec![false]), on(DRAG, 14), vec![],
    Some(divider(vec![false])))]
#[case::a_divider_let_go_ends(surface::Mouse::None, divider(Vec::new()), on(UP, 14), vec![], None)]
#[case::a_selection_follows_the_pointer(surface::Mouse::None, selecting(None), on(DRAG, 6),
    vec![Order::Select(picked((1, 1), (4, 2)))], Some(selecting(Some((4, 2)))))]
#[case::a_selection_let_go_is_copied_here(surface::Mouse::None, selecting(Some((4, 2))), on(UP, 7),
    vec![Order::Copy(picked((1, 1), (5, 2)))], None)]
#[case::a_press_let_go_where_it_was_clears_the_selection(surface::Mouse::None, selecting(None), on(UP, 3),
    vec![Order::Select(Selection { pane: Some(PANE), clear: true, ..Selection::default() })], None)]
#[case::a_drag_to_a_program_goes_to_the_pane_held(surface::Mouse::ButtonMotion, RemoteDrag::Mouse { pane: PANE },
    on(DRAG, 6), vec![mouse(MouseKind::Drag, 4, 2)], Some(RemoteDrag::Mouse { pane: PANE }))]
#[case::a_release_to_a_program_ends_the_hold(surface::Mouse::ButtonMotion, RemoteDrag::Mouse { pane: PANE }, on(UP,
    6), vec![mouse(MouseKind::Up, 4, 2)], None)]
#[case::a_press_while_holding_ends_the_hold(surface::Mouse::ButtonMotion, RemoteDrag::Mouse { pane: PANE },
    on(MouseEventKind::Down(MouseButton::Left), 6), vec![], None)]
#[case::a_pane_gone_there_ends_the_hold(surface::Mouse::ButtonMotion, RemoteDrag::Mouse { pane: 9 }, on(UP, 6), vec![],
    None)]
fn a_drag_on_a_machine_s_tab(
    #[case] reads: surface::Mouse,
    #[case] drag: RemoteDrag,
    #[case] ev: MouseEvent,
    #[case] orders: Vec<Order>,
    #[case] then: Option<RemoteDrag>,
) {
    let pointed = dragged(&screen(reads), drag, ev, AREA);

    assert_eq!(pointed, Pointed { orders, drag: then });
}
