use ratatui::layout::{Position, Rect};

use super::{Handled, conflict, json, malformed, none};
use crate::app::App;
use crate::control::contract::{Code, Refusal};
use crate::control::surface::{self, Size};
use crate::control::terminal::{
    End, FileLink, Input, InputEvent, KeyKind, Lines, Link, Linked, Moved, Point, Ratio, Scroll, Scrolled, Selection,
    Sent, Skipped, Why,
};
use crate::control::{Done, Layout};
use crate::files::link;
use crate::log;
use crate::project::Hierarchy;
use crate::term::keys;
use crate::term::mouse::{self, MouseMode};
use crate::term::{self, Term};

fn explicit(pane: Option<u64>, request: &str) -> Result<u64, Refusal> {
    pane.ok_or_else(|| malformed(format!("`{request}` needs the pane to act on: pass --pane")))
}

fn fits(term: &Term, pane: u64, size: Option<Size>) -> Result<(u16, u16), Refusal> {
    let (rows, cols) = term.emulator.size().map_err(|e| Refusal::from(format!("cannot read pane {pane}: {e}")))?;
    if let Some(size) = size.filter(|size| (size.cols, size.rows) != (cols, rows)) {
        return Err(conflict(format!(
            "the request is for a {}×{} terminal, but pane {pane} is {cols}×{rows} now; read its latest frame and try again",
            size.cols, size.rows
        )));
    }
    Ok((cols, rows))
}

fn outside(pane: u64, at: Point, (cols, rows): (u16, u16)) -> Option<Refusal> {
    (at.x >= cols || at.y >= rows)
        .then(|| conflict(format!("({}, {}) is outside pane {pane}'s {cols}×{rows} terminal", at.x, at.y)))
}

fn busy(term: &Term, pane: u64) -> Refusal {
    if term.queue_full() {
        Refusal::new(
            Code::Busy,
            format!(
                "the program in pane {pane} is not reading its input: {} MiB are waiting for it, so nothing was sent",
                term::MAX_QUEUED / (1024 * 1024)
            ),
        )
    } else {
        Refusal::from(format!("cannot write to pane {pane}: its terminal is gone"))
    }
}

fn unsent(kind: KeyKind) -> Why {
    if kind == KeyKind::Release { Why::Release } else { Why::Key }
}

struct Encoded {
    bytes: Vec<u8>,
    skipped: Vec<Skipped>,
    typed: bool,
}

fn encode(term: &mut Term, pane: u64, input: &Input, dims: (u16, u16)) -> Result<Encoded, Refusal> {
    let mut out = Encoded { bytes: Vec::new(), skipped: Vec::new(), typed: false };
    for (event, item) in input.events.iter().enumerate() {
        match item {
            InputEvent::Key(key) => {
                let bytes = term.emulator.encode_key(key.event().map_err(malformed)?);
                out.typed |= !bytes.is_empty();
                if bytes.is_empty() {
                    out.skipped.push(Skipped { event, why: unsent(key.kind) });
                }
                out.bytes.extend(bytes);
            }
            InputEvent::Text(text) => {
                if text.chars().any(char::is_control) {
                    return Err(malformed(
                        "text holds a control character; send Enter, Tab or Esc as keys, and use a paste for lines",
                    ));
                }
                out.typed |= !text.is_empty();
                out.bytes.extend(text.chars().flat_map(|c| term.emulator.encode_key(keys::typed(c))));
            }
            InputEvent::Paste(text) => {
                out.typed |= !text.is_empty();
                out.bytes.extend(term.pasted(text));
            }
            InputEvent::Mouse(at) => {
                if let Some(refusal) = outside(pane, Point { x: at.x, y: at.y }, dims) {
                    return Err(refusal);
                }
                let (mode, encoding) = (term.emulator.mouse_mode(), term.emulator.mouse_encoding());
                match mouse::encode(at.event(), at.x, at.y, mode, encoding) {
                    Some(bytes) => out.bytes.extend(bytes),
                    None => out.skipped.push(Skipped { event, why: Why::Mouse }),
                }
            }
        }
    }
    Ok(out)
}

fn note_typed(term: &mut Term, events: &[InputEvent]) {
    for event in events {
        match event {
            InputEvent::Key(key) => key.event().into_iter().for_each(|key| term.typed_key(key)),
            InputEvent::Text(text) => term.typed_text(text),
            InputEvent::Paste(text) => term.typed_paste(text),
            InputEvent::Mouse(_) => {}
        }
    }
}

impl App {
    fn pane_mut(&mut self, pane: u64) -> Result<&mut Term, Refusal> {
        self.projects.pane_by_mut(pane).ok_or_else(|| none("pane", pane))
    }

    pub(in crate::app) fn input_request(&mut self, input: &Input) -> Handled {
        let pane = explicit(input.pane, "input")?;
        let sending = self.sending(pane);
        let term = self.pane_mut(pane)?;
        let dims = fits(term, pane, input.size)?;
        if input.events.is_empty() {
            return Err(malformed("no events given"));
        }
        let sent = encode(term, pane, input, dims)?;
        if sent.typed && sending {
            return Err(Refusal::new(
                Code::Busy,
                format!(
                    "a message sent with `cornercase send` is between its paste and its Enter in pane {pane}, and \
                     what you type would join it; nothing was sent: send again in a moment"
                ),
            ));
        }
        if !sent.bytes.is_empty() && !term.write(&sent.bytes) {
            return Err(busy(term, pane));
        }
        if sent.typed {
            term.emulator.scroll_to_bottom();
            note_typed(term, &input.events);
        }
        let answer = Sent { pane, bytes: sent.bytes.len(), skipped: sent.skipped };
        Ok(Some(json(&answer)))
    }

    pub(in crate::app) fn scroll_request(&mut self, scroll: &Scroll) -> Handled {
        let pane = explicit(scroll.pane, "scroll")?;
        let (lines, to) = match (scroll.lines, scroll.to) {
            (Some(0), _) => return Err(malformed("`lines` is a number of rows other than 0")),
            (Some(lines), None) => (Some(lines), None),
            (None, Some(end)) => (None, Some(end)),
            _ => return Err(malformed("`scroll` takes either `lines` or `to` (top or bottom), and one of them")),
        };
        let term = self.pane_mut(pane)?;
        if term.emulator.alternate_screen() {
            return Err(conflict(format!(
                "pane {pane} shows its alternate screen, which has no history: its program scrolls itself; send it \
                 keys or the wheel with `cornercase input`"
            )));
        }
        if term.emulator.mouse_mode() != MouseMode::None {
            return Err(conflict(format!(
                "the program in pane {pane} reads the mouse, so its view stays at the bottom; send it the wheel with \
                 `cornercase input`"
            )));
        }
        match (lines, to) {
            (Some(lines), _) => {
                let rows = isize::try_from(lines).unwrap_or(if lines < 0 { isize::MIN } else { isize::MAX });
                term.emulator.scroll(rows);
            }
            (None, Some(End::Top)) => term.emulator.scroll_to_top(),
            (None, _) => term.emulator.scroll_to_bottom(),
        }
        let bar = term.emulator.scrollbar();
        let scroll = surface::Scroll { total: bar.total, offset: bar.offset, rows: bar.rows };
        Ok(Some(json(&Scrolled { pane, scroll })))
    }

    pub(in crate::app) fn selection_request(&mut self, selection: &Selection) -> Handled {
        let pane = explicit(selection.pane, "selection")?;
        let picking = selection.from.is_some() || selection.to.is_some();
        if selection.to.is_some() && selection.from.is_none() {
            return Err(malformed("`to` needs `from`"));
        }
        if selection.from.is_some() && selection.to.is_none() {
            return Err(malformed("`from` needs `to`"));
        }
        if selection.clear && (picking || selection.copy) {
            return Err(malformed("`clear` cannot come with `from`, `to` or `copy`"));
        }
        let term = self.pane_mut(pane)?;
        let dims = fits(term, pane, selection.size)?;
        let failed = |e| Refusal::from(format!("cannot select in pane {pane}: {e}"));
        let text = if let Some((from, to)) = selection.from.zip(selection.to) {
            if let Some(refusal) = outside(pane, from, dims).or_else(|| outside(pane, to, dims)) {
                return Err(refusal);
            }
            let at = |point: Point| Position::new(point.x, point.y);
            term.emulator.select(at(from), at(to), selection.rectangle).map_err(failed)?
        } else if selection.clear {
            term.emulator.clear_selection().map_err(failed)?;
            None
        } else {
            term.emulator.selection_text().map_err(failed)?
        };
        if selection.copy {
            let Some(text) = &text else {
                return Err(conflict(format!("nothing is selected in pane {pane}, so nothing was copied")));
            };
            self.copy(text);
        }
        Ok(Some(json(&Done { text, ..super::pane_ids(pane) })))
    }

    pub(in crate::app) fn link_request(&mut self, at: &Link) -> Handled {
        let pane = explicit(at.pane, "link")?;
        let cell = at.cell.ok_or_else(|| malformed("`link` needs the cell to look at: pass --cell X,Y"))?;
        let (p, w, t) = self.projects.tab_of_pane(pane).ok_or_else(|| none("pane", pane))?;
        let home = self.paths.home.clone();
        let workspace = &mut self.projects[p].workspaces[w];
        let (root, id) = (workspace.path.clone(), workspace.id);
        let term = workspace.tabs[t].pane_by_mut(pane).ok_or_else(|| none("pane", pane))?;
        let dims = fits(term, pane, at.size)?;
        if let Some(refusal) = outside(pane, cell, dims) {
            return Err(refusal);
        }
        let screen = term.emulator.snapshot().map_err(|e| Refusal::from(format!("cannot read pane {pane}: {e}")))?;
        let cwd = term.cwd();
        let found = screen
            .rows
            .get(usize::from(cell.y))
            .and_then(|row| link::under(row, cell.x, cwd.as_deref(), &root, home.as_deref()));
        let link = found.map(|(span, target)| {
            let inside = target.inside();
            FileLink {
                path: target.real.to_string_lossy().into_owned(),
                inside,
                workspace: id,
                relative: inside.then(|| target.path.clone()),
                lines: target.lines.map(|(first, last)| Lines { first, last }),
                row: cell.y,
                start: span.start,
                end: span.end,
            }
        });
        Ok(Some(json(&Linked { pane, link })))
    }

    pub(in crate::app) fn ratio_request(&mut self, ratio: &Ratio, area: Option<Rect>) -> Handled {
        let Some(id) = ratio.tab else {
            return Err(malformed("`ratio` needs the tab whose split to change: pass --tab"));
        };
        #[expect(clippy::cast_possible_truncation, reason = "a ratio between 0 and 1")]
        let share = ratio.ratio as f32;
        if !(share.is_finite() && share > 0.0 && share < 1.0) {
            return Err(malformed(format!(
                "the ratio is the first side's share, between 0 and 1, such as 0.5, not {}",
                ratio.ratio
            )));
        }
        let (p, w, t) = self.projects.tab_index(id).ok_or_else(|| none("tab", id))?;
        let tab = &mut self.projects[p].workspaces[w].tabs[t];
        let moved = tab.layout.set_ratio(&ratio.path, share);
        if !moved {
            return Err(Refusal::new(
                Code::NotFound,
                format!("tab {id} has no divider at that path; the `dividers` of a surface frame list them"),
            ));
        }
        let layout = Layout::from(&tab.layout);
        log::debug!("control", "divider moved", tab = id, depth = ratio.path.len(), ratio = ratio.ratio);
        if let Some(area) = area {
            self.resize(area);
        }
        Ok(Some(json(&Moved { tab: id, layout })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_release_is_skipped_as_a_release() {
        assert_eq!(unsent(KeyKind::Release), Why::Release);
        assert_eq!([unsent(KeyKind::Press), unsent(KeyKind::Repeat)], [Why::Key, Why::Key]);
    }
}
