use serde::Deserialize;

use super::patch::{Changes, Copied, PaneChanges};
use super::{Area, Colours, Content, Cursor, Divider, Frame, Modes, Scroll, Size, Style};
use crate::control::Layout;

pub(crate) type Row = Vec<(Vec<String>, Style, Option<String>)>;

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Held {
    pub(crate) pane: u64,
    pub(crate) active: bool,
    pub(crate) rect: Area,
    pub(crate) size: Size,
    pub(crate) cursor: Option<Cursor>,
    pub(crate) modes: Modes,
    pub(crate) scroll: Scroll,
    pub(crate) colours: Colours,
    pub(crate) rows: Vec<Row>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Shown {
    pub(crate) cols: u16,
    pub(crate) rows: u16,
    pub(crate) layout: Option<Layout>,
    pub(crate) dividers: Vec<Divider>,
    pub(crate) panes: Vec<Held>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Mirror {
    pub(crate) revision: u64,
    pub(crate) boot: String,
    pub(crate) surface: u64,
    pub(crate) projection: Option<u64>,
    pub(crate) shown: Shown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unapplied {
    Behind,
    Wrong(String),
}

#[derive(Deserialize)]
#[serde(bound(deserialize = "B: Default + Deserialize<'de>"))]
struct Streamed<B> {
    ok: Frame<B>,
}

#[derive(Deserialize)]
struct Line {
    ok: serde_json::Value,
}

fn resolved(runs: &[super::Run], styles: &[Style], links: &[super::Hyperlink]) -> Row {
    runs.iter()
        .map(|run| {
            let style = styles.get(run.style).copied().unwrap_or_default();
            let uri = run.hyperlink.and_then(|at| links.get(at)).map(|link| link.uri.clone());
            (run.cells.clone(), style, uri)
        })
        .collect()
}

impl Mirror {
    pub(crate) fn of(frame: &Frame) -> Self {
        let Content { layout, dividers, styles, hyperlinks, panes } = &frame.content;
        let panes = panes
            .iter()
            .map(|pane| Held {
                pane: pane.pane,
                active: pane.active,
                rect: pane.rect,
                size: pane.size,
                cursor: pane.cursor,
                modes: pane.modes,
                scroll: pane.scroll,
                colours: pane.colours.clone(),
                rows: pane.lines.iter().map(|row| resolved(row, styles, hyperlinks)).collect(),
            })
            .collect();
        let shown =
            Shown { cols: frame.cols, rows: frame.rows, layout: layout.clone(), dividers: dividers.clone(), panes };
        Self {
            revision: frame.revision,
            boot: frame.boot.clone(),
            surface: frame.surface,
            projection: frame.projection,
            shown,
        }
    }

    pub(crate) fn apply(&mut self, text: &str) -> Result<(), Unapplied> {
        let wrong = |e: serde_json::Error| Unapplied::Wrong(e.to_string());
        let line: Line = serde_json::from_str(text).map_err(wrong)?;
        if line.ok.get("base").is_none() {
            let frame: Streamed<Content> = serde_json::from_str(text).map_err(wrong)?;
            *self = Self::of(&frame.ok);
            return Ok(());
        }
        let patch: Frame<Changes> = serde_json::from_str::<Streamed<Changes>>(text).map_err(wrong)?.ok;
        if patch.base != Some(self.revision) {
            return Err(Unapplied::Behind);
        }
        let Changes { styles, hyperlinks, panes } = patch.content;
        self.check(&panes).map_err(Unapplied::Wrong)?;
        for changes in panes {
            if let Some(held) = self.shown.panes.iter_mut().find(|held| held.pane == changes.pane) {
                apply_pane(held, changes, &styles, &hyperlinks);
            }
        }
        (self.revision, self.boot, self.surface, self.projection) =
            (patch.revision, patch.boot, patch.surface, patch.projection);
        Ok(())
    }

    fn check(&self, panes: &[PaneChanges]) -> Result<(), String> {
        for changes in panes {
            let held = self
                .shown
                .panes
                .iter()
                .find(|held| held.pane == changes.pane)
                .ok_or_else(|| format!("a patch names pane {}, which is not shown", changes.pane))?;
            let len = held.rows.len();
            if changes.copy.is_some_and(|copy| !fits(copy, len)) {
                return Err("a copy reaches past the pane's rows".to_string());
            }
            if let Some(y) = changes.lines.keys().copied().find(|y| usize::from(*y) >= len) {
                return Err(format!("row {y} is not in the pane"));
            }
        }
        Ok(())
    }
}

fn fits(copy: Copied, len: usize) -> bool {
    let end = |start: u16| usize::from(start).checked_add(usize::from(copy.count)).is_some_and(|end| end <= len);
    end(copy.from) && end(copy.to)
}

fn apply_pane(held: &mut Held, changes: PaneChanges, styles: &[Style], links: &[super::Hyperlink]) {
    held.cursor = changes.cursor;
    if let Some(active) = changes.active {
        held.active = active;
    }
    if let Some(modes) = changes.modes {
        held.modes = modes;
    }
    if let Some(scroll) = changes.scroll {
        held.scroll = scroll;
    }
    if let Some(colours) = changes.colours {
        held.colours = colours;
    }
    if let Some(copy) = changes.copy {
        let (from, to, count) = (usize::from(copy.from), usize::from(copy.to), usize::from(copy.count));
        let moved = held.rows[from..from + count].to_vec();
        held.rows[to..to + count].clone_from_slice(&moved);
    }
    for (y, runs) in changes.lines {
        if let Some(row) = held.rows.get_mut(usize::from(y)) {
            *row = resolved(&runs, styles, links);
        }
    }
}

#[cfg(test)]
mod tests;
