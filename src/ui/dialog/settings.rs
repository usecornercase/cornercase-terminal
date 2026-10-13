use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::issues::{issue_buttons_in, issues_area};
use super::{
    Note, draw_dialog_buttons, draw_input, draw_tabs, first_visible, form_inner, list_item, pick_background, tabs_in,
};
use crate::text::truncate_right;
use crate::ui::View;
use crate::ui::rows::{GAP, row_close_button};
use crate::ui::widgets::{hovered, overlay_block, rail};

pub fn settings_area(area: Rect) -> Rect {
    issues_area(area)
}

fn settings_rows(settings: Rect) -> [Rect; 5] {
    let [tabs, _, body, edit, note, buttons] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(GAP),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(form_inner(settings));
    [tabs, body, edit, note, buttons]
}

pub fn settings_tabs(settings: Rect, names: &[&str]) -> Vec<Rect> {
    tabs_in(settings_rows(settings)[0], names)
}

pub fn settings_body(settings: Rect) -> Rect {
    settings_rows(settings)[1]
}

pub fn settings_edit(settings: Rect) -> Rect {
    settings_rows(settings)[2]
}

pub fn settings_done(settings: Rect) -> Rect {
    settings_buttons(settings)[1]
}

pub fn settings_restart(settings: Rect) -> Rect {
    settings_buttons(settings)[0]
}

fn settings_buttons(settings: Rect) -> [Rect; 2] {
    let rects = issue_buttons_in(settings_rows(settings)[4], &[crate::settings::RESTART, crate::settings::DONE]);
    [rects[0], rects[1]]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsLine {
    Blank,
    Header(usize),
    Row(usize),
}

pub fn settings_lines(sections: &[&str]) -> Vec<SettingsLine> {
    let mut lines = Vec::new();
    for (i, section) in sections.iter().enumerate() {
        if !section.is_empty() && (i == 0 || sections[i - 1] != *section) {
            if i > 0 {
                lines.push(SettingsLine::Blank);
            }
            lines.push(SettingsLine::Header(i));
        }
        lines.push(SettingsLine::Row(i));
    }
    lines
}

pub fn settings_scroll(sections: &[&str], cursor: usize, height: usize) -> usize {
    let lines = settings_lines(sections);
    let at = lines.iter().position(|l| *l == SettingsLine::Row(cursor)).unwrap_or(0);
    let header = at.saturating_sub(1);
    if at < height { 0 } else { (at + 1).saturating_sub(height).min(header) }
}

pub fn settings_row(area: Rect, sections: &[&str], cursor: usize, row: usize) -> Rect {
    let body = settings_body(settings_area(area));
    let scroll = settings_scroll(sections, cursor, usize::from(body.height));
    let Some(line) = settings_lines(sections).iter().position(|l| *l == SettingsLine::Row(row)) else {
        return Rect::default();
    };
    let Some(visible) = line.checked_sub(scroll) else { return Rect::default() };
    let y = body.y.saturating_add(u16::try_from(visible).unwrap_or(u16::MAX));
    Rect::new(body.x, y, body.width, 1).intersection(body)
}

pub fn settings_remove(row: Rect) -> Rect {
    row_close_button(row, 1)
}

pub fn settings_moves(row: Rect) -> [Rect; 2] {
    let down = Rect::new(row.right().saturating_sub(3), row.y, 3, 1).intersection(row);
    let up = Rect::new(row.right().saturating_sub(6), row.y, 3, 1).intersection(row);
    [up, down]
}

pub fn settings_pick_list(settings: Rect) -> Rect {
    let body = settings_body(settings);
    Rect::new(body.x, body.y.saturating_add(2), body.width, body.height.saturating_sub(2))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsHit {
    Tab(usize),
    Row(usize),
    Remove(usize),
    MoveUp(usize),
    MoveDown(usize),
    Pick(usize),
    Done,
    Restart,
}

pub struct SettingsLayout<'a> {
    pub tabs: &'a [&'a str],
    pub sections: &'a [&'a str],
    pub removable: &'a [bool],
    pub movable: &'a [bool],
    pub cursor: usize,
    pub pick: Option<(usize, usize)>,
}

pub fn settings_hit(area: Rect, layout: &SettingsLayout, pos: Position) -> Option<SettingsHit> {
    let settings = settings_area(area);
    if settings_done(settings).contains(pos) {
        return Some(SettingsHit::Done);
    }
    if settings_restart(settings).contains(pos) {
        return Some(SettingsHit::Restart);
    }
    if let Some(i) = settings_tabs(settings, layout.tabs).iter().position(|r| r.contains(pos)) {
        return Some(SettingsHit::Tab(i));
    }
    if let Some((items, scroll)) = layout.pick {
        let list = settings_pick_list(settings);
        if !list.contains(pos) {
            return None;
        }
        let i = first_visible(list, items, scroll) + usize::from(pos.y - list.y);
        return (i < items).then_some(SettingsHit::Pick(i));
    }
    let i =
        (0..layout.sections.len()).find(|&i| settings_row(area, layout.sections, layout.cursor, i).contains(pos))?;
    let row = settings_row(area, layout.sections, layout.cursor, i);
    if layout.removable.get(i).copied().unwrap_or(false) && settings_remove(row).contains(pos) {
        return Some(SettingsHit::Remove(i));
    }
    if layout.movable.get(i).copied().unwrap_or(false) {
        let [up, down] = settings_moves(row);
        if up.contains(pos) {
            return Some(SettingsHit::MoveUp(i));
        }
        if down.contains(pos) {
            return Some(SettingsHit::MoveDown(i));
        }
    }
    Some(SettingsHit::Row(i))
}

pub struct SettingsRow {
    pub section: &'static str,
    pub label: String,
    pub value: String,
    pub note: String,
    pub dangerous: bool,
    pub removable: bool,
    pub movable: bool,
}

pub struct SettingsEdit {
    pub label: String,
    pub value: String,
}

pub struct SettingsPick {
    pub title: String,
    pub filter: String,
    pub items: Vec<(String, String, bool)>,
    pub selected: Option<usize>,
    pub scroll: usize,
}

pub struct Settings {
    pub tabs: Vec<&'static str>,
    pub tab: usize,
    pub rows: Vec<SettingsRow>,
    pub cursor: usize,
    pub edit: Option<SettingsEdit>,
    pub pick: Option<SettingsPick>,
    pub note: Option<Note>,
    pub hint: String,
    pub submit: &'static str,
}

impl Settings {
    pub fn sections(&self) -> Vec<&'static str> {
        self.rows.iter().map(|r| r.section).collect()
    }
}

pub(in crate::ui) fn draw_settings(f: &mut Frame, view: &View, settings: &Settings) {
    let area = f.area();
    let r = settings_area(area);
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, "Settings"), r);
    let [_, body, edit_row, note, _] = settings_rows(r);
    let dim = Style::default().fg(view.muted);
    let mut cursor = None;
    draw_tabs(f, view, &settings_tabs(r, &settings.tabs), &settings.tabs, settings.tab);

    if let Some(pick) = &settings.pick {
        cursor = draw_input(f, view.muted, Rect { height: 1, ..body }, &pick.title, &pick.filter);
        let list = settings_pick_list(r);
        if pick.items.is_empty() {
            f.render_widget(Paragraph::new(Span::styled(" nothing matches", dim)), list);
        }
        for (i, (value, item_note, dangerous)) in pick.items.iter().enumerate() {
            let row = list_item(list, pick.items.len(), pick.scroll, i);
            if row.is_empty() {
                continue;
            }
            let selected = pick.selected == Some(i);
            let base = pick_background(view, selected, row);
            let value_style = if *dangerous { base.fg(Color::Red) } else { base };
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    rail(selected, base),
                    Span::styled(format!("{value:<32} "), value_style.add_modifier(Modifier::BOLD)),
                    Span::styled(item_note.clone(), base.fg(view.muted)),
                ]))
                .style(base),
                row,
            );
        }
    } else {
        let sections = settings.sections();
        let scroll = settings_scroll(&sections, settings.cursor, usize::from(body.height));
        let lines = settings_lines(&sections);
        for (n, line) in lines.iter().skip(scroll).take(usize::from(body.height)).enumerate() {
            let y = body.y + u16::try_from(n).unwrap_or(0);
            let rect = Rect::new(body.x, y, body.width, 1);
            match line {
                SettingsLine::Blank => {}
                SettingsLine::Header(i) => f.render_widget(
                    Paragraph::new(Span::styled(settings.rows[*i].section, dim.add_modifier(Modifier::BOLD))),
                    rect,
                ),
                SettingsLine::Row(i) => draw_settings_row(f, view, &settings.rows[*i], *i == settings.cursor, rect),
            }
        }
    }

    if let Some(edit) = &settings.edit {
        cursor = draw_input(f, view.muted, edit_row, &format!("{}:", edit.label), &edit.value);
    }
    let (text, style) = match &settings.note {
        Some(Note::Error(text)) => (text.as_str(), Style::default().fg(Color::Red)),
        Some(Note::Busy(text)) => (*text, dim),
        None => (settings.hint.as_str(), dim),
    };
    f.render_widget(Paragraph::new(Span::styled(truncate_right(text, usize::from(note.width)), style)), note);
    let buttons = [settings_done(r), settings_restart(r)];
    draw_dialog_buttons(f, view, buttons, settings.submit, crate::settings::RESTART);
    if let Some(pos) = cursor.filter(|_| !matches!(settings.note, Some(Note::Busy(_)))) {
        f.set_cursor_position(pos);
    }
}

fn draw_settings_row(f: &mut Frame, view: &View, row: &SettingsRow, selected: bool, r: Rect) {
    let dim = Style::default().fg(view.muted);
    let (marker, bg) = if selected { ("▌ ", Style::default().bg(view.surface())) } else { ("  ", Style::default()) };
    let label_style = if selected { bg.add_modifier(Modifier::BOLD) } else { bg };
    let value_style = if row.dangerous { bg.fg(Color::Red) } else { bg };
    let width = usize::from(r.width);
    let label = truncate_right(&row.label, 24);
    let value = truncate_right(&row.value, width.saturating_sub(30) / 2 + 10);
    let spans = vec![
        Span::styled(marker, bg.fg(Color::Cyan)),
        Span::styled(format!("{label:<26}"), label_style),
        Span::styled(value.clone(), value_style),
        Span::styled(if value.is_empty() || row.note.is_empty() { String::new() } else { "  ".into() }, bg),
        Span::styled(row.note.clone(), bg.fg(view.muted)),
    ];
    f.render_widget(Paragraph::new(Line::from(spans)).style(bg), r);
    if !hovered(view, r) {
        return;
    }
    if row.removable {
        let close = settings_remove(r);
        let style = if hovered(view, close) { bg.fg(Color::Red).add_modifier(Modifier::BOLD) } else { bg.patch(dim) };
        f.render_widget(Paragraph::new(Span::styled(" × ", style)), close);
    }
    if row.movable {
        for (rect, arrow) in settings_moves(r).into_iter().zip([" ↑ ", " ↓ "]) {
            let style =
                if hovered(view, rect) { bg.fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { bg.patch(dim) };
            f.render_widget(Paragraph::new(Span::styled(arrow, style)), rect);
        }
    }
}
