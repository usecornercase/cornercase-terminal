use rstest::rstest;
use serde_json::{Value, json};

use super::*;

fn complete(revision: u64) -> Value {
    json!({"ok": {
        "revision": revision, "cols": 20, "rows": 3, "layout": {"leaf": 1},
        "styles": [{}, {"bold": true}],
        "panes": [{
            "pane": 1, "active": true,
            "rect": {"x": 0, "y": 0, "cols": 20, "rows": 3}, "size": {"cols": 20, "rows": 3},
            "lines": [[{"style": 0, "cells": ["a"]}], [{"style": 1, "cells": ["b"]}], []]
        }]
    }})
}

fn held() -> Mirror {
    let mut mirror = Mirror::default();
    mirror.apply(&complete(1).to_string()).expect("a complete frame");
    mirror
}

fn patch(panes: &Value) -> Value {
    json!({"ok": {"revision": 2, "base": 1, "styles": [{}], "panes": panes}})
}

fn row(text: &str) -> Value {
    json!([{"style": 0, "cells": text.chars().map(|c| c.to_string()).collect::<Vec<_>>()}])
}

fn text(mirror: &Mirror, y: usize) -> String {
    mirror.shown.panes[0].rows[y].iter().flat_map(|(cells, ..)| cells.iter().map(String::as_str)).collect()
}

mod a_hostile_line {
    use super::*;

    #[rstest]
    #[case::broken_json("{\"ok\": {".to_string())]
    #[case::an_error_line(json!({"error": {"code": "busy"}}).to_string())]
    #[case::ok_is_not_an_object(json!({"ok": 7}).to_string())]
    #[case::a_copy_past_the_rows(patch(&json!([{"pane": 1, "copy": {"from": 65535, "to": 0, "count": 65535}}])).to_string())]
    #[case::a_copy_onto_rows_past_the_end(patch(&json!([{"pane": 1, "copy": {"from": 0, "to": 2, "count": 2}}])).to_string())]
    #[case::row_65535(patch(&json!([{"pane": 1, "lines": {"65535": row("x")}}])).to_string())]
    #[case::a_row_past_u16(patch(&json!([{"pane": 1, "lines": {"65536": row("x")}}])).to_string())]
    #[case::a_row_that_is_not_a_number(patch(&json!([{"pane": 1, "lines": {"top": row("x")}}])).to_string())]
    #[case::an_unknown_pane(patch(&json!([{"pane": 9, "lines": {"0": row("x")}}])).to_string())]
    #[case::an_old_base(json!({"ok": {"revision": 9, "base": 0, "panes": []}}).to_string())]
    fn is_refused_and_leaves_the_screen_as_it_was(#[case] line: String) {
        let mut mirror = held();
        let before = mirror.shown.clone();

        assert!(mirror.apply(&line).is_err());

        assert_eq!((mirror.revision, &mirror.shown), (1, &before));
    }

    #[test]
    fn a_patch_with_one_bad_pane_changes_no_pane() {
        let mut mirror = held();
        let panes = json!([{"pane": 1, "lines": {"0": row("x")}}, {"pane": 1, "lines": {"3": row("y")}}]);

        assert!(mirror.apply(&patch(&panes).to_string()).is_err());

        assert_eq!(text(&mirror, 0), "a");
    }

    #[test]
    fn a_patch_for_another_revision_is_behind() {
        let mut mirror = held();

        let applied = mirror.apply(&json!({"ok": {"revision": 9, "base": 8, "panes": []}}).to_string());

        assert_eq!(applied, Err(Unapplied::Behind));
    }

    #[rstest]
    #[case::broken_json("{\"ok\": {".to_string())]
    #[case::an_error_line(json!({"error": {"code": "busy"}}).to_string())]
    #[case::an_unknown_pane(patch(&json!([{"pane": 9, "lines": {"0": row("x")}}])).to_string())]
    fn that_cannot_be_read_is_wrong(#[case] line: String) {
        let applied = held().apply(&line);

        assert!(matches!(applied, Err(Unapplied::Wrong(_))), "{applied:?}");
    }
}

mod a_good_line {
    use super::*;

    #[test]
    fn a_patch_copies_rows_and_writes_lines() {
        let mut mirror = held();
        let panes = json!([{"pane": 1, "copy": {"from": 1, "to": 0, "count": 2}, "lines": {"2": row("new")}}]);

        mirror.apply(&patch(&panes).to_string()).expect("a patch");

        assert_eq!(
            (mirror.revision, text(&mirror, 0), text(&mirror, 1), text(&mirror, 2)),
            (2, "b".into(), String::new(), "new".into())
        );
    }

    #[test]
    fn a_complete_frame_replaces_everything_held() {
        let mut mirror = held();
        let other = json!({"ok": {"revision": 7, "cols": 5, "rows": 1, "layout": {"leaf": 4}, "panes": [{"pane": 4, "lines": [[]]}]}});

        mirror.apply(&other.to_string()).expect("a complete frame");

        assert_eq!(mirror.revision, 7);
        assert_eq!((mirror.shown.cols, mirror.shown.rows), (5, 1));
        assert_eq!(mirror.shown.panes.iter().map(|held| held.pane).collect::<Vec<_>>(), [4]);
    }

    #[test]
    fn a_complete_frame_keeps_each_pane_s_rect_and_size() {
        let held = &held().shown.panes[0];

        assert_eq!((held.rect, held.size), (Area { x: 0, y: 0, cols: 20, rows: 3 }, Size { cols: 20, rows: 3 }));
    }

    #[test]
    fn each_line_keeps_the_frame_s_boot_surface_and_projection() {
        let mut mirror = held();
        let header = json!({"boot": "b-1", "surface": 3, "projection": 2, "revision": 2, "base": 1, "panes": []});

        mirror.apply(&json!({"ok": header}).to_string()).expect("a patch");

        assert_eq!((mirror.boot.as_str(), mirror.surface, mirror.projection), ("b-1", 3, Some(2)));
    }
}
