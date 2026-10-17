use std::collections::{BTreeMap, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};

use serde::{Deserialize, Serialize};

use super::{Colours, Content, Cursor, Hyperlink, Modes, Pane, Run, Scroll, Style, Tables};

const CANDIDATES: usize = 4;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Changes {
    pub(crate) styles: Vec<Style>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) hyperlinks: Vec<Hyperlink>,
    pub(crate) panes: Vec<PaneChanges>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PaneChanges {
    pub(crate) pane: u64,
    pub(crate) cursor: Option<Cursor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) modes: Option<Modes>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) scroll: Option<Scroll>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) colours: Option<Colours>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) copy: Option<Copied>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", deserialize_with = "rows")]
    pub(crate) lines: BTreeMap<u16, Vec<Run>>,
}

fn rows<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<BTreeMap<u16, Vec<Run>>, D::Error> {
    let named: BTreeMap<String, Vec<Run>> = Deserialize::deserialize(deserializer)?;
    named.into_iter().map(|(row, runs)| Ok((row.parse().map_err(serde::de::Error::custom)?, runs))).collect()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Copied {
    pub(crate) from: u16,
    pub(crate) to: u16,
    pub(crate) count: u16,
}

#[derive(Clone, Copy)]
struct Seen<'a> {
    styles: &'a [Style],
    hyperlinks: &'a [Hyperlink],
}

impl<'a> Seen<'a> {
    fn of(content: &'a Content) -> Self {
        Self { styles: &content.styles, hyperlinks: &content.hyperlinks }
    }

    fn style(&self, run: &Run) -> Style {
        self.styles.get(run.style).copied().unwrap_or_default()
    }

    fn uri(&self, run: &Run) -> Option<&'a str> {
        run.hyperlink.and_then(|at| self.hyperlinks.get(at)).map(|link| link.uri.as_str())
    }

    fn key(&self, row: &[Run]) -> u64 {
        let mut hasher = DefaultHasher::new();
        for run in row {
            run.cells.hash(&mut hasher);
            self.style(run).hash(&mut hasher);
            self.uri(run).hash(&mut hasher);
        }
        hasher.finish()
    }
}

fn same(was: Seen, a: &[Run], now: Seen, b: &[Run]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| a.cells == b.cells && was.style(a) == now.style(b) && was.uri(a) == now.uri(b))
}

pub(crate) fn changes(base: &Content, next: &Content) -> Option<Changes> {
    if base.layout != next.layout || base.dividers != next.dividers || base.panes.len() != next.panes.len() {
        return None;
    }
    let (was, now) = (Seen::of(base), Seen::of(next));
    let mut tables = Tables::default();
    let mut panes = Vec::new();
    for (old, new) in base.panes.iter().zip(&next.panes) {
        if old.pane != new.pane || old.rect != new.rect || old.size != new.size || old.lines.len() != new.lines.len() {
            return None;
        }
        if let Some(changed) = pane_changes(old, was, new, now, &mut tables) {
            panes.push(changed);
        }
    }
    Some(Changes { styles: tables.styles.into_entries(), hyperlinks: tables.hyperlinks.into_entries(), panes })
}

fn pane_changes(old: &Pane, was: Seen, new: &Pane, now: Seen, tables: &mut Tables) -> Option<PaneChanges> {
    let copy = copy_of(&old.lines, was, &new.lines, now);
    let mut changed = PaneChanges {
        pane: new.pane,
        cursor: new.cursor,
        active: (old.active != new.active).then_some(new.active),
        modes: (old.modes != new.modes).then_some(new.modes),
        scroll: (old.scroll != new.scroll).then_some(new.scroll),
        colours: (old.colours != new.colours).then(|| new.colours.clone()),
        copy,
        lines: BTreeMap::new(),
    };
    for y in rows_to_send(&old.lines, was, &new.lines, now, copy.as_ref()) {
        let row = new.lines[usize::from(y)].iter().map(|run| reindexed(run, now, tables)).collect();
        changed.lines.insert(y, row);
    }
    let quiet = old.cursor == new.cursor
        && changed.active.is_none()
        && changed.modes.is_none()
        && changed.scroll.is_none()
        && changed.colours.is_none()
        && changed.copy.is_none()
        && changed.lines.is_empty();
    (!quiet).then_some(changed)
}

fn reindexed(run: &Run, now: Seen, tables: &mut Tables) -> Run {
    Run {
        style: tables.styles.of(now.style(run)),
        hyperlink: now.uri(run).map(|uri| tables.hyperlinks.of(Hyperlink { uri: uri.to_string() })),
        cells: run.cells.clone(),
    }
}

fn rows_to_send(old: &[Vec<Run>], was: Seen, new: &[Vec<Run>], now: Seen, copy: Option<&Copied>) -> Vec<u16> {
    let held = |y: usize| -> &[Run] {
        match copy {
            Some(c) if y >= usize::from(c.to) && y < usize::from(c.to) + usize::from(c.count) => {
                &old[y - usize::from(c.to) + usize::from(c.from)]
            }
            _ => &old[y],
        }
    };
    (0..new.len()).filter(|&y| !same(was, held(y), now, &new[y])).filter_map(|y| u16::try_from(y).ok()).collect()
}

fn copy_of(old: &[Vec<Run>], was: Seen, new: &[Vec<Run>], now: Seen) -> Option<Copied> {
    let mut index: HashMap<u64, Vec<usize>> = HashMap::new();
    for (at, row) in old.iter().enumerate().filter(|(_, row)| !row.is_empty()) {
        let seen = index.entry(was.key(row)).or_default();
        if seen.len() < CANDIDATES {
            seen.push(at);
        }
    }
    let mut votes: HashMap<isize, usize> = HashMap::new();
    for (y, row) in new.iter().enumerate().filter(|(_, row)| !row.is_empty()) {
        for &from in index.get(&now.key(row)).into_iter().flatten().filter(|&&from| from != y) {
            let shift = isize::try_from(from).ok()? - isize::try_from(y).ok()?;
            *votes.entry(shift).or_default() += 1;
        }
    }
    let shift = votes
        .into_iter()
        .max_by_key(|&(shift, count)| (count, std::cmp::Reverse(shift.unsigned_abs()), shift > 0))
        .map(|(shift, _)| shift)?;
    let (start, count) = longest_match(old, was, new, now, shift)?;
    let copy = Copied {
        from: u16::try_from(start.checked_add_signed(shift)?).ok()?,
        to: u16::try_from(start).ok()?,
        count: u16::try_from(count).ok()?,
    };
    let with = rows_to_send(old, was, new, now, Some(&copy)).len();
    let without = rows_to_send(old, was, new, now, None).len();
    (with < without).then_some(copy)
}

fn longest_match(old: &[Vec<Run>], was: Seen, new: &[Vec<Run>], now: Seen, shift: isize) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    let mut run: Option<(usize, usize)> = None;
    for (y, row) in new.iter().enumerate() {
        let matches =
            y.checked_add_signed(shift).and_then(|from| old.get(from)).is_some_and(|held| same(was, held, now, row));
        run = match (matches, run) {
            (true, Some((start, len))) => Some((start, len + 1)),
            (true, None) => Some((y, 1)),
            (false, _) => None,
        };
        if let Some(found) = run.filter(|(_, len)| best.is_none_or(|(_, most)| *len > most)) {
            best = Some(found);
        }
    }
    best
}

#[cfg(test)]
mod tests;
