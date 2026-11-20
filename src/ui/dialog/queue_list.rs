use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::issues::{draw_buttons, draw_empty_and_tail, issue_buttons_in, issues_area};
use super::{draw_input, first_visible, form_inner, list_item, pick_background};
use crate::queue::dialog::Hit;
use crate::text::truncate_right;
use crate::ui::View;
use crate::ui::widgets::{overlay_block, rail};

const MARK: u16 = 3;

pub(crate) struct QueueList {
    pub(crate) filter: String,
    pub(crate) rows: Vec<QueueRow>,
    pub(crate) selected: Option<usize>,
    pub(crate) scroll: usize,
    pub(crate) empty: String,
    pub(crate) tail: Option<String>,
    pub(crate) hint: String,
    pub(crate) buttons: Vec<&'static str>,
}

pub(crate) struct QueueRow {
    pub(crate) fold: Option<bool>,
    pub(crate) run: bool,
    pub(crate) when: String,
    pub(crate) title: String,
    pub(crate) meta: String,
    pub(crate) hidden: Option<String>,
    pub(crate) colour: Option<Color>,
    pub(crate) dim: bool,
}

fn rows(screen: Rect) -> [Rect; 4] {
    let [input, _, list, hint, buttons] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(form_inner(issues_area(screen)));
    [input, list, hint, buttons]
}

pub(crate) fn queue_list_rows(screen: Rect) -> Rect {
    rows(screen)[1]
}

pub(crate) fn queue_list_room(screen: Rect) -> (usize, usize) {
    let [_, list, _, buttons] = rows(screen);
    (usize::from(list.height), usize::from(buttons.width))
}

pub(crate) fn queue_list_buttons(screen: Rect, labels: &[&str]) -> Vec<Rect> {
    issue_buttons_in(rows(screen)[3], labels)
}

pub(crate) fn queue_list_hit(screen: Rect, view: &QueueList, pos: Position) -> Option<Hit> {
    if let Some(i) = queue_list_buttons(screen, &view.buttons).iter().position(|r| r.contains(pos)) {
        return Some(Hit::Button(i));
    }
    let list = queue_list_rows(screen);
    if !list.contains(pos) {
        return None;
    }
    let i = first_visible(list, view.rows.len(), view.scroll) + usize::from(pos.y - list.y);
    let row = view.rows.get(i)?;
    let on_mark = row.fold.is_some() && pos.x < list.x.saturating_add(MARK);
    Some(if on_mark { Hit::Fold(i) } else { Hit::Row(i) })
}

pub(in crate::ui) fn draw(f: &mut Frame, view: &View, queue: &QueueList) {
    let screen = f.area();
    let area = issues_area(screen);
    f.render_widget(Clear, area);
    f.render_widget(overlay_block(view.muted, "Queue"), area);
    let [input, list, hint, _] = rows(screen);
    let cursor = draw_input(f, view.muted, input, "", &queue.filter);
    draw_empty_and_tail(f, view, list, (queue.rows.len(), &queue.empty, queue.tail.as_deref()));
    let when_width = queue.rows.iter().map(|row| row.when.chars().count()).max().unwrap_or(0);
    for (i, row) in queue.rows.iter().enumerate() {
        let r = list_item(list, queue.rows.len(), queue.scroll, i);
        if !r.is_empty() {
            draw_row(f, view, (row, queue.selected == Some(i), when_width), r);
        }
    }
    let dim = Style::default().fg(view.muted);
    f.render_widget(Paragraph::new(Span::styled(truncate_right(&queue.hint, usize::from(hint.width)), dim)), hint);
    draw_buttons(f, view, &queue_list_buttons(screen, &queue.buttons), &queue.buttons);
    if let Some(pos) = cursor {
        f.set_cursor_position(pos);
    }
}

fn draw_row(f: &mut Frame, view: &View, (row, chosen, when_width): (&QueueRow, bool, usize), r: Rect) {
    let bg = pick_background(view, chosen, r);
    let style = if chosen { bg.add_modifier(Modifier::BOLD) } else { bg };
    let mark = match row.fold {
        Some(true) => "▸ ",
        Some(false) => "▾ ",
        None => "  ",
    };
    let indent = if row.run { "  " } else { "" };
    let width = usize::from(r.width);
    let fixed = 1 + 2 + indent.len() + when_width + 2 + 1;
    let (meta, title_style, meta_colour) = if row.run {
        (truncate_right(&row.meta, width / 2), style.fg(row.colour.unwrap_or(view.muted)), view.muted)
    } else {
        let title = if row.dim { style.fg(view.muted) } else { style };
        let hidden = row.hidden.as_deref().unwrap_or_default();
        let meta = truncate_right(&row.meta, (width / 3).saturating_sub(hidden.chars().count()));
        (format!("{meta}{hidden}"), title, row.colour.unwrap_or(view.muted))
    };
    let meta_width = meta.chars().count();
    let title = truncate_right(&row.title, width.saturating_sub(fixed + meta_width + 1));
    let gap = width.saturating_sub(fixed + title.chars().count() + meta_width);
    let line = Line::from(vec![
        rail(chosen, bg),
        Span::styled(mark, bg.fg(view.muted)),
        Span::styled(format!("{indent}{:<when_width$}  ", row.when), style.fg(Color::Cyan)),
        Span::styled(title, title_style),
        Span::styled(" ".repeat(gap), bg),
        Span::styled(format!("{meta} "), bg.fg(meta_colour)),
    ]);
    f.render_widget(Paragraph::new(line).style(bg), r);
}
