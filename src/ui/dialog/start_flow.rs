use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Clear, Paragraph};

use super::flows::{
    MAX_WARNINGS, beside, draw_buttons, draw_hint, draw_preview, draw_text, draw_warnings, flows_area, text_inside,
};
use super::issues::issue_buttons_in;
use super::settings::{Columns, SettingsHit, SettingsRow, draw_rows, hit_in_rows};
use super::{draw_input, form_inner};
use crate::flows::Flow;
use crate::flows::start::{Focus, START};
use crate::text::truncate_right;
use crate::todo::editor::Editor;
use crate::ui::View;
use crate::ui::widgets::overlay_block;

const LIST_LABEL: usize = 22;
const PLACEHOLDER: &str = "what the team should do: a feature, a bug, an issue's text…";

fn start_rows(area: Rect, warnings: usize) -> [Rect; 8] {
    let shown = u16::try_from(warnings.min(MAX_WARNINGS)).unwrap_or(0);
    let [heading, _, top, _, label, prompt, _, branch, warnings, note, buttons] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(2),
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
    [heading, top, label, prompt, branch, warnings, note, buttons]
}

pub(crate) fn start_list(area: Rect, warnings: usize, preview: bool) -> Rect {
    beside(area, start_rows(area, warnings)[1], preview).0
}

pub(crate) fn start_preview(area: Rect, warnings: usize, preview: bool, diagram: bool) -> Option<Rect> {
    match beside(area, start_rows(area, warnings)[1], preview) {
        (_, Some(beside)) => Some(beside),
        (top, None) if preview && diagram => Some(top),
        _ => None,
    }
}

pub(crate) fn start_prompt(area: Rect, warnings: usize) -> Rect {
    text_inside(start_rows(area, warnings)[3])
}

pub(crate) fn start_branch(area: Rect, warnings: usize) -> Rect {
    start_rows(area, warnings)[4]
}

pub(crate) fn start_buttons(area: Rect, warnings: usize, labels: &[&str]) -> Vec<Rect> {
    issue_buttons_in(start_rows(area, warnings)[7], labels)
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
    pub(crate) hint: String,
    pub(crate) buttons: Vec<&'static str>,
    pub(crate) preview: Option<Flow>,
    pub(crate) diagram: bool,
}

impl StartFlow {
    fn list_hidden(&self, area: Rect) -> bool {
        let warnings = self.warnings.len();
        self.preview_area(area) == Some(start_rows(area, warnings)[1])
    }

    fn preview_area(&self, area: Rect) -> Option<Rect> {
        start_preview(area, self.warnings.len(), self.preview.is_some(), self.diagram)
    }

    fn list(&self, area: Rect) -> Rect {
        start_list(area, self.warnings.len(), self.preview.is_some())
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
    let warnings = view.warnings.len();
    if let Some(i) = start_buttons(area, warnings, &view.buttons).iter().position(|r| r.contains(pos)) {
        return Some(StartHit::Button(i));
    }
    let prompt = start_prompt(area, warnings);
    if prompt.contains(pos) {
        return Some(StartHit::Prompt { line: usize::from(pos.y - prompt.y), col: usize::from(pos.x - prompt.x) });
    }
    if start_branch(area, warnings).contains(pos) {
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
    let warnings = start.warnings.len();
    let [heading, _, label, prompt, branch, warning_rows, note, _] = start_rows(area, warnings);
    let dim = Style::default().fg(view.muted);
    let heading_text = truncate_right(&start.heading, usize::from(heading.width));
    f.render_widget(Paragraph::new(Span::styled(heading_text, dim.add_modifier(Modifier::BOLD))), heading);
    if let Some(preview) = start.preview_area(area) {
        draw_preview(f, view, preview);
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
    draw_hint(f, view, None, &start.hint, note);
    draw_buttons(f, view, &start_buttons(area, warnings, &start.buttons), &start.buttons, START);
    if let Some(pos) = cursor {
        f.set_cursor_position(pos);
    }
}
