use std::sync::Arc;

use ratatui::style::{Color, Modifier};

use super::splits::three;
use super::*;
use crate::machines::stream::screen::{RemotePane, RemoteScreen};
use crate::project::split::{Dir, Divider};
use crate::term::emulator::Cell;

fn origin() -> Rect {
    Rect { x: 0, y: 0, ..areas().pane }
}

fn mirrored(tab: &TabView, down: bool) -> RemoteTab {
    let area = origin();
    let pane = |(i, rect): (usize, Rect)| RemotePane {
        pane: u64::try_from(i).expect("a pane id"),
        rect,
        active: i == tab.active,
        snapshot: tab.screens[i].clone(),
        ..RemotePane::default()
    };
    let panes = tab.layout.visible(area, tab.active).into_iter().map(pane).collect();
    let screen = RemoteScreen { area, panes, dividers: tab.layout.dividers(area) };
    RemoteTab { screen: Arc::new(screen), dim_inactive: tab.dim_inactive, down, dragging: tab.dragging.clone() }
}

fn there(down: bool) -> View<'static> {
    View { has_project: true, remote_project: true, remote_tab: Some(mirrored(&three(true), down)), ..view(&["~"]) }
}

fn fallen() -> View<'static> {
    let card = RemoteCard {
        machine: "vps".into(),
        agent: Some(("claude".into(), None)),
        folder: "~/code/web-shop".into(),
        down: Some(Retry::In(4)),
    };
    View { remote_card: Some(card), ..there(true) }
}

fn active_pane_corner() -> Position {
    let tab = three(true);
    let (_, rect) = tab.layout.panes(areas().pane)[tab.active];
    rect.as_position()
}

mod live {
    use super::*;

    #[test]
    fn draws_a_split_as_a_tab_here_with_the_same_tree() {
        let mut here =
            render(&View { has_project: true, remote_project: true, tab: Some(three(true)), ..view(&["~"]) });

        let mut shown = render(&there(false));

        assert_eq!(shown.backend().buffer(), here.backend().buffer());
        assert_eq!(shown.get_cursor_position().ok(), here.get_cursor_position().ok());
    }

    #[test]
    fn lights_the_divider_dragged_there_as_one_dragged_here() {
        let dragged = || TabView { dragging: Some(vec![true]), ..three(true) };
        let here = render(&View { has_project: true, remote_project: true, tab: Some(dragged()), ..view(&["~"]) });

        let shown = render(&View { remote_tab: Some(mirrored(&dragged(), false)), ..there(false) });

        assert_eq!(shown.backend().buffer(), here.backend().buffer());
        assert!(shown.backend().buffer().content().iter().any(|cell| cell.fg == Color::Cyan));
    }

    #[test]
    fn leaves_its_active_pane_bright() {
        let at = active_pane_corner();

        let style = render(&there(false)).backend().buffer()[(at.x, at.y)].style();

        assert!(!style.add_modifier.contains(Modifier::DIM));
    }
}

mod fallen {
    use super::*;

    #[test]
    fn dims_the_last_frame_under_the_machine_s_card() {
        insta::assert_snapshot!(render(&fallen()).backend());
    }

    #[test]
    fn dims_the_active_pane_too() {
        let at = active_pane_corner();

        let style = render(&fallen()).backend().buffer()[(at.x, at.y)].style();

        assert!(style.add_modifier.contains(Modifier::DIM));
    }
}

mod a_hostile_frame {
    use super::*;

    const INSIDE: Rect = Rect { x: 10, y: 2, width: 20, height: 5 };

    fn huge() -> RemoteTab {
        let row = vec![Cell { symbol: "x".into(), ..Cell::default() }; 300];
        let snapshot = Snapshot { rows: vec![row; 100], ..Snapshot::default() };
        let rect = Rect::new(0, 0, 300, 100);
        let pane = RemotePane { rect, active: true, snapshot, ..RemotePane::default() };
        let crossing = Divider::seen(Vec::new(), Dir::Right, rect, Rect::new(5, 0, 1, 100), None);
        let beyond = Divider::seen(vec![true], Dir::Down, rect, Rect::new(0, 400, 300, 1), None);
        let screen = RemoteScreen { area: rect, panes: vec![pane], dividers: vec![crossing, beyond] };
        RemoteTab { screen: Arc::new(screen), dim_inactive: true, down: false, dragging: None }
    }

    #[test]
    fn draws_nothing_outside_the_pane_area() {
        let mut t = Terminal::new(TestBackend::new(W, H)).expect("test backend");
        let v = view(&["~"]);

        t.draw(|f| crate::ui::remote_tab::draw_remote_tab(f, &v, &huge(), INSIDE)).expect("draw");

        let buffer = t.backend().buffer();
        let mut outside = buffer.area.positions().filter(|at| !INSIDE.contains(*at));
        assert!(outside.all(|at| buffer[(at.x, at.y)].symbol() == " "));
        assert_eq!(buffer[(INSIDE.x, INSIDE.y)].symbol(), "x");
        assert_eq!(buffer[(INSIDE.x + 5, INSIDE.bottom() - 1)].symbol(), "│");
    }
}
