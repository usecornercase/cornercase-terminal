use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};

use super::geometry::{At, Cell, E, Geometry, N, Placed, S, W};
use super::{Diagram, Hit, Node, State};
use crate::agents::activity::Status;
use crate::flows::progress::Edge;
use crate::text;
use crate::ui::theme::{BRAND_COLOR, WAITING_COLOR, hover_colour, line_colour, status_icon};

struct Canvas<'a> {
    buf: &'a mut Buffer,
    area: Rect,
    scroll: u16,
}

impl Canvas<'_> {
    fn spot(&self, (x, y): At) -> Option<(u16, u16)> {
        let y = y.checked_sub(self.scroll)?;
        (x < self.area.width && y < self.area.height).then(|| (self.area.x + x, self.area.y + y))
    }

    fn put(&mut self, at: At, text: &str, style: Style) {
        let mut x = at.0;
        for ch in text.chars() {
            if let Some((sx, sy)) = self.spot((x, at.1)) {
                self.buf[(sx, sy)].set_char(ch).set_style(style);
            }
            x = x.saturating_add(1);
        }
    }

    fn fill(&mut self, rect: Rect, style: Style) {
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                if let Some(spot) = self.spot((x, y)) {
                    self.buf[spot].set_style(style);
                }
            }
        }
    }
}

fn glyph(dirs: u8, dashed: bool) -> &'static str {
    const NS: u8 = N | S;
    const EW: u8 = E | W;
    match dirs {
        N | S | NS if dashed => "┆",
        E | W | EW if dashed => "┄",
        N | S | NS => "│",
        E | W | EW => "─",
        0b0110 => "╭",
        0b1010 => "╮",
        0b0101 => "╰",
        0b1001 => "╯",
        0b0111 => "├",
        0b1011 => "┤",
        0b1110 => "┬",
        0b1101 => "┴",
        0b1111 => "┼",
        _ => " ",
    }
}

fn hovered_edge(d: &Diagram, g: &Geometry, at: Option<At>) -> Option<Edge> {
    let at = at?;
    let clickable = d.live.as_ref().map(|live| live.messages.as_slice()).unwrap_or_default();
    let on_cell = g.cells.get(&at).into_iter().flat_map(|cell| cell.edges.iter().copied());
    let on_label = g
        .labels
        .iter()
        .filter(|l| l.at.1 == at.1 && (l.at.0..l.at.0 + text_width(&l.text)).contains(&at.0))
        .map(|l| Edge::Loop(l.stage));
    on_cell.chain(on_label).find(|edge| clickable.contains(edge))
}

fn text_width(text: &str) -> u16 {
    u16::try_from(text.chars().count()).unwrap_or(u16::MAX)
}

fn local(area: Rect, scroll: u16, pos: Position) -> Option<At> {
    area.contains(pos).then(|| (pos.x - area.x, pos.y - area.y + scroll))
}

pub(super) fn hit(area: Rect, d: &Diagram, g: &Geometry, scroll: u16, pos: Position) -> Option<Hit> {
    let at = local(area, scroll, pos)?;
    let inside = |b: &&Placed| b.rect.contains(Position::new(at.0, at.1));
    if let Some(node) = g.boxes.iter().find(inside).and_then(|b| d.node(b.place)) {
        return Some(Hit::Role(node.role.clone()));
    }
    hovered_edge(d, g, Some(at)).map(Hit::Edge)
}

pub(super) fn paint(buf: &mut Buffer, area: Rect, d: &Diagram, g: &Geometry, scroll: u16, hover: Option<Position>) {
    let at = hover.and_then(|pos| local(area, scroll, pos));
    let lit = d.live.as_ref().and_then(|live| live.lit.as_ref());
    let hovered = hovered_edge(d, g, at);
    let line = line_colour(d.light);
    let mut frame = Canvas { buf, area, scroll };
    let colour = |edges: &[Edge]| {
        if hovered.is_some_and(|h| edges.contains(&h)) {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else if let Some(lit) = lit.filter(|lit| edges.contains(&lit.edge)) {
            Style::default().fg(lit.colour)
        } else {
            Style::default().fg(line)
        }
    };
    for (&spot, cell) in &g.cells {
        let Cell { dirs, arrow, edges, dashed } = cell;
        frame.put(spot, arrow.unwrap_or_else(|| glyph(*dirs, *dashed)), colour(edges));
    }
    for label in &g.labels {
        let back = d.loops.get(label.stage).and_then(Option::as_ref);
        let over = back.is_some_and(|b| d.live.is_some() && b.rounds > b.at_most);
        let edge = [Edge::Loop(label.stage)];
        let style = if over { Style::default().fg(WAITING_COLOR) } else { colour(&edge) };
        let style = if style.fg == Some(line) { Style::default().fg(d.muted) } else { style };
        frame.put(label.at, &label.text, style);
    }
    for placed in &g.boxes {
        if let Some(node) = d.node(placed.place) {
            let lit = at.is_some_and(|(x, y)| placed.rect.contains(Position::new(x, y)));
            draw_box(&mut frame, d, node, placed, lit);
        }
    }
    if let Some(end) = g.end {
        let ended = d.live.as_ref().is_some_and(|live| live.ended);
        let (mark, words) = if ended {
            (Style::default().fg(BRAND_COLOR).add_modifier(Modifier::BOLD), Style::default())
        } else {
            (Style::default().fg(d.muted), Style::default().fg(d.muted))
        };
        frame.put(end, "◆", mark);
        let at = g.below.unwrap_or((end.0 + 2, end.1));
        let room = usize::from(area.width.saturating_sub(at.0));
        frame.put(at, &text::truncate_right(d.end, room), words);
    }
    if let Some(lit) = lit
        && let Some(done) = lit.dot
    {
        let path = g.path(lit.edge, lit.from, lit.to);
        let count = path.len();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "a fraction of a path a few dozen cells long"
        )]
        let i = ((done * count as f64) as usize).min(count.saturating_sub(1));
        if let Some(&spot) = path.get(i) {
            frame.put(spot, "●", Style::default().fg(lit.colour).add_modifier(Modifier::BOLD));
        }
    }
}

struct Borders {
    corners: [&'static str; 4],
    across: &'static str,
    side: &'static str,
    exit: &'static str,
}

const LIGHT: Borders = Borders { corners: ["╭", "╮", "╰", "╯"], across: "─", side: "│", exit: "┬" };

const HEAVY: Borders = Borders { corners: ["┏", "┓", "┗", "┛"], across: "━", side: "┃", exit: "┰" };

fn draw_box(frame: &mut Canvas, d: &Diagram, node: &Node, placed: &Placed, lit: bool) {
    let r = placed.rect;
    let working = node.state == Some(State::Agent(Some(Status::Working)));
    let closed = node.state == Some(State::Closed);
    let borders = if working { &HEAVY } else { &LIGHT };
    let bg = if lit { Style::default().bg(hover_colour(d.light)) } else { Style::default() };
    let border = bg.fg(if working { node.colour } else { line_colour(d.light) });
    frame.fill(r, bg);
    let inner = usize::from(r.width.saturating_sub(2));
    let (top, bottom) = (r.y, r.y + r.height - 1);
    let [tl, tr, bl, br] = borders.corners;
    frame.put((r.x, top), &format!("{tl}{}{tr}", borders.across.repeat(inner)), border);
    frame.put((r.x, bottom), &format!("{bl}{}{br}", borders.across.repeat(inner)), border);
    for y in top + 1..bottom {
        frame.put((r.x, y), borders.side, border);
        frame.put((r.right() - 1, y), borders.side, border);
    }
    if let Some(x) = placed.exit {
        frame.put((x, bottom), borders.exit, border);
    }
    let name = text::truncate_right(&node.role, usize::from(r.width.saturating_sub(6)));
    let name_style = if closed { bg.fg(d.muted) } else { bg.fg(node.colour).add_modifier(Modifier::BOLD) };
    if r.width >= 6 {
        frame.put((r.x + 2, top), &format!(" {name} "), name_style);
    }
    let icon = match node.state {
        Some(State::Agent(Some(status))) => Some(status_icon(d.muted, status, d.spin)),
        Some(State::Agent(None)) => Some(status_icon(d.muted, Status::Idle, d.spin)),
        Some(State::Closed) | None => None,
    };
    let reserve = if icon.is_some() { 6 } else { 4 };
    let words = if closed { "closed" } else { node.text.as_str() };
    let words = text::truncate_right(words, usize::from(r.width.saturating_sub(reserve)));
    frame.put((r.x + 2, top + 1), &words, bg.fg(d.muted));
    if let Some(icon) = icon.filter(|_| r.width >= 5) {
        frame.put((r.right() - 3, top + 1), &icon.content, bg.patch(icon.style));
    }
}
