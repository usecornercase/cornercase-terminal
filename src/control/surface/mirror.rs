use serde::Deserialize;

use super::patch::{Changes, PaneChanges};
use super::{Colours, Content, Cursor, Frame, Modes, Scroll, Style};
use crate::control::Layout;

pub(crate) type Row = Vec<(Vec<String>, Style, Option<String>)>;

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Held {
    pub(crate) pane: u64,
    pub(crate) active: bool,
    pub(crate) cursor: Option<Cursor>,
    pub(crate) modes: Modes,
    pub(crate) scroll: Scroll,
    pub(crate) colours: Colours,
    pub(crate) rows: Vec<Row>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Shown {
    pub(crate) layout: Option<Layout>,
    pub(crate) panes: Vec<Held>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Mirror {
    pub(crate) revision: u64,
    pub(crate) shown: Shown,
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
        let Content { layout, styles, hyperlinks, panes, .. } = &frame.content;
        let panes = panes
            .iter()
            .map(|pane| Held {
                pane: pane.pane,
                active: pane.active,
                cursor: pane.cursor,
                modes: pane.modes,
                scroll: pane.scroll,
                colours: pane.colours.clone(),
                rows: pane.lines.iter().map(|row| resolved(row, styles, hyperlinks)).collect(),
            })
            .collect();
        Self { revision: frame.revision, shown: Shown { layout: layout.clone(), panes } }
    }

    pub(crate) fn apply(&mut self, text: &str) -> Result<(), String> {
        let line: Line = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if line.ok.get("base").is_none() {
            let frame: Streamed<Content> = serde_json::from_str(text).map_err(|e| e.to_string())?;
            *self = Self::of(&frame.ok);
            return Ok(());
        }
        let patch: Frame<Changes> = serde_json::from_str::<Streamed<Changes>>(text).map_err(|e| e.to_string())?.ok;
        let base = patch.base.unwrap_or_default();
        if base != self.revision {
            return Err(format!("a patch for revision {base} met revision {}", self.revision));
        }
        let Changes { styles, hyperlinks, panes } = patch.content;
        for changes in panes {
            let held = self
                .shown
                .panes
                .iter_mut()
                .find(|held| held.pane == changes.pane)
                .ok_or_else(|| format!("a patch names pane {}, which is not shown", changes.pane))?;
            apply_pane(held, changes, &styles, &hyperlinks)?;
        }
        self.revision = patch.revision;
        Ok(())
    }
}

fn apply_pane(
    held: &mut Held,
    changes: PaneChanges,
    styles: &[Style],
    links: &[super::Hyperlink],
) -> Result<(), String> {
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
        let before = held.rows.clone();
        let (from, to, count) = (usize::from(copy.from), usize::from(copy.to), usize::from(copy.count));
        if from + count > before.len() || to + count > held.rows.len() {
            return Err("a copy reaches past the pane's rows".to_string());
        }
        held.rows[to..to + count].clone_from_slice(&before[from..from + count]);
    }
    for (y, runs) in changes.lines {
        let row = held.rows.get_mut(usize::from(y)).ok_or_else(|| format!("row {y} is not in the pane"))?;
        *row = resolved(&runs, styles, links);
    }
    Ok(())
}
