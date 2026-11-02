use serde_json::{Value, json};

use super::screen::RemoteScreen;
use crate::control::{Report, Snapshot, ViewInfo};

pub(crate) const SURFACE: u64 = 3;

#[derive(Debug, Clone, Copy)]
pub(crate) struct There {
    pub(crate) boot: &'static str,
    pub(crate) view: u64,
    pub(crate) tab: u64,
}

impl There {
    pub(crate) fn view_line(self, projection: u64) -> Value {
        let view = ViewInfo { id: self.view, projection, active: true, ..ViewInfo::default() };
        let report = Report { boot: self.boot.into(), view: Some(view), ..Report::default() };
        json!({"ok": Snapshot { revision: 1, report }})
    }

    pub(crate) fn complete(self, revision: u64, projection: u64, text: &str) -> Value {
        let pane = json!({
            "pane": 1, "active": true, "rect": {"x": 0, "y": 0, "cols": 20, "rows": 2},
            "size": {"cols": 20, "rows": 2}, "lines": [row(text), []]
        });
        json!({"ok": {
            "boot": self.boot, "revision": revision, "surface": SURFACE, "tab": self.tab, "cols": 20, "rows": 2,
            "view": self.view, "projection": projection, "layout": {"leaf": 1}, "styles": [{}], "panes": [pane]
        }})
    }

    pub(crate) fn split(self, revision: u64, projection: u64, mouse: &str) -> Value {
        let pane = |id: u64, x: u16| {
            json!({
                "pane": id, "active": id == 1, "rect": {"x": x, "y": 0, "cols": 9, "rows": 2},
                "size": {"cols": 9, "rows": 2}, "modes": {"mouse": mouse}, "lines": [row(&format!("pane {id}")), []]
            })
        };
        let layout = json!({"split": {"dir": "right", "ratio": 0.5, "first": {"leaf": 1}, "second": {"leaf": 2}}});
        let divider = json!({"dir": "right", "path": [], "area": {"x": 0, "y": 0, "cols": 20, "rows": 2},
            "line": {"x": 9, "y": 0, "cols": 1, "rows": 2}});
        json!({"ok": {
            "boot": self.boot, "revision": revision, "surface": SURFACE, "tab": self.tab, "cols": 20, "rows": 2,
            "view": self.view, "projection": projection, "layout": layout, "dividers": [divider], "styles": [{}],
            "panes": [pane(1, 0), pane(2, 11)]
        }})
    }

    pub(crate) fn patch(self, revision: u64, base: u64, text: &str) -> Value {
        json!({"ok": {
            "boot": self.boot, "revision": revision, "base": base, "surface": SURFACE, "tab": self.tab,
            "cols": 20, "rows": 2, "view": self.view, "projection": 1, "styles": [{}],
            "panes": [{"pane": 1, "lines": {"0": row(text)}}]
        }})
    }
}

fn row(text: &str) -> Value {
    json!([{"style": 0, "cells": text.chars().map(|c| c.to_string()).collect::<Vec<_>>()}])
}

pub(crate) fn text(screen: &RemoteScreen) -> String {
    let cells = screen.panes[0].snapshot.rows[0].iter().map(|cell| cell.symbol.as_str());
    cells.collect::<String>().trim_end().to_string()
}
