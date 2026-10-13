use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{first_visible, list_item, pick_background};
use crate::ui::View;
use crate::ui::rows::GAP;
use crate::ui::text::{truncate_left, truncate_right};
use crate::ui::widgets::rail;

fn results_rows(results: Rect) -> [Rect; 2] {
    let [list, _, hint] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(GAP), Constraint::Length(1)]).areas(results);
    [list, hint]
}

pub fn results_list(results: Rect) -> Rect {
    results_rows(results)[0]
}

pub fn result_item(results: Rect, items: usize, scroll: usize, i: usize) -> Rect {
    list_item(results_list(results), items, scroll, i)
}

pub fn result_hit(results: Rect, items: usize, scroll: usize, pos: Position) -> Option<usize> {
    let list = results_list(results);
    if !list.contains(pos) {
        return None;
    }
    let i = first_visible(list, items, scroll) + usize::from(pos.y - list.y);
    (i < items).then_some(i)
}

pub struct ResultRow {
    pub name: String,
    pub context: String,
}

pub struct Search {
    pub query: String,
    pub results: Vec<ResultRow>,
    pub selected: usize,
    pub scroll: usize,
    pub hint: String,
}

impl Search {
    pub(in crate::ui) fn showing_results(&self) -> bool {
        !self.query.trim().is_empty()
    }
}

pub(in crate::ui) fn draw_results(f: &mut Frame, view: &View, search: &Search, r: Rect) {
    f.render_widget(Clear, r);
    let [list, hint] = results_rows(r);
    let dim = Style::default().fg(view.muted);
    if search.results.is_empty() {
        f.render_widget(Paragraph::new(Span::styled("  no matches", dim)), list);
    }
    for (i, result) in search.results.iter().enumerate() {
        let row = result_item(r, search.results.len(), search.scroll, i);
        if row.is_empty() {
            continue;
        }
        let selected = search.selected == i;
        let bg = pick_background(view, selected, row);
        let base = if selected { bg.add_modifier(Modifier::BOLD) } else { bg };
        let matched = base.fg(Color::Cyan).add_modifier(Modifier::BOLD);
        let width = usize::from(row.width);
        let context = truncate_left(&result.context, width / 2);
        let context_width = context.chars().count();
        let name = truncate_right(&result.name, width.saturating_sub(context_width + 5));
        let gap = width.saturating_sub(name.chars().count() + context_width + 3);
        let mut spans = vec![rail(selected, bg), Span::styled(" ", bg)];
        spans.extend(highlight(&name, &search.query, base, matched));
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

fn highlight(name: &str, query: &str, base: Style, matched: Style) -> Vec<Span<'static>> {
    let Some((start, end)) = crate::search::find(name, query) else {
        return vec![Span::styled(name.to_string(), base)];
    };
    let part = |from: usize, to: usize| name.chars().skip(from).take(to - from).collect::<String>();
    vec![
        Span::styled(part(0, start), base),
        Span::styled(part(start, end), matched),
        Span::styled(part(end, name.chars().count()), base),
    ]
}
