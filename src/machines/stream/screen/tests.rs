use ratatui::style::{Color, Modifier};
use rstest::rstest;
use serde_json::{Value, json};

use super::*;

fn cells(text: &str) -> Vec<String> {
    text.chars().map(|c| c.to_string()).collect()
}

fn pane_json(id: u64, rect: [u16; 4], lines: &Value) -> Value {
    json!({
        "pane": id, "active": id == 1,
        "rect": {"x": rect[0], "y": rect[1], "cols": rect[2], "rows": rect[3]},
        "size": {"cols": rect[2], "rows": rect[3]},
        "lines": lines
    })
}

fn frame(cols: u16, rows: u16, content: &Value) -> Mirror {
    let mut ok = json!({"revision": 1, "cols": cols, "rows": rows});
    for (key, value) in content.as_object().expect("an object") {
        ok[key] = value.clone();
    }
    let mut mirror = Mirror::default();
    mirror.apply(&json!({"ok": ok}).to_string()).expect("a frame");
    mirror
}

fn one_pane(cols: u16, rows: u16, lines: &Value) -> RemoteScreen {
    let content = json!({"layout": {"leaf": 1}, "styles": [{}], "panes": [pane_json(1, [0, 0, cols, rows], lines)]});
    RemoteScreen::of(&frame(cols, rows, &content))
}

fn text(pane: &RemotePane, y: usize) -> String {
    pane.snapshot.rows[y].iter().map(|cell| cell.symbol.as_str()).collect::<String>()
}

fn split() -> RemoteScreen {
    let content = json!({
        "layout": {"split": {"dir": "right", "ratio": 0.5, "first": {"leaf": 1}, "second": {"leaf": 2}}},
        "dividers": [{"dir": "right", "path": [], "area": {"x": 0, "y": 0, "cols": 21, "rows": 4},
                      "line": {"x": 10, "y": 0, "cols": 1, "rows": 4}}],
        "styles": [{}, {"bold": true, "fg": {"index": 2}}],
        "panes": [
            pane_json(1, [0, 0, 10, 4], &json!([[{"style": 1, "cells": cells("left")}]])),
            pane_json(2, [12, 0, 9, 4], &json!([[{"style": 0, "cells": cells("right")}]])),
        ]
    });
    RemoteScreen::of(&frame(21, 4, &content))
}

fn inside(screen: &RemoteScreen) {
    for pane in &screen.panes {
        assert_eq!(pane.rect.intersection(screen.area), pane.rect, "pane {}", pane.pane);
        assert_eq!(pane.snapshot.rows.len(), usize::from(pane.rect.height), "pane {}", pane.pane);
        for row in &pane.snapshot.rows {
            assert_eq!(row.len(), usize::from(pane.rect.width), "pane {}", pane.pane);
        }
    }
    for divider in &screen.dividers {
        assert_eq!(divider.line.intersection(screen.area), divider.line);
    }
}

mod a_frame {
    use super::*;

    #[test]
    fn keeps_each_pane_s_rect_and_the_dividers() {
        let screen = split();

        assert_eq!(screen.area, Rect::new(0, 0, 21, 4));
        assert_eq!(
            screen.panes.iter().map(|pane| pane.rect).collect::<Vec<_>>(),
            [Rect::new(0, 0, 10, 4), Rect::new(12, 0, 9, 4)]
        );
        assert_eq!(
            screen.dividers.iter().map(|d| (d.dir, d.line)).collect::<Vec<_>>(),
            [(crate::project::split::Dir::Right, Rect::new(10, 0, 1, 4))]
        );
    }

    #[rstest]
    #[case::from_its_layout(true)]
    #[case::of_a_pane_each_without_one(false)]
    fn gives_each_divider_the_least_its_sides_take(#[case] with_layout: bool) {
        let layout = json!({"split": {"dir": "right", "ratio": 0.6,
            "first": {"split": {"dir": "right", "ratio": 0.5, "first": {"leaf": 1}, "second": {"leaf": 3}}},
            "second": {"leaf": 2}}});
        let node = Node::from(&serde_json::from_value::<crate::control::Layout>(layout.clone()).expect("a layout"));
        let area = Rect::new(0, 0, 60, 4);
        let at = |rect: Rect| json!({"x": rect.x, "y": rect.y, "cols": rect.width, "rows": rect.height});
        let dividers: Vec<Value> = node
            .dividers(area)
            .iter()
            .map(|d| json!({"dir": d.dir, "path": d.path, "area": at(d.area), "line": at(d.line)}))
            .collect();
        let mut content = json!({"dividers": dividers, "styles": [{}], "panes": []});
        if with_layout {
            content["layout"] = layout;
        }

        let screen = RemoteScreen::of(&frame(60, 4, &content));

        let expected: Vec<Divider> = if with_layout {
            node.dividers(area)
        } else {
            node.dividers(area).into_iter().map(|d| Divider::seen(d.path, d.dir, d.area, d.line, None)).collect()
        };
        assert_eq!(screen.dividers, expected);
    }

    #[test]
    fn becomes_cells_with_the_styles_of_its_table() {
        let screen = split();
        let left = &screen.panes[0].snapshot.rows[0][0];

        assert_eq!(text(&screen.panes[0], 0), "left      ");
        assert_eq!(left.style.fg, Some(Color::Indexed(2)));
        assert!(left.style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(text(&screen.panes[1], 3), "         ");
    }

    #[test]
    fn keeps_which_pane_is_active_and_its_cursor() {
        let mut mirror = frame(10, 2, &json!({"panes": [pane_json(1, [0, 0, 10, 2], &json!([]))]}));
        mirror.shown.panes[0].cursor = Some(surface::Cursor { x: 3, y: 1, shape: Shape::Bar, blinking: true });

        let screen = RemoteScreen::of(&mirror);

        let pane = &screen.panes[0];
        assert!(pane.active);
        assert_eq!(
            (pane.snapshot.cursor, pane.snapshot.shape, pane.snapshot.blinking),
            (Some(Position::new(3, 1)), CursorShape::Bar, true)
        );
    }

    #[test]
    fn counts_the_rows_below_from_its_scroll() {
        let mut mirror = frame(10, 2, &json!({"panes": [pane_json(1, [0, 0, 10, 2], &json!([]))]}));
        mirror.shown.panes[0].scroll = Scroll { total: 50, offset: 10, rows: 2 };

        assert_eq!(RemoteScreen::of(&mirror).panes[0].snapshot.below, 38);
    }

    #[test]
    fn keeps_a_wide_character_and_its_spacer() {
        let screen = one_pane(4, 1, &json!([[{"style": 0, "cells": ["界", "", "a"]}]]));

        let row = &screen.panes[0].snapshot.rows[0];
        assert_eq!((row[0].symbol.as_str(), row[1].spacer, row[2].symbol.as_str()), ("界", true, "a"));
    }
}

mod a_hostile_frame {
    use super::*;

    #[rstest]
    #[case::escape("\u{1b}")]
    #[case::bell("\u{7}")]
    #[case::right_to_left_override("\u{202e}")]
    #[case::zero_width_space("\u{200b}")]
    fn never_draws_a_control_or_hidden_character(#[case] symbol: &str) {
        let screen = one_pane(4, 1, &json!([[{"style": 0, "cells": ["a", symbol, format!("b{symbol}")]}]]));

        assert_eq!(text(&screen.panes[0], 0), "a b ");
    }

    #[test]
    fn has_a_symbol_wider_than_a_wide_character_blanked() {
        let screen = one_pane(4, 1, &json!([[{"style": 0, "cells": ["界界界", "a"]}]]));

        assert_eq!(text(&screen.panes[0], 0), " a  ");
    }

    #[test]
    fn has_a_wide_character_in_the_last_column_blanked() {
        let screen = one_pane(3, 1, &json!([[{"style": 0, "cells": ["a", "b", "界"]}]]));

        assert_eq!(text(&screen.panes[0], 0), "ab ");
        inside(&screen);
    }

    #[test]
    fn keeps_no_pane_or_divider_outside_its_area() {
        let content = json!({
            "dividers": [{"dir": "down", "path": [true], "area": {"x": 50, "y": 50, "cols": 5, "rows": 5},
                          "line": {"x": 50, "y": 52, "cols": 5, "rows": 1}}],
            "panes": [pane_json(1, [0, 0, 10, 4], &json!([])), pane_json(2, [30, 30, 10, 4], &json!([]))]
        });

        let screen = RemoteScreen::of(&frame(10, 4, &content));

        assert_eq!(screen.panes.iter().map(|pane| pane.pane).collect::<Vec<_>>(), [1]);
        assert_eq!(screen.dividers.len(), 0);
    }

    #[test]
    fn with_more_panes_than_fit_keeps_only_the_first_ones() {
        let many: Vec<Value> =
            (1..=300u16).map(|id| pane_json(u64::from(id), [id % 20, id / 20, 1, 1], &json!([]))).collect();
        let lines: Vec<Value> = (0..300)
            .map(|_| {
                json!({"dir": "down", "path": [], "area": {"x": 0, "y": 0, "cols": 10, "rows": 4},
                            "line": {"x": 0, "y": 2, "cols": 10, "rows": 1}})
            })
            .collect();

        let screen = RemoteScreen::of(&frame(20, 20, &json!({"panes": many, "dividers": lines})));

        assert_eq!((screen.panes.len(), screen.panes.last().map(|pane| pane.pane)), (256, Some(256)));
        assert_eq!(screen.dividers.len(), 256);
    }

    #[test]
    fn with_many_panes_as_big_as_its_area_keeps_no_more_cells_than_the_area() {
        let many: Vec<Value> = (1..=50).map(|id| pane_json(id, [0, 0, 40, 10], &json!([]))).collect();

        let screen = RemoteScreen::of(&frame(40, 10, &json!({"panes": many})));

        let cells: usize = screen.panes.iter().flat_map(|pane| &pane.snapshot.rows).map(Vec::len).sum();
        assert_eq!((screen.panes.len(), cells), (1, 400));
    }

    #[test]
    fn with_a_pane_twice_keeps_the_first() {
        let content =
            json!({"panes": [pane_json(1, [0, 0, 5, 4], &json!([])), pane_json(1, [5, 0, 5, 4], &json!([]))]});

        let screen = RemoteScreen::of(&frame(10, 4, &content));

        assert_eq!(screen.panes.iter().map(|pane| pane.rect).collect::<Vec<_>>(), [Rect::new(0, 0, 5, 4)]);
    }

    #[test]
    fn has_a_pane_reaching_past_its_area_cut_to_it() {
        let lines = json!([[{"style": 0, "cells": cells(&"x".repeat(40))}]]);
        let content = json!({"panes": [pane_json(1, [5, 2, 40, 9], &lines)]});

        let screen = RemoteScreen::of(&frame(10, 4, &content));

        assert_eq!(screen.panes[0].rect, Rect::new(5, 2, 5, 2));
        inside(&screen);
    }

    #[test]
    fn of_two_thousand_columns_is_held_to_a_thousand() {
        let lines = json!([[{"style": 0, "cells": cells(&"y".repeat(2000))}]]);
        let content = json!({"panes": [pane_json(1, [0, 0, 2000, 1], &lines)]});

        let screen = RemoteScreen::of(&frame(2000, 600, &content));

        assert_eq!(screen.area, Rect::new(0, 0, 1000, 500));
        assert_eq!(screen.panes[0].size, Size { cols: 1000, rows: 1 });
        inside(&screen);
    }

    #[test]
    fn has_more_rows_than_its_rect_cut_to_it() {
        let lines: Vec<Value> = (0..20).map(|n| json!([{"style": 0, "cells": cells(&n.to_string())}])).collect();

        let screen = one_pane(4, 3, &Value::Array(lines));

        assert_eq!(text(&screen.panes[0], 2), "2   ");
        inside(&screen);
    }

    #[test]
    fn has_a_style_outside_its_table_drawn_plain() {
        let screen = one_pane(2, 1, &json!([[{"style": 99, "cells": ["a"]}]]));

        assert_eq!(screen.panes[0].snapshot.rows[0][0].style, Cell::default().style.fg(Color::Reset).bg(Color::Reset));
    }

    #[test]
    fn keeps_no_hyperlink() {
        let content = json!({
            "hyperlinks": [{"uri": "https://example.com"}],
            "panes": [pane_json(1, [0, 0, 4, 1], &json!([[{"style": 0, "hyperlink": 0, "cells": ["a"]}]]))]
        });

        let screen = RemoteScreen::of(&frame(4, 1, &content));

        assert_eq!(screen.panes[0].snapshot.links, Vec::<String>::new());
        assert_eq!(screen.panes[0].snapshot.rows[0][0].link, None);
    }

    #[test]
    fn has_a_cursor_outside_its_pane_left_out() {
        let mut mirror = frame(4, 1, &json!({"panes": [pane_json(1, [0, 0, 4, 1], &json!([]))]}));
        mirror.shown.panes[0].cursor = Some(surface::Cursor { x: 900, y: 0, ..surface::Cursor::default() });

        assert_eq!(RemoteScreen::of(&mirror).panes[0].snapshot.cursor, None);
    }
}
