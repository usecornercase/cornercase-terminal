use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

use super::geometry::{At, Cell, E, Geometry, N, Place, Placed, S, W};
use super::{Diagram, Hit, Lit, Node, State, TAG_GAP};
use crate::agents::activity::Status;
use crate::flows::progress::Edge;
use crate::text;
use crate::ui::theme::{BRAND_COLOR, WAITING_COLOR, hover_colour, line_colour, status_icon};

pub(super) struct Canvas<'a> {
    buf: &'a mut Buffer,
    area: Rect,
    scroll: u16,
}

impl<'a> Canvas<'a> {
    pub(super) fn new(buf: &'a mut Buffer, area: Rect, scroll: u16) -> Self {
        Self { buf, area, scroll }
    }
}

impl Canvas<'_> {
    fn spot(&self, (x, y): At) -> Option<(u16, u16)> {
        let y = y.checked_sub(self.scroll)?;
        (x < self.area.width && y < self.area.height).then(|| (self.area.x + x, self.area.y + y))
    }

    fn put(&mut self, at: At, text: &str, style: Style) {
        if let Some((x, y)) = self.spot(at) {
            self.buf.set_stringn(x, y, text, usize::from(self.area.right() - x), style);
        }
    }

    fn fill(&mut self, rect: Rect, style: Style) {
        let at = (rect.y..rect.bottom()).flat_map(|y| (rect.x..rect.right()).map(move |x| (x, y)));
        let spots: Vec<(u16, u16)> = at.filter_map(|at| self.spot(at)).collect();
        for spot in spots {
            self.buf[spot].set_style(style);
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

type Arrow = (Edge, Option<usize>);

fn branch(g: &Geometry, edge: Edge, at: At) -> Option<usize> {
    let Edge::Forward(stage) = edge else { return None };
    let entries = &g.gaps.get(stage)?.entries;
    let mut on = entries.iter().enumerate().filter(|(_, path)| path.contains(&at)).map(|(j, _)| j);
    match (on.next(), on.next()) {
        (Some(j), None) => Some(j),
        _ => None,
    }
}

fn hovered_edge(d: &Diagram, g: &Geometry, at: Option<At>) -> Option<Arrow> {
    let at = at?;
    let on_cell = g.cells.get(&at).into_iter().flat_map(|cell| cell.edges.iter().map(|&e| (e, branch(g, e, at))));
    let on_label = g
        .labels
        .iter()
        .filter(|l| l.at.1 == at.1 && (l.at.0..l.at.0.saturating_add(text_width(&l.text))).contains(&at.0))
        .map(|l| (Edge::Loop(l.stage), None));
    on_cell.chain(on_label).find(|&(edge, j)| d.sent(edge, j.and_then(|j| d.branch_role(edge, j))))
}

fn arrow_cells(g: &Geometry, (edge, branch): Arrow) -> Option<Vec<At>> {
    let (Edge::Forward(stage), Some(j)) = (edge, branch) else { return None };
    let gap = g.gaps.get(stage)?;
    let trunk = gap.merges.iter().flatten().chain(&gap.middle);
    Some(trunk.chain(gap.entries.get(j).into_iter().flatten()).copied().collect())
}

fn tag_cells(d: &Diagram, g: &Geometry) -> Option<(At, Span<'static>)> {
    Some((g.end_tag?, d.tag()?))
}

fn text_width(text: &str) -> u16 {
    u16::try_from(text::cells(text)).unwrap_or(u16::MAX)
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
    if let Some(((x, y), tag)) = tag_cells(d, g)
        && at.1 == y
        && (x..x.saturating_add(text_width(&tag.content))).contains(&at.0)
    {
        return Some(Hit::Pull);
    }
    let (edge, branch) = hovered_edge(d, g, Some(at))?;
    Some(Hit::Edge(edge, branch.and_then(|j| d.branch_role(edge, j)).map(str::to_string)))
}

struct Tint<'a> {
    hovered: Option<Arrow>,
    cells: Option<Vec<At>>,
    lit: Option<&'a Lit>,
    line: Color,
}

impl Tint<'_> {
    fn of(&self, spot: Option<At>, edges: &[Edge]) -> Style {
        let on = |spot: At| self.cells.as_ref().is_none_or(|cells| cells.contains(&spot));
        if self.hovered.is_some_and(|(h, _)| edges.contains(&h) && spot.is_none_or(on)) {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else if let Some(lit) = self.lit.filter(|lit| edges.contains(&lit.edge)) {
            Style::default().fg(lit.colour)
        } else {
            Style::default().fg(self.line)
        }
    }
}

pub(super) fn paint(frame: &mut Canvas, d: &Diagram, g: &Geometry, hover: Option<Position>) {
    let at = hover.and_then(|pos| local(frame.area, frame.scroll, pos));
    let hovered = hovered_edge(d, g, at);
    let lit = d.live.as_ref().and_then(|live| live.lit.as_ref());
    let line = line_colour(d.light);
    let tint = Tint { hovered, cells: hovered.and_then(|arrow| arrow_cells(g, arrow)), lit, line };
    for (&spot, cell) in &g.cells {
        let Cell { dirs, arrow, edges, dashed } = cell;
        frame.put(spot, arrow.unwrap_or_else(|| glyph(*dirs, *dashed)), tint.of(Some(spot), edges));
    }
    paint_labels(frame, d, g, &tint);
    for placed in &g.boxes {
        if let Some(node) = d.node(placed.place) {
            let lit = at.is_some_and(|(x, y)| placed.rect.contains(Position::new(x, y)));
            draw_box(frame, d, node, placed, lit);
        }
    }
    draw_end(frame, d, g, at);
    if let Some(lit) = lit {
        draw_dot(frame, g, lit);
    }
}

fn paint_labels(frame: &mut Canvas, d: &Diagram, g: &Geometry, tint: &Tint) {
    for label in &g.labels {
        let back = d.loops.get(label.stage).and_then(Option::as_ref);
        let over = back.is_some_and(|b| d.live.is_some() && b.rounds > b.at_most);
        let edge = [Edge::Loop(label.stage)];
        let style = if over { Style::default().fg(WAITING_COLOR) } else { tint.of(None, &edge) };
        let style = if style.fg == Some(tint.line) { Style::default().fg(d.muted) } else { style };
        frame.put(label.at, &label.text, style);
    }
}

fn draw_dot(frame: &mut Canvas, g: &Geometry, lit: &Lit) {
    let Some(done) = lit.dot else { return };
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

fn draw_end(frame: &mut Canvas, d: &Diagram, g: &Geometry, hover: Option<At>) {
    let Some(end) = g.end else { return };
    let reached = d.live.as_ref().is_some_and(|live| live.end.reached);
    let (mark, words) = if reached {
        (Style::default().fg(BRAND_COLOR).add_modifier(Modifier::BOLD), Style::default())
    } else {
        (Style::default().fg(d.muted), Style::default().fg(d.muted))
    };
    frame.put(end, "◆", mark);
    let at = g.below.unwrap_or((end.0 + 2, end.1));
    frame.put(at, &g.end_label, words);
    let Some(((x, y), tag)) = tag_cells(d, g) else { return };
    if !g.end_label.is_empty() {
        frame.put((x.saturating_sub(text_width(TAG_GAP)), y), TAG_GAP, Style::default().fg(d.muted));
    }
    let under = hover.is_some_and(|(hx, hy)| hy == y && (x..x.saturating_add(text_width(&tag.content))).contains(&hx));
    let style = if under { tag.style.add_modifier(Modifier::UNDERLINED) } else { tag.style };
    frame.put((x, y), &tag.content, style);
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
    let focused = matches!(placed.place, Place::Role { stage, .. } if d.focus == Some(stage));
    let working = node.state == Some(State::Agent(Some(Status::Working))) || focused;
    let borders = if working { &HEAVY } else { &LIGHT };
    let bg = if lit { Style::default().bg(hover_colour(d.light)) } else { Style::default() };
    let border = bg.fg(if working { node.colour } else { line_colour(d.light) });
    frame.fill(r, bg);
    draw_outline(frame, r, borders, border, placed.exit);
    draw_name(frame, d, node, r, bg);
    draw_words(frame, d, node, r, bg);
}

fn draw_outline(frame: &mut Canvas, r: Rect, borders: &Borders, border: Style, exit: Option<u16>) {
    let inner = usize::from(r.width.saturating_sub(2));
    let (top, bottom) = (r.y, r.y + r.height - 1);
    let [tl, tr, bl, br] = borders.corners;
    frame.put((r.x, top), &format!("{tl}{}{tr}", borders.across.repeat(inner)), border);
    frame.put((r.x, bottom), &format!("{bl}{}{br}", borders.across.repeat(inner)), border);
    for y in top + 1..bottom {
        frame.put((r.x, y), borders.side, border);
        frame.put((r.right() - 1, y), borders.side, border);
    }
    if let Some(x) = exit {
        frame.put((x, bottom), borders.exit, border);
    }
}

fn draw_name(frame: &mut Canvas, d: &Diagram, node: &Node, r: Rect, bg: Style) {
    let name = text::truncate_cells(&node.role, usize::from(r.width.saturating_sub(6)));
    let name_style = if node.state == Some(State::Closed) {
        bg.fg(d.muted)
    } else {
        bg.fg(node.colour).add_modifier(Modifier::BOLD)
    };
    if r.width >= 6 {
        frame.put((r.x + 2, r.y), &format!(" {name} "), name_style);
    }
    if node.own && r.width >= 6 {
        let used = u16::try_from(text::cells(&name)).unwrap_or(u16::MAX).saturating_add(4);
        let tag = [super::OWN_LONG, super::OWN_SHORT]
            .into_iter()
            .find(|tag| u16::try_from(text::cells(tag)).is_ok_and(|w| w + 1 + used <= r.width));
        if let Some(tag) = tag {
            let x = r.right() - 1 - u16::try_from(text::cells(tag)).unwrap_or(0);
            frame.put((x, r.y), tag, bg.fg(d.muted));
        }
    }
}

fn draw_words(frame: &mut Canvas, d: &Diagram, node: &Node, r: Rect, bg: Style) {
    let icon = match node.state {
        Some(State::Agent(Some(status))) => Some(status_icon(d.muted, status, d.spin)),
        Some(State::Agent(None)) => Some(status_icon(d.muted, Status::Idle, d.spin)),
        Some(State::Closed) | None => None,
    };
    let reserve = if icon.is_some() { 6 } else { 4 };
    let words = if node.state == Some(State::Closed) { "closed" } else { node.text.as_str() };
    let words = text::truncate_cells(words, usize::from(r.width.saturating_sub(reserve)));
    frame.put((r.x + 2, r.y + 1), &words, bg.fg(d.muted));
    if let Some(icon) = icon.filter(|_| r.width >= 5) {
        frame.put((r.right() - 3, r.y + 1), &icon.content, bg.patch(icon.style));
    }
}
