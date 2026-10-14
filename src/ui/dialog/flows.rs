use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Clear, Paragraph};

use super::issues::issue_buttons_in;
use super::settings::{
    Columns, SettingsEdit, SettingsHit, SettingsPick, SettingsRow, draw_pick, draw_rows, hit_in_rows, pick_hit,
    pick_list_in,
};
use super::{Note, draw_input, draw_secondary, draw_submit, draw_tabs, form_inner, tabs_in};
use crate::flows::Flow;
use crate::text::truncate_right;
use crate::todo::editor::Editor;
use crate::ui::View;
use crate::ui::flow::{Diagram, draw_diagram};
use crate::ui::widgets::overlay_block;

const FLOWS_WIDTH: u16 = 140;
const FLOWS_HEIGHT: u16 = 40;
const PREVIEW_WIDTH: u16 = 46;
const WIDE_FROM: u16 = 112;
pub(in crate::ui) const MAX_WARNINGS: usize = 4;

pub(crate) fn flows_area(area: Rect) -> Rect {
    let width = area.width.saturating_sub(4).min(FLOWS_WIDTH);
    let height = area.height.saturating_sub(2).min(FLOWS_HEIGHT);
    Rect::new(area.x + (area.width - width) / 2, area.y + (area.height - height) / 2, width, height)
}

pub(crate) fn flows_wide(area: Rect) -> bool {
    form_inner(flows_area(area)).width >= WIDE_FROM
}

fn flows_rows(area: Rect, warnings: usize) -> [Rect; 6] {
    let shown = u16::try_from(warnings.min(MAX_WARNINGS)).unwrap_or(0);
    let [tabs, _, body, warnings, edit, note, buttons] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(shown),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(form_inner(flows_area(area)));
    [tabs, body, warnings, edit, note, buttons]
}

fn split(area: Rect, warnings: usize, preview: bool) -> (Rect, Option<Rect>) {
    beside(area, flows_rows(area, warnings)[1], preview)
}

pub(in crate::ui) fn beside(area: Rect, body: Rect, preview: bool) -> (Rect, Option<Rect>) {
    if !preview || !flows_wide(area) {
        return (body, None);
    }
    let width = PREVIEW_WIDTH.min(body.width);
    let form = Rect { width: body.width.saturating_sub(width + 2), ..body };
    (form, Some(Rect { x: body.right().saturating_sub(width), width, ..body }))
}

pub(in crate::ui) fn text_inside(body: Rect) -> Rect {
    Rect { x: body.x.saturating_add(1), width: body.width.saturating_sub(2), ..body }
}

pub(crate) fn flows_body(area: Rect, warnings: usize, preview: bool) -> Rect {
    split(area, warnings, preview).0
}

pub(crate) fn flows_preview(area: Rect, warnings: usize, preview: bool, diagram: bool) -> Option<Rect> {
    match split(area, warnings, preview) {
        (_, Some(beside)) => Some(beside),
        (body, None) if preview && diagram => Some(body),
        _ => None,
    }
}

pub(crate) fn flows_text(area: Rect, warnings: usize, preview: bool) -> Rect {
    text_inside(flows_body(area, warnings, preview))
}

pub(crate) fn flows_pick_rows(area: Rect, warnings: usize, preview: bool) -> usize {
    usize::from(pick_list_in(flows_body(area, warnings, preview)).height)
}

pub(crate) fn flows_tabs(area: Rect, names: &[&str]) -> Vec<Rect> {
    tabs_in(flows_rows(area, 0)[0], names)
}

pub(crate) fn flows_buttons(area: Rect, warnings: usize, labels: &[&str]) -> Vec<Rect> {
    issue_buttons_in(flows_rows(area, warnings)[5], labels)
}

pub(crate) struct Flows {
    pub(crate) title: String,
    pub(crate) heading: String,
    pub(crate) tabs: Vec<String>,
    pub(crate) tab: usize,
    pub(crate) rows: Vec<SettingsRow>,
    pub(crate) cursor: usize,
    pub(crate) label: usize,
    pub(crate) edit: Option<SettingsEdit>,
    pub(crate) pick: Option<SettingsPick>,
    pub(crate) text: Option<Editor>,
    pub(crate) warnings: Vec<String>,
    pub(crate) note: Option<Note>,
    pub(crate) hint: String,
    pub(crate) buttons: Vec<&'static str>,
    pub(crate) submit: &'static str,
    pub(crate) preview: Option<Flow>,
    pub(crate) diagram: bool,
}

impl Flows {
    fn tab_names(&self) -> Vec<&str> {
        self.tabs.iter().map(String::as_str).collect()
    }

    fn body(&self, area: Rect) -> Rect {
        flows_body(area, self.warnings.len(), self.preview.is_some())
    }

    fn preview_area(&self, area: Rect) -> Option<Rect> {
        flows_preview(area, self.warnings.len(), self.preview.is_some(), self.diagram)
    }

    fn form_hidden(&self, area: Rect) -> bool {
        self.preview_area(area) == Some(self.body(area))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlowsHit {
    Tab(usize),
    Row(SettingsHit),
    Pick(usize),
    Button(usize),
    Text { line: usize, col: usize },
}

pub(crate) fn flows_hit(area: Rect, view: &Flows, pos: Position) -> Option<FlowsHit> {
    if let Some(i) = flows_buttons(area, view.warnings.len(), &view.buttons).iter().position(|r| r.contains(pos)) {
        return Some(FlowsHit::Button(i));
    }
    if view.text.is_some() {
        let text = flows_text(area, view.warnings.len(), view.preview.is_some());
        return text
            .contains(pos)
            .then(|| FlowsHit::Text { line: usize::from(pos.y - text.y), col: usize::from(pos.x - text.x) });
    }
    if let Some(i) = flows_tabs(area, &view.tab_names()).iter().position(|r| r.contains(pos)) {
        return Some(FlowsHit::Tab(i));
    }
    if view.form_hidden(area) {
        return None;
    }
    let body = view.body(area);
    if let Some(pick) = &view.pick {
        return pick_hit(pick_list_in(body), pick.items.len(), pick.scroll, pos).map(FlowsHit::Pick);
    }
    hit_in_rows(body, &view.rows, view.cursor, pos).map(FlowsHit::Row)
}

pub(in crate::ui) fn draw_flows(f: &mut Frame, view: &View, flows: &Flows) {
    let area = f.area();
    let r = flows_area(area);
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, &flows.title), r);
    let [tabs, _, warnings, edit_row, note, _] = flows_rows(area, flows.warnings.len());
    let dim = Style::default().fg(view.muted);
    let body = flows.body(area);
    let mut cursor = None;
    if flows.tabs.is_empty() || flows.text.is_some() {
        let heading = truncate_right(&flows.heading, usize::from(tabs.width));
        f.render_widget(Paragraph::new(Span::styled(heading, dim.add_modifier(Modifier::BOLD))), tabs);
    } else {
        let names = flows.tab_names();
        draw_tabs(f, view, &flows_tabs(area, &names), &names, flows.tab);
    }
    if let Some(preview) = flows.preview_area(area) {
        draw_preview(f, view, preview, flows.preview.as_ref());
    }
    if let Some(editor) = &flows.text {
        cursor = draw_text(f, view, body, editor);
    } else if let Some(pick) = flows.pick.as_ref().filter(|_| !flows.form_hidden(area)) {
        cursor = draw_pick(f, view, body, pick);
    } else if !flows.form_hidden(area) {
        draw_rows(f, view, body, &flows.rows, flows.cursor, &Columns { label: flows.label, fill: true });
    }
    draw_warnings(f, &flows.warnings, warnings);
    if let Some(edit) = &flows.edit {
        cursor = draw_input(f, view.muted, edit_row, &format!("{}:", edit.label), &edit.value);
    }
    draw_hint(f, view, flows.note.as_ref(), &flows.hint, note);
    draw_buttons(f, view, &flows_buttons(area, flows.warnings.len(), &flows.buttons), &flows.buttons, flows.submit);
    if let Some(pos) = cursor {
        f.set_cursor_position(pos);
    }
}

pub(in crate::ui) fn draw_hint(f: &mut Frame, view: &View, note: Option<&Note>, hint: &str, r: Rect) {
    let dim = Style::default().fg(view.muted);
    let (text, style) = match note {
        Some(Note::Error(text)) => (text.as_str(), Style::default().fg(Color::Red)),
        Some(Note::Busy(text)) => (*text, dim),
        None => (hint, dim),
    };
    f.render_widget(Paragraph::new(Span::styled(truncate_right(text, usize::from(r.width)), style)), r);
}

pub(in crate::ui) fn draw_buttons(f: &mut Frame, view: &View, rects: &[Rect], labels: &[&str], submit: &str) {
    for (rect, label) in rects.iter().zip(labels) {
        if *label == submit {
            draw_submit(f, view, *rect, label);
        } else {
            draw_secondary(f, view, *rect, label);
        }
    }
}

pub(in crate::ui) fn draw_warnings(f: &mut Frame, warnings: &[String], r: Rect) {
    let red = Style::default().fg(Color::Red);
    let shown = usize::from(r.height);
    let mut lines: Vec<String> = warnings.iter().take(shown).cloned().collect();
    if warnings.len() > shown
        && let Some(last) = lines.last_mut()
    {
        *last = format!("+{} more problems", warnings.len() + 1 - shown);
    }
    for (i, line) in lines.iter().enumerate() {
        let y = r.y.saturating_add(u16::try_from(i).unwrap_or(u16::MAX));
        let row = Rect { y, height: 1, ..r };
        f.render_widget(Paragraph::new(Span::styled(truncate_right(line, usize::from(r.width)), red)), row);
    }
}

pub(in crate::ui) fn draw_preview(f: &mut Frame, view: &View, r: Rect, flow: Option<&Flow>) {
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, "Diagram"), r);
    if let Some(flow) = flow {
        let diagram = Diagram::plan(flow, view.muted, view.light);
        draw_diagram(f.buffer_mut(), r.inner(Margin::new(2, 1)), &diagram, 0, None);
    }
}

pub(in crate::ui) fn draw_text(f: &mut Frame, view: &View, body: Rect, editor: &Editor) -> Option<Position> {
    let bg = Style::default().bg(view.surface());
    f.buffer_mut().set_style(body, bg);
    let text = text_inside(body);
    let width = usize::from(text.width);
    let chars: Vec<char> = editor.text().chars().collect();
    let lines = editor.lines(width);
    for (n, range) in lines.iter().skip(editor.scroll()).take(usize::from(text.height)).enumerate() {
        let line: String = chars[range.clone()].iter().collect();
        let y = text.y.saturating_add(u16::try_from(n).unwrap_or(u16::MAX));
        f.render_widget(Paragraph::new(Span::styled(line, bg)), Rect { y, height: 1, ..text });
    }
    let (line, col) = editor.position(width);
    let row = line.checked_sub(editor.scroll()).and_then(|row| u16::try_from(row).ok())?;
    let at = Position::new(text.x.saturating_add(u16::try_from(col).ok()?), text.y.saturating_add(row));
    text.contains(at).then_some(at)
}
