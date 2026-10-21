use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::issues::issue_buttons_in;
use super::{
    Note, draw_dialog_buttons, draw_input, draw_secondary, draw_tabs, first_visible, form_inner, list_item,
    pick_background, tabs_in,
};
use crate::text::truncate_right;
use crate::ui::View;
use crate::ui::rows::{GAP, row_close_button};
use crate::ui::widgets::{hovered, overlay_block, rail};

const LABEL_WIDTH: usize = 26;

pub(crate) fn settings_area(area: Rect) -> Rect {
    super::flows::flows_area(area)
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

pub(crate) fn settings_tabs(settings: Rect, names: &[&str]) -> Vec<Rect> {
    tabs_in(settings_rows(settings)[0], names)
}

pub(crate) fn settings_body(settings: Rect) -> Rect {
    settings_rows(settings)[1]
}

#[cfg(test)]
pub(crate) fn settings_edit(settings: Rect) -> Rect {
    settings_rows(settings)[2]
}

pub(crate) fn settings_done(settings: Rect) -> Rect {
    settings_buttons(settings)[2]
}

pub(crate) fn settings_restart(settings: Rect) -> Rect {
    settings_buttons(settings)[1]
}

pub(crate) fn settings_share(settings: Rect) -> Rect {
    settings_buttons(settings)[0]
}

fn settings_buttons(settings: Rect) -> [Rect; 3] {
    let labels = [crate::settings::SHARE, crate::settings::RESTART, crate::settings::DONE];
    let rects = issue_buttons_in(settings_rows(settings)[4], &labels);
    [rects[0], rects[1], rects[2]]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsLine {
    Blank,
    Header(usize),
    Row(usize),
}

pub(crate) fn settings_lines(sections: &[&str]) -> Vec<SettingsLine> {
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

pub(crate) fn settings_scroll(sections: &[&str], cursor: usize, height: usize) -> usize {
    let lines = settings_lines(sections);
    let at = lines.iter().position(|l| *l == SettingsLine::Row(cursor)).unwrap_or(0);
    let header = at.saturating_sub(1);
    if at < height { 0 } else { (at + 1).saturating_sub(height).min(header) }
}

#[cfg(test)]
pub(crate) fn settings_row(area: Rect, sections: &[&str], cursor: usize, row: usize) -> Rect {
    line_rect(settings_body(settings_area(area)), sections, cursor, row)
}

pub(crate) fn line_rect(body: Rect, sections: &[&str], cursor: usize, row: usize) -> Rect {
    let scroll = settings_scroll(sections, cursor, usize::from(body.height));
    let Some(line) = settings_lines(sections).iter().position(|l| *l == SettingsLine::Row(row)) else {
        return Rect::default();
    };
    let Some(visible) = line.checked_sub(scroll) else { return Rect::default() };
    let y = body.y.saturating_add(u16::try_from(visible).unwrap_or(u16::MAX));
    Rect::new(body.x, y, body.width, 1).intersection(body)
}

pub(crate) fn settings_remove(row: Rect) -> Rect {
    row_close_button(row, 1)
}

fn moves_beside(row: Rect, removable: bool) -> [Rect; 2] {
    let close = if removable { settings_remove(row).width } else { 0 };
    settings_moves(Rect { width: row.width.saturating_sub(close), ..row })
}

pub(crate) fn settings_moves(row: Rect) -> [Rect; 2] {
    let down = Rect::new(row.right().saturating_sub(3), row.y, 3, 1).intersection(row);
    let up = Rect::new(row.right().saturating_sub(6), row.y, 3, 1).intersection(row);
    [up, down]
}

pub(crate) fn settings_pick_list(settings: Rect) -> Rect {
    pick_list_in(settings_body(settings))
}

pub(crate) fn pick_list_in(body: Rect) -> Rect {
    Rect::new(body.x, body.y.saturating_add(2), body.width, body.height.saturating_sub(2))
}

pub(in crate::ui) fn pick_hit(list: Rect, items: usize, scroll: usize, pos: Position) -> Option<usize> {
    if !list.contains(pos) {
        return None;
    }
    let i = first_visible(list, items, scroll) + usize::from(pos.y - list.y);
    (i < items).then_some(i)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsHit {
    Tab(usize),
    Row(usize),
    Remove(usize),
    MoveUp(usize),
    MoveDown(usize),
    Pick(usize),
    Done,
    Restart,
    Share,
}

pub(crate) struct SettingsLayout<'a> {
    pub(crate) tabs: &'a [&'a str],
    pub(crate) sections: &'a [&'a str],
    pub(crate) removable: &'a [bool],
    pub(crate) movable: &'a [bool],
    pub(crate) cursor: usize,
    pub(crate) pick: Option<(usize, usize)>,
}

pub(crate) fn settings_hit(area: Rect, layout: &SettingsLayout, pos: Position) -> Option<SettingsHit> {
    let settings = settings_area(area);
    if settings_done(settings).contains(pos) {
        return Some(SettingsHit::Done);
    }
    if settings_restart(settings).contains(pos) {
        return Some(SettingsHit::Restart);
    }
    if settings_share(settings).contains(pos) {
        return Some(SettingsHit::Share);
    }
    if let Some(i) = settings_tabs(settings, layout.tabs).iter().position(|r| r.contains(pos)) {
        return Some(SettingsHit::Tab(i));
    }
    if let Some((items, scroll)) = layout.pick {
        return pick_hit(settings_pick_list(settings), items, scroll, pos).map(SettingsHit::Pick);
    }
    rows_hit(settings_body(settings), layout, pos)
}

pub(in crate::ui) fn hit_in_rows(
    body: Rect,
    rows: &[SettingsRow],
    cursor: usize,
    pos: Position,
) -> Option<SettingsHit> {
    let sections: Vec<&str> = rows.iter().map(|r| r.section).collect();
    let removable: Vec<bool> = rows.iter().map(|r| r.removable).collect();
    let movable: Vec<bool> = rows.iter().map(|r| r.movable).collect();
    let layout =
        SettingsLayout { tabs: &[], sections: &sections, removable: &removable, movable: &movable, cursor, pick: None };
    rows_hit(body, &layout, pos)
}

fn rows_hit(body: Rect, layout: &SettingsLayout, pos: Position) -> Option<SettingsHit> {
    let i = (0..layout.sections.len()).find(|&i| line_rect(body, layout.sections, layout.cursor, i).contains(pos))?;
    let row = line_rect(body, layout.sections, layout.cursor, i);
    if layout.removable.get(i).copied().unwrap_or(false) && settings_remove(row).contains(pos) {
        return Some(SettingsHit::Remove(i));
    }
    if layout.movable.get(i).copied().unwrap_or(false) {
        let [up, down] = moves_beside(row, layout.removable.get(i).copied().unwrap_or(false));
        if up.contains(pos) {
            return Some(SettingsHit::MoveUp(i));
        }
        if down.contains(pos) {
            return Some(SettingsHit::MoveDown(i));
        }
    }
    Some(SettingsHit::Row(i))
}

#[expect(clippy::struct_excessive_bools, reason = "one flag for each way a row can be drawn")]
pub(crate) struct SettingsRow {
    pub(crate) section: &'static str,
    pub(crate) label: String,
    pub(crate) value: String,
    pub(crate) note: String,
    pub(crate) dangerous: bool,
    pub(crate) muted: bool,
    pub(crate) removable: bool,
    pub(crate) movable: bool,
}

pub(crate) struct SettingsEdit {
    pub(crate) label: String,
    pub(crate) value: String,
}

pub(crate) struct SettingsPick {
    pub(crate) title: String,
    pub(crate) filter: String,
    pub(crate) items: Vec<(String, String, bool)>,
    pub(crate) selected: Option<usize>,
    pub(crate) scroll: usize,
}

pub(crate) struct Settings {
    pub(crate) tabs: Vec<&'static str>,
    pub(crate) tab: usize,
    pub(crate) rows: Vec<SettingsRow>,
    pub(crate) cursor: usize,
    pub(crate) edit: Option<SettingsEdit>,
    pub(crate) pick: Option<SettingsPick>,
    pub(crate) note: Option<Note>,
    pub(crate) hint: String,
    pub(crate) submit: &'static str,
}

impl Settings {
    #[cfg(test)]
    pub(crate) fn sections(&self) -> Vec<&'static str> {
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
        cursor = draw_pick(f, view, body, pick);
    } else {
        let columns = Columns { label: LABEL_WIDTH, fill: false };
        draw_rows(f, view, body, Table { rows: &settings.rows, cursor: settings.cursor, columns });
    }

    if let Some(edit) = &settings.edit {
        cursor = draw_input(f, view.muted, edit_row, &format!("{}:", edit.label), &edit.value);
    }
    let (text, style) = match &settings.note {
        Some(Note::Error(text)) => (text.as_str(), Style::default().fg(Color::Red)),
        Some(Note::Busy(text)) => (*text, dim),
        Some(Note::Info(text)) => (text.as_str(), dim),
        None => (settings.hint.as_str(), dim),
    };
    f.render_widget(Paragraph::new(Span::styled(truncate_right(text, usize::from(note.width)), style)), note);
    let buttons = [settings_done(r), settings_restart(r)];
    draw_dialog_buttons(f, view, buttons, settings.submit, crate::settings::RESTART);
    draw_secondary(f, view, settings_share(r), crate::settings::SHARE);
    if let Some(pos) = cursor.filter(|_| !matches!(settings.note, Some(Note::Busy(_)))) {
        f.set_cursor_position(pos);
    }
}

pub(in crate::ui) fn draw_pick(f: &mut Frame, view: &View, body: Rect, pick: &SettingsPick) -> Option<Position> {
    let cursor = draw_input(f, view.muted, Rect { height: 1, ..body }, &pick.title, &pick.filter);
    let list = pick_list_in(body);
    if pick.items.is_empty() {
        f.render_widget(Paragraph::new(Span::styled(" nothing matches", Style::default().fg(view.muted))), list);
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
    cursor
}

pub(in crate::ui) struct Columns {
    pub(in crate::ui) label: usize,
    pub(in crate::ui) fill: bool,
}

pub(in crate::ui) struct Table<'a> {
    pub(in crate::ui) rows: &'a [SettingsRow],
    pub(in crate::ui) cursor: usize,
    pub(in crate::ui) columns: Columns,
}

pub(in crate::ui) fn draw_rows(f: &mut Frame, view: &View, body: Rect, table: Table) {
    let Table { rows, cursor, columns } = table;
    let dim = Style::default().fg(view.muted);
    let sections: Vec<&str> = rows.iter().map(|r| r.section).collect();
    let scroll = settings_scroll(&sections, cursor, usize::from(body.height));
    for (n, line) in settings_lines(&sections).iter().skip(scroll).take(usize::from(body.height)).enumerate() {
        let y = body.y + u16::try_from(n).unwrap_or(0);
        let rect = Rect::new(body.x, y, body.width, 1);
        match line {
            SettingsLine::Blank => {}
            SettingsLine::Header(i) => {
                f.render_widget(Paragraph::new(Span::styled(rows[*i].section, dim.add_modifier(Modifier::BOLD))), rect);
            }
            SettingsLine::Row(i) => {
                draw_settings_row(f, view, &rows[*i], Placed { r: rect, selected: *i == cursor }, &columns);
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Placed {
    r: Rect,
    selected: bool,
}

fn draw_settings_row(f: &mut Frame, view: &View, row: &SettingsRow, at: Placed, columns: &Columns) {
    let Placed { r, selected } = at;
    let bg = if selected { Style::default().bg(view.surface()) } else { Style::default() };
    let spans = settings_row_spans(view, row, selected, r.width, columns);
    f.render_widget(Paragraph::new(Line::from(spans)).style(bg), r);
    if hovered(view, r) {
        draw_row_buttons(f, view, row, r, bg);
    }
}

fn settings_row_spans(
    view: &View,
    row: &SettingsRow,
    selected: bool,
    width: u16,
    columns: &Columns,
) -> Vec<Span<'static>> {
    let label = columns.label;
    let (marker, bg) = if selected { ("▌ ", Style::default().bg(view.surface())) } else { ("  ", Style::default()) };
    let (label_style, value_style) = if row.muted {
        (bg.fg(view.muted), bg.fg(view.muted))
    } else {
        (
            if selected { bg.add_modifier(Modifier::BOLD) } else { bg },
            if row.dangerous { bg.fg(Color::Red) } else { bg },
        )
    };
    let width = usize::from(width);
    let room = if columns.fill {
        let buttons = if row.removable || row.movable { 9 } else { 0 };
        let note = if row.note.is_empty() { 0 } else { row.note.chars().count() + 2 };
        width.saturating_sub(2 + label + note + buttons)
    } else {
        width.saturating_sub(30) / 2 + 10
    };
    let value = truncate_right(&row.value, room);
    let padded = format!("{:<label$}", truncate_right(&row.label, label.saturating_sub(2)));
    vec![
        Span::styled(marker, bg.fg(Color::Cyan)),
        Span::styled(padded, label_style),
        Span::styled(value.clone(), value_style),
        Span::styled(if value.is_empty() || row.note.is_empty() { String::new() } else { "  ".into() }, bg),
        Span::styled(row.note.clone(), bg.fg(view.muted)),
    ]
}

fn draw_row_buttons(f: &mut Frame, view: &View, row: &SettingsRow, r: Rect, bg: Style) {
    let dim = Style::default().fg(view.muted);
    if row.removable {
        let close = settings_remove(r);
        let style = if hovered(view, close) { bg.fg(Color::Red).add_modifier(Modifier::BOLD) } else { bg.patch(dim) };
        f.render_widget(Paragraph::new(Span::styled(" × ", style)), close);
    }
    if row.movable {
        for (rect, arrow) in moves_beside(r, row.removable).into_iter().zip([" ↑ ", " ↓ "]) {
            let style =
                if hovered(view, rect) { bg.fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { bg.patch(dim) };
            f.render_widget(Paragraph::new(Span::styled(arrow, style)), rect);
        }
    }
}
