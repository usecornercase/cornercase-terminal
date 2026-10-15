use std::ops::Range;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{first_visible_by, list_item_by, pick_background};
use crate::text::{truncate_left, truncate_right};
use crate::ui::View;
use crate::ui::rows::GAP;
use crate::ui::widgets::{middle, rail};

fn results_rows(results: Rect) -> [Rect; 2] {
    let [list, _, hint] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(GAP), Constraint::Length(1)]).areas(results);
    [list, hint]
}

pub(crate) fn results_list(results: Rect) -> Rect {
    results_rows(results)[0]
}

pub(crate) const SESSIONS_PITCH: u16 = 2;
pub(crate) const MODE_WIDTH: u16 = 3;

pub(crate) fn mode_button(bar: Rect) -> Rect {
    let r = middle(bar);
    let width = MODE_WIDTH.min(r.width);
    Rect { x: r.right() - width, width, ..r }
}

pub(crate) fn result_item(results: Rect, items: usize, scroll: usize, i: usize, pitch: u16) -> Rect {
    list_item_by(results_list(results), items, scroll, i, pitch)
}

pub(crate) fn result_hit(results: Rect, items: usize, scroll: usize, pos: Position, pitch: u16) -> Option<usize> {
    let list = results_list(results);
    if !list.contains(pos) {
        return None;
    }
    let i = first_visible_by(list, items, scroll, pitch) + usize::from((pos.y - list.y) / pitch.max(1));
    (i < items).then_some(i)
}

pub(crate) struct ResultRow {
    pub(crate) name: String,
    pub(crate) context: String,
    pub(crate) detail: String,
    pub(crate) hit: Option<Range<usize>>,
}

pub(crate) struct Search {
    pub(crate) query: String,
    pub(crate) results: Vec<ResultRow>,
    pub(crate) selected: usize,
    pub(crate) scroll: usize,
    pub(crate) hint: String,
    pub(crate) sessions: bool,
    pub(crate) searching: bool,
}

impl Search {
    pub(crate) fn pitch(&self) -> u16 {
        if self.sessions { SESSIONS_PITCH } else { 1 }
    }

    pub(in crate::ui) fn showing_results(&self) -> bool {
        !self.query.trim().is_empty()
    }
}

pub(in crate::ui) fn draw_results(f: &mut Frame, view: &View, search: &Search, r: Rect) {
    f.render_widget(Clear, r);
    let [list, hint] = results_rows(r);
    let dim = Style::default().fg(view.muted);
    if search.results.is_empty() && !search.searching {
        f.render_widget(Paragraph::new(Span::styled("  no matches", dim)), list);
    }
    for (i, result) in search.results.iter().enumerate() {
        let row = result_item(r, search.results.len(), search.scroll, i, search.pitch());
        if row.is_empty() {
            continue;
        }
        let selected = search.selected == i;
        let bg = pick_background(view, selected, row);
        let base = if selected { bg.add_modifier(Modifier::BOLD) } else { bg };
        let matched = base.fg(Color::Cyan).add_modifier(Modifier::BOLD);
        let width = usize::from(row.width);
        if search.sessions {
            let snippet = truncate_right(&result.name, width.saturating_sub(3));
            let mut first = vec![rail(selected, bg), Span::styled(" ", bg)];
            first.extend(highlight(&snippet, result.hit.clone(), &search.query, base, matched));
            let detail = truncate_right(&result.detail, width.saturating_sub(4));
            let room = width.saturating_sub(detail.chars().count() + 4);
            let context = truncate_left(&result.context, room);
            let gap = room.saturating_sub(context.chars().count());
            let second = vec![
                rail(selected, bg),
                Span::styled(format!(" {detail}"), bg.fg(view.muted)),
                Span::styled(" ".repeat(gap), bg),
                Span::styled(format!("{context} "), bg.fg(view.muted)),
            ];
            f.render_widget(Paragraph::new(vec![Line::from(first), Line::from(second)]).style(bg), row);
            continue;
        }
        let context = truncate_left(&result.context, width / 2);
        let context_width = context.chars().count();
        let name = truncate_right(&result.name, width.saturating_sub(context_width + 5));
        let gap = width.saturating_sub(name.chars().count() + context_width + 3);
        let mut spans = vec![rail(selected, bg), Span::styled(" ", bg)];
        spans.extend(highlight(&name, None, &search.query, base, matched));
        spans.push(Span::styled(" ".repeat(gap), bg));
        spans.push(Span::styled(format!("{context} "), bg.fg(view.muted)));
        f.render_widget(Paragraph::new(Line::from(spans)).style(bg), row);
    }
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("  {}", truncate_left(&search.hint, usize::from(hint.width).saturating_sub(2))),
            dim,
        )),
        hint,
    );
}

fn highlight(name: &str, hit: Option<Range<usize>>, query: &str, base: Style, matched: Style) -> Vec<Span<'static>> {
    let found = hit.or_else(|| crate::project::search::find(name, query).map(|(start, end)| start..end));
    let Some(Range { start, end }) = found.filter(|r| r.start < r.end) else {
        return vec![Span::styled(name.to_string(), base)];
    };
    let count = name.chars().count();
    let (start, end) = (start.min(count), end.min(count));
    let part = |from: usize, to: usize| name.chars().skip(from).take(to - from).collect::<String>();
    vec![
        Span::styled(part(0, start), base),
        Span::styled(part(start, end), matched),
        Span::styled(part(end, count), base),
    ]
}
