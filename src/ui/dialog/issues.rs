use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};

use super::{
    Note, dialog_button_style, draw_input, draw_tabs, first_visible, form_inner, list_item, pick_background,
    right_aligned, tabs_in,
};
use crate::issues::browser::IssuesHit;
use crate::text::truncate_right;
use crate::ui::View;
use crate::ui::widgets::{button_width, hovered, overlay_block, rail};

const ISSUES_WIDTH: u16 = 110;

const ISSUES_HEIGHT: u16 = 30;

pub fn issues_area(area: Rect) -> Rect {
    let width = area.width.saturating_sub(4).min(ISSUES_WIDTH);
    let height = area.height.saturating_sub(2).min(ISSUES_HEIGHT);
    Rect::new(area.x + (area.width - width) / 2, area.y + (area.height - height) / 2, width, height)
}

pub(in crate::ui) fn issues_rows(issues: Rect) -> [Rect; 5] {
    let [tabs, _, input, _, list, note, buttons] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(form_inner(issues));
    [tabs, input, list, note, buttons]
}

pub(crate) fn issues_list(issues: Rect) -> Rect {
    issues_rows(issues)[2]
}

pub(crate) fn issue_detail(issues: Rect) -> Rect {
    let [tabs, _, _, note, _] = issues_rows(issues);
    Rect::new(tabs.x, tabs.y, tabs.width, note.y.saturating_sub(tabs.y))
}

pub(crate) fn issue_tabs(issues: Rect, names: &[&str]) -> Vec<Rect> {
    tabs_in(issues_rows(issues)[0], names)
}

fn toggle_width(label: &str) -> u16 {
    button_width(label).saturating_add(4)
}

pub(crate) fn issue_toggles(issues: Rect, labels: &[&str]) -> Vec<Rect> {
    right_aligned(issues_rows(issues)[0], labels, toggle_width, 2)
}

pub(crate) fn issue_buttons_width(issues: Rect) -> u16 {
    issues_rows(issues)[4].width
}

pub fn issue_buttons(issues: Rect, labels: &[&str]) -> Vec<Rect> {
    issue_buttons_in(issues_rows(issues)[4], labels)
}

pub(super) fn issue_buttons_in(row: Rect, labels: &[&str]) -> Vec<Rect> {
    right_aligned(row, labels, button_width, 1)
}

pub(crate) fn issue_button_hit(area: Rect, buttons: &[&str], pos: Position) -> Option<usize> {
    issue_buttons(issues_area(area), buttons).iter().position(|r| r.contains(pos))
}

pub(crate) fn issues_hit(
    area: Rect,
    tabs: &[&str],
    toggles: &[&str],
    buttons: &[&str],
    items: usize,
    scroll: usize,
    pos: Position,
) -> Option<IssuesHit> {
    let issues = issues_area(area);
    if let Some(i) = issue_tabs(issues, tabs).iter().position(|r| r.contains(pos)) {
        return Some(IssuesHit::Tab(i));
    }
    if let Some(i) = issue_toggles(issues, toggles).iter().position(|r| r.contains(pos)) {
        return Some(IssuesHit::Toggle(i));
    }
    if let Some(i) = issue_button_hit(area, buttons, pos) {
        return Some(IssuesHit::Button(i));
    }
    let list = issues_list(issues);
    if !list.contains(pos) {
        return None;
    }
    let i = first_visible(list, items, scroll) + usize::from(pos.y - list.y);
    (i < items).then_some(IssuesHit::Item(i))
}

pub(crate) struct IssueRow {
    pub(crate) key: String,
    pub(crate) title: String,
    pub(crate) meta: String,
}

pub(crate) enum IssuesBody {
    List { filter: String, items: Vec<IssueRow>, selected: Option<usize>, scroll: usize, empty: String },
    Token { label: &'static str, input: String, help: Vec<String> },
    Detail { lines: Vec<Line<'static>>, scroll: usize },
}

pub(crate) struct Issues {
    pub(crate) title: String,
    pub(crate) tabs: Vec<&'static str>,
    pub(crate) tab: usize,
    pub(crate) toggles: Vec<String>,
    pub(crate) on: Vec<bool>,
    pub(crate) body: IssuesBody,
    pub(crate) note: Option<Note>,
    pub(crate) hint: String,
    pub(crate) buttons: Vec<&'static str>,
}

pub(in crate::ui) fn draw_issues(f: &mut Frame, view: &View, issues: &Issues) {
    let r = issues_area(f.area());
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, &issues.title), r);
    let [_, input, list, note, _] = issues_rows(r);
    let dim = Style::default().fg(view.muted);

    let cursor = match &issues.body {
        IssuesBody::Detail { lines, scroll } => {
            let area = issue_detail(r);
            let visible: Vec<Line> = lines.iter().skip(*scroll).take(usize::from(area.height)).cloned().collect();
            f.render_widget(Paragraph::new(visible), area);
            None
        }
        IssuesBody::List { filter, items, selected, scroll, empty } => {
            draw_issue_tabs(f, view, issues, r);
            let cursor = draw_input(f, view.muted, input, "", filter);
            if items.is_empty() {
                f.render_widget(Paragraph::new(Span::styled(format!(" {empty}"), dim)), list);
            }
            draw_issue_rows(f, view, list, items, *selected, *scroll);
            cursor
        }
        IssuesBody::Token { label, input: typed, help } => {
            draw_issue_tabs(f, view, issues, r);
            let cursor = draw_input(f, view.muted, input, label, typed);
            let text: Vec<Line> = help.iter().map(|h| Line::from(h.as_str())).collect();
            f.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), list.inner(Margin::new(1, 0)));
            cursor
        }
    };

    let (text, style) = match &issues.note {
        Some(Note::Error(text)) => (text.as_str(), Style::default().fg(Color::Red)),
        Some(Note::Busy(text)) => (*text, dim),
        None => (issues.hint.as_str(), dim),
    };
    f.render_widget(Paragraph::new(Span::styled(truncate_right(text, usize::from(note.width)), style)), note);

    let rects = issue_buttons(r, &issues.buttons);
    for (i, (rect, label)) in rects.iter().zip(&issues.buttons).enumerate() {
        let style = dialog_button_style(view, *rect, i == 0);
        f.render_widget(Paragraph::new(Span::styled(format!(" {label} "), style)), *rect);
    }

    if let Some(pos) = cursor.filter(|_| !matches!(issues.note, Some(Note::Busy(_)))) {
        f.set_cursor_position(pos);
    }
}

fn draw_issue_tabs(f: &mut Frame, view: &View, issues: &Issues, r: Rect) {
    draw_tabs(f, view, &issue_tabs(r, &issues.tabs), &issues.tabs, issues.tab);
    let labels: Vec<&str> = issues.toggles.iter().map(String::as_str).collect();
    for (rect, (label, on)) in issue_toggles(r, &labels).iter().zip(issues.toggles.iter().zip(&issues.on)) {
        let mark = if *on { "[x]" } else { "[ ]" };
        let style = if hovered(view, *rect) { Style::default().fg(Color::Cyan) } else { Style::default() };
        f.render_widget(Paragraph::new(Span::styled(format!(" {mark} {label} "), style)), *rect);
    }
}

fn draw_issue_rows(f: &mut Frame, view: &View, list: Rect, items: &[IssueRow], selected: Option<usize>, scroll: usize) {
    let key_width = items.iter().map(|i| i.key.chars().count()).max().unwrap_or(0);
    for (i, item) in items.iter().enumerate() {
        let row = list_item(list, items.len(), scroll, i);
        if row.is_empty() {
            continue;
        }
        let chosen = selected == Some(i);
        let bg = pick_background(view, chosen, row);
        let style = if chosen { bg.add_modifier(Modifier::BOLD) } else { bg };
        let width = usize::from(row.width);
        let meta = truncate_right(&item.meta, width / 3);
        let meta_width = meta.chars().count();
        let title = truncate_right(&item.title, width.saturating_sub(key_width + meta_width + 6));
        let gap = width.saturating_sub(key_width + title.chars().count() + meta_width + 5);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                rail(chosen, bg),
                Span::styled(format!("{:<key_width$}  ", item.key), style.fg(Color::Cyan)),
                Span::styled(format!("{title}{}", " ".repeat(gap)), style),
                Span::styled(format!("{meta} "), bg.fg(view.muted)),
            ]))
            .style(bg),
            row,
        );
    }
}
