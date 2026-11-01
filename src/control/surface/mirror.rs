use std::collections::HashMap;
use std::sync::Arc;

use serde::Deserialize;

use super::patch::{Changes, Copied, PaneChanges};
use super::{Area, Colours, Content, Cursor, Divider, Frame, Hyperlink, Modes, Run, Scroll, Size, Style};
use crate::control::Layout;

pub(crate) const MOST_HELD: usize = 64 * 1024 * 1024;
const TOO_MUCH: &str = "the screen holds more than a terminal shows";

pub(crate) type Row = Vec<(Vec<String>, Style, Option<Arc<str>>)>;

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
    weights: Vec<usize>,
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
    pub(crate) tab: u64,
    pub(crate) projection: Option<u64>,
    pub(crate) shown: Shown,
    held: usize,
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

fn resolved(runs: &[Run], styles: &[Style], links: &[Arc<str>]) -> Row {
    runs.iter()
        .map(|run| {
            let style = styles.get(run.style).copied().unwrap_or_default();
            let uri = run.hyperlink.and_then(|at| links.get(at)).map(Arc::clone);
            (run.cells.clone(), style, uri)
        })
        .collect()
}

fn shared(links: &[Hyperlink]) -> Vec<Arc<str>> {
    links.iter().map(|link| Arc::from(link.uri.as_str())).collect()
}

fn cells_weight(cells: &[String]) -> usize {
    cells.iter().map(|cell| size_of::<String>() + cell.len()).sum()
}

fn weight(row: &Row) -> usize {
    row.iter().map(|(cells, _, uri)| cells_weight(cells) + uri.as_ref().map_or(0, |uri| uri.len())).sum()
}

fn runs_weight(runs: &[Run], links: &[Hyperlink]) -> usize {
    let uri = |run: &Run| run.hyperlink.and_then(|at| links.get(at)).map_or(0, |link| link.uri.len());
    runs.iter().map(|run| cells_weight(&run.cells) + uri(run)).sum()
}

fn reweighed(rows: &mut [usize], changes: &PaneChanges, links: &[Hyperlink]) {
    if let Some(copy) = changes.copy {
        let (from, to, count) = (usize::from(copy.from), usize::from(copy.to), usize::from(copy.count));
        rows.copy_within(from..from + count, to);
    }
    for (y, runs) in &changes.lines {
        if let Some(row) = rows.get_mut(usize::from(*y)) {
            *row = runs_weight(runs, links);
        }
    }
}

impl Mirror {
    pub(crate) fn of(frame: &Frame) -> Self {
        let Content { layout, dividers, styles, hyperlinks, panes } = &frame.content;
        let links = shared(hyperlinks);
        let panes: Vec<Held> = panes
            .iter()
            .map(|pane| {
                let rows: Vec<Row> = pane.lines.iter().map(|row| resolved(row, styles, &links)).collect();
                Held {
                    pane: pane.pane,
                    active: pane.active,
                    rect: pane.rect,
                    size: pane.size,
                    cursor: pane.cursor,
                    modes: pane.modes,
                    scroll: pane.scroll,
                    colours: pane.colours.clone(),
                    weights: rows.iter().map(weight).collect(),
                    rows,
                }
            })
            .collect();
        let held = panes.iter().flat_map(|held| held.weights.iter()).sum();
        let shown =
            Shown { cols: frame.cols, rows: frame.rows, layout: layout.clone(), dividers: dividers.clone(), panes };
        Self {
            revision: frame.revision,
            boot: frame.boot.clone(),
            surface: frame.surface,
            tab: frame.tab,
            projection: frame.projection,
            shown,
            held,
        }
    }

    pub(crate) fn apply(&mut self, text: &str) -> Result<(), Unapplied> {
        let wrong = |e: serde_json::Error| Unapplied::Wrong(e.to_string());
        let line: Line = serde_json::from_str(text).map_err(wrong)?;
        if line.ok.get("base").is_none() {
            let frame: Streamed<Content> = serde_json::from_str(text).map_err(wrong)?;
            let mirror = Self::of(&frame.ok);
            if mirror.held > MOST_HELD {
                return Err(Unapplied::Wrong(TOO_MUCH.into()));
            }
            *self = mirror;
            return Ok(());
        }
        let patch: Frame<Changes> = serde_json::from_str::<Streamed<Changes>>(text).map_err(wrong)?.ok;
        if patch.base != Some(self.revision) {
            return Err(Unapplied::Behind);
        }
        let Changes { styles, hyperlinks, panes } = patch.content;
        self.check(&panes).map_err(Unapplied::Wrong)?;
        let (held, mut weights) = self
            .weighed(&panes, &hyperlinks)
            .filter(|(held, _)| *held <= MOST_HELD)
            .ok_or_else(|| Unapplied::Wrong(TOO_MUCH.into()))?;
        self.held = held;
        let links = shared(&hyperlinks);
        for changes in panes {
            if let Some(held) = self.shown.panes.iter_mut().find(|held| held.pane == changes.pane) {
                apply_pane(held, changes, &styles, &links);
            }
        }
        for held in &mut self.shown.panes {
            if let Some(rows) = weights.remove(&held.pane) {
                held.weights = rows;
            }
        }
        (self.revision, self.boot, self.surface, self.tab, self.projection) =
            (patch.revision, patch.boot, patch.surface, patch.tab, patch.projection);
        Ok(())
    }

    fn weighed(&self, panes: &[PaneChanges], links: &[Hyperlink]) -> Option<(usize, HashMap<u64, Vec<usize>>)> {
        let mut touched: HashMap<u64, (usize, Vec<usize>)> = HashMap::new();
        for changes in panes {
            let Some(held) = self.shown.panes.iter().find(|held| held.pane == changes.pane) else { continue };
            let (_, rows) =
                touched.entry(changes.pane).or_insert_with(|| (held.weights.iter().sum(), held.weights.clone()));
            reweighed(rows, changes, links);
        }
        let held = touched
            .values()
            .try_fold(self.held, |held, (before, rows)| held.checked_sub(*before)?.checked_add(rows.iter().sum()))?;
        Some((held, touched.into_iter().map(|(pane, (_, rows))| (pane, rows)).collect()))
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

fn apply_pane(held: &mut Held, changes: PaneChanges, styles: &[Style], links: &[Arc<str>]) {
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
