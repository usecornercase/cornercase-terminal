use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Clear, Paragraph};

use super::flows::{
    MAX_WARNINGS, beside, draw_buttons, draw_hint, draw_preview, draw_text, draw_warnings, flows_area, flows_wide,
    text_inside,
};
use super::issues::issue_buttons_in;
use super::settings::{Columns, SettingsHit, SettingsRow, draw_rows, hit_in_rows, settings_lines};
use super::{Note, draw_input, form_inner};
use crate::flows::Flow;
use crate::flows::start::{Focus, START};
use crate::text::truncate_right;
use crate::todo::editor::Editor;
use crate::ui::View;
use crate::ui::widgets::overlay_block;

const LIST_LABEL: usize = 22;
const PLACEHOLDER: &str = "what the team should do: a feature, a bug, an issue's text…";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Shape {
    pub(crate) warnings: usize,
    pub(crate) preview: bool,
    pub(crate) diagram: bool,
    pub(crate) lines: usize,
}

impl Shape {
    pub(crate) fn new(sections: &[&str], warnings: usize, preview: bool, diagram: bool) -> Self {
        Self { warnings, preview, diagram, lines: settings_lines(sections).len() }
    }
}

fn start_rows(area: Rect, shape: Shape) -> [Rect; 8] {
    let shown = u16::try_from(shape.warnings.min(MAX_WARNINGS)).unwrap_or(0);
    let [heading, _, body, _, branch, warnings, note, buttons] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(shown),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(form_inner(flows_area(area)));
    let split = |top| {
        Layout::vertical([top, Constraint::Length(1), Constraint::Length(1), Constraint::Fill(1)]).areas::<4>(body)
    };
    let [most, ..] = split(Constraint::Fill(2));
    let on_top = shape.preview && shape.diagram && !flows_wide(area);
    let lines = u16::try_from(shape.lines).unwrap_or(u16::MAX).min(most.height);
    let [top, _, label, prompt] = if on_top { split(Constraint::Fill(2)) } else { split(Constraint::Length(lines)) };
    [heading, top, label, prompt, branch, warnings, note, buttons]
}

fn columns(area: Rect, shape: Shape) -> (Rect, Option<Rect>) {
    let [_, top, _, prompt, ..] = start_rows(area, shape);
    beside(area, Rect { height: prompt.bottom().saturating_sub(top.y), ..top }, shape.preview)
}

fn left(rect: Rect, column: Rect) -> Rect {
    Rect { x: column.x, width: column.width, ..rect }
}

pub(crate) fn start_list(area: Rect, shape: Shape) -> Rect {
    left(start_rows(area, shape)[1], columns(area, shape).0)
}

pub(crate) fn start_preview(area: Rect, shape: Shape) -> Option<Rect> {
    match columns(area, shape) {
        (_, Some(beside)) => Some(beside),
        (_, None) if shape.preview && shape.diagram => Some(start_rows(area, shape)[1]),
        _ => None,
    }
}

fn prompt_label(area: Rect, shape: Shape) -> Rect {
    left(start_rows(area, shape)[2], columns(area, shape).0)
}

fn prompt_area(area: Rect, shape: Shape) -> Rect {
    left(start_rows(area, shape)[3], columns(area, shape).0)
}

pub(crate) fn start_prompt(area: Rect, shape: Shape) -> Rect {
    text_inside(prompt_area(area, shape))
}

pub(crate) fn start_branch(area: Rect, shape: Shape) -> Rect {
    start_rows(area, shape)[4]
}

pub(crate) fn start_buttons(area: Rect, shape: Shape, labels: &[&str]) -> Vec<Rect> {
    issue_buttons_in(start_rows(area, shape)[7], labels)
}

pub(crate) struct StartFlow {
    pub(crate) title: String,
    pub(crate) heading: String,
    pub(crate) rows: Vec<SettingsRow>,
    pub(crate) cursor: usize,
    pub(crate) prompt: Editor,
    pub(crate) branch: String,
    pub(crate) focus: Focus,
    pub(crate) warnings: Vec<String>,
    pub(crate) note: Option<Note>,
    pub(crate) hint: String,
    pub(crate) buttons: Vec<&'static str>,
    pub(crate) preview: Option<Flow>,
    pub(crate) diagram: bool,
}

impl StartFlow {
    pub(crate) fn shape(&self) -> Shape {
        let sections: Vec<&str> = self.rows.iter().map(|row| row.section).collect();
        Shape::new(&sections, self.warnings.len(), self.preview.is_some(), self.diagram)
    }

    fn list_hidden(&self, area: Rect) -> bool {
        self.preview_area(area) == Some(start_rows(area, self.shape())[1])
    }

    fn preview_area(&self, area: Rect) -> Option<Rect> {
        start_preview(area, self.shape())
    }

    fn list(&self, area: Rect) -> Rect {
        start_list(area, self.shape())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartHit {
    Row(usize),
    Prompt { line: usize, col: usize },
    Branch,
    Button(usize),
}

pub(crate) fn start_hit(area: Rect, view: &StartFlow, pos: Position) -> Option<StartHit> {
    let shape = view.shape();
    if let Some(i) = start_buttons(area, shape, &view.buttons).iter().position(|r| r.contains(pos)) {
        return Some(StartHit::Button(i));
    }
    let prompt = start_prompt(area, shape);
    if prompt.contains(pos) {
        return Some(StartHit::Prompt { line: usize::from(pos.y - prompt.y), col: usize::from(pos.x - prompt.x) });
    }
    if start_branch(area, shape).contains(pos) {
        return Some(StartHit::Branch);
    }
    if view.list_hidden(area) {
        return None;
    }
    match hit_in_rows(view.list(area), &view.rows, view.cursor, pos)? {
        SettingsHit::Row(i) => Some(StartHit::Row(i)),
        _ => None,
    }
}

pub(in crate::ui) fn draw_start_flow(f: &mut Frame, view: &View, start: &StartFlow) {
    let area = f.area();
    let r = flows_area(area);
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, &start.title), r);
    let shape = start.shape();
    let [heading, _, _, _, branch, warning_rows, note, _] = start_rows(area, shape);
    let (label, prompt) = (prompt_label(area, shape), prompt_area(area, shape));
    let dim = Style::default().fg(view.muted);
    let heading_text = truncate_right(&start.heading, usize::from(heading.width));
    f.render_widget(Paragraph::new(Span::styled(heading_text, dim.add_modifier(Modifier::BOLD))), heading);
    if let Some(preview) = start.preview_area(area) {
        draw_preview(f, view, preview, start.preview.as_ref());
    }
    if !start.list_hidden(area) {
        let columns = Columns { label: LIST_LABEL, fill: true };
        draw_rows(f, view, start.list(area), &start.rows, start.cursor, &columns);
    }
    let lit = |focus: Focus| if start.focus == focus { Style::default().fg(Color::Cyan) } else { dim };
    f.render_widget(Paragraph::new(Span::styled("prompt", lit(Focus::Prompt))), label);
    let mut cursor = draw_text(f, view, prompt, &start.prompt).filter(|_| start.focus == Focus::Prompt);
    if start.prompt.is_empty() {
        let at = text_inside(prompt);
        let placeholder = truncate_right(PLACEHOLDER, usize::from(at.width));
        f.render_widget(Paragraph::new(Span::styled(placeholder, dim.bg(view.surface()))), Rect { height: 1, ..at });
    }
    let typed = draw_input(f, view.muted, branch, "branch:", &start.branch);
    if start.focus == Focus::Branch {
        cursor = typed;
    }
    draw_warnings(f, &start.warnings, warning_rows);
    draw_hint(f, view, start.note.as_ref(), &start.hint, note);
    draw_buttons(f, view, &start_buttons(area, shape, &start.buttons), &start.buttons, START);
    if let Some(pos) = cursor {
        f.set_cursor_position(pos);
    }
}
