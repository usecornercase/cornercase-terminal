use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::flows::{
    MAX_WARNINGS, Previewed, beside, draw_buttons, draw_hint, draw_preview, draw_text, draw_warnings, flows_area,
    flows_wide, text_inside,
};
use super::issues::issue_buttons_in;
use super::{INPUT_PROMPT, Note, draw_input, form_inner, pick_background};
use crate::editor::Editor;
use crate::flows::Flow;
use crate::flows::start::{Focus, Item, START};
use crate::list::Combo;
use crate::text::truncate_right;
use crate::ui::View;
use crate::ui::combo;
use crate::ui::widgets::overlay_block;

const HEADING: &str = "Flow templates";
const ACCOUNT_LABEL: usize = 16;
const ACCOUNT_FIELD: u16 = 28;
const PLACEHOLDER: &str = "what the team should do: a feature, a bug, an issue's text…";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Shape {
    pub(crate) warnings: usize,
    pub(crate) preview: bool,
    pub(crate) diagram: bool,
    pub(crate) lines: usize,
    pub(crate) accounts: usize,
    pub(crate) workspace: bool,
}

impl Shape {
    pub(crate) fn new(items: usize, warnings: usize, preview: bool, diagram: bool) -> Self {
        Self { warnings, preview, diagram, lines: 1 + 2 * items, accounts: 0, workspace: false }
    }

    pub(crate) fn with_accounts(self, accounts: usize) -> Self {
        Self { accounts, ..self }
    }

    pub(crate) fn with_workspace(self, workspace: bool) -> Self {
        Self { workspace, ..self }
    }
}

fn start_rows(area: Rect, shape: Shape) -> [Rect; 10] {
    let shown = u16::try_from(shape.warnings.min(MAX_WARNINGS)).unwrap_or(0);
    let [heading, _, body, _, branch, workspace, accounts, warnings, note, buttons] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(u16::from(shape.workspace)),
        Constraint::Length(u16::try_from(shape.accounts).unwrap_or(u16::MAX)),
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
    [heading, top, label, prompt, branch, workspace, accounts, warnings, note, buttons]
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

pub(crate) fn start_workspace(area: Rect, shape: Shape) -> Rect {
    start_rows(area, shape)[5]
}

pub(crate) fn start_buttons(area: Rect, shape: Shape, labels: &[&str]) -> Vec<Rect> {
    issue_buttons_in(start_rows(area, shape)[9], labels)
}

fn account_row(area: Rect, shape: Shape, n: usize) -> Rect {
    let rows = start_rows(area, shape)[6];
    let y = rows.y.saturating_add(u16::try_from(n).unwrap_or(u16::MAX));
    Rect { y, height: 1, ..rows }.intersection(rows)
}

pub(crate) fn start_account(area: Rect, shape: Shape, n: usize) -> Rect {
    let row = account_row(area, shape, n);
    let skip = u16::try_from(INPUT_PROMPT.chars().count() + ACCOUNT_LABEL).unwrap_or(u16::MAX);
    let x = row.x.saturating_add(skip).min(row.right());
    Rect { x, width: ACCOUNT_FIELD.min(row.right() - x), ..row }
}

pub(crate) fn start_account_list(area: Rect, shape: Shape, n: usize, combo: &Combo) -> Option<Rect> {
    combo.is_open().then(|| combo::list(start_account(area, shape, n), combo, flows_area(area)))
}

#[derive(Debug, Clone)]
pub(crate) struct AccountField {
    pub(crate) kind: String,
    pub(crate) combo: Combo,
}

pub(crate) struct StartFlow {
    pub(crate) title: String,
    pub(crate) heading: String,
    pub(crate) items: Vec<Item>,
    pub(crate) cursor: usize,
    pub(crate) preview_scroll: u16,
    pub(crate) prompt: Editor,
    pub(crate) branch: String,
    pub(crate) workspace: Option<String>,
    pub(crate) focus: Focus,
    pub(crate) warnings: Vec<String>,
    pub(crate) note: Option<Note>,
    pub(crate) hint: String,
    pub(crate) buttons: Vec<&'static str>,
    pub(crate) preview: Option<Flow>,
    pub(crate) diagram: bool,
    pub(crate) accounts: Vec<AccountField>,
    pub(crate) focused_account: Option<usize>,
}

impl StartFlow {
    pub(crate) fn shape(&self) -> Shape {
        Shape::new(self.items.len(), self.warnings.len(), self.preview.is_some(), self.diagram)
            .with_accounts(self.accounts.len())
            .with_workspace(self.workspace.is_some())
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
    Workspace,
    Account(usize),
    Choose(usize),
    Close,
    Button(usize),
}

pub(crate) fn start_hit(area: Rect, view: &StartFlow, pos: Position) -> Option<StartHit> {
    let shape = view.shape();
    for (n, field) in view.accounts.iter().enumerate() {
        if let Some(list) = start_account_list(area, shape, n, &field.combo) {
            return Some(combo::hit(list, &field.combo, pos).map_or(StartHit::Close, StartHit::Choose));
        }
    }
    if let Some(n) = (0..view.accounts.len()).find(|n| start_account(area, shape, *n).contains(pos)) {
        return Some(StartHit::Account(n));
    }
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
    if shape.workspace && start_workspace(area, shape).contains(pos) {
        return Some(StartHit::Workspace);
    }
    if view.list_hidden(area) {
        return None;
    }
    let items = items_area(view.list(area));
    if !items.contains(pos) {
        return None;
    }
    let i = first_item(items, view.items.len(), view.cursor) + usize::from((pos.y - items.y) / 2);
    (i < view.items.len()).then_some(StartHit::Row(i))
}

fn items_area(list: Rect) -> Rect {
    Rect { y: list.y.saturating_add(1), height: list.height.saturating_sub(1), ..list }
}

fn first_item(items: Rect, count: usize, cursor: usize) -> usize {
    let shown = usize::from(items.height / 2).max(1);
    (cursor + 1).saturating_sub(shown).min(count.saturating_sub(shown))
}

fn draw_items(f: &mut Frame, view: &View, list: Rect, start: &StartFlow) {
    let dim = Style::default().fg(view.muted);
    f.render_widget(
        Paragraph::new(Span::styled(HEADING, dim.add_modifier(Modifier::BOLD))),
        Rect { height: 1, ..list },
    );
    let items = items_area(list);
    let first = first_item(items, start.items.len(), start.cursor);
    for (n, item) in start.items.iter().enumerate().skip(first) {
        let y = items.y.saturating_add(u16::try_from((n - first) * 2).unwrap_or(u16::MAX));
        if y.saturating_add(1) >= items.bottom() {
            break;
        }
        let row = Rect { y, height: 2, ..items };
        let selected = n == start.cursor;
        let bg = pick_background(view, selected, row);
        f.buffer_mut().set_style(row, bg);
        let rail = Span::styled(if selected { "▌ " } else { "  " }, bg.fg(Color::Cyan));
        let width = usize::from(row.width);
        let agents = format!(" {} ", item.agents);
        let tag = item.tag.as_ref().map(|tag| format!("  {tag}")).unwrap_or_default();
        let room = width.saturating_sub(2 + agents.chars().count() + tag.chars().count());
        let name = truncate_right(&item.name, room);
        let gap = width.saturating_sub(2 + name.chars().count() + tag.chars().count() + agents.chars().count());
        let first = Line::from(vec![
            rail.clone(),
            Span::styled(name, bg.add_modifier(Modifier::BOLD)),
            Span::styled(tag, bg.fg(view.muted)),
            Span::styled(" ".repeat(gap), bg),
            Span::styled(agents, bg.fg(view.muted)),
        ]);
        f.render_widget(Paragraph::new(first), Rect { height: 1, ..row });
        let style = if item.red { bg.fg(Color::Red) } else { bg.fg(view.muted) };
        let line = truncate_right(&item.line, width.saturating_sub(4));
        let second = Line::from(vec![rail, Span::styled(format!("  {line}"), style)]);
        f.render_widget(Paragraph::new(second), Rect { y: y + 1, height: 1, ..row });
    }
}

fn draw_start_prompt(f: &mut Frame, view: &View, start: &StartFlow, area: Rect) -> Option<Position> {
    let shape = start.shape();
    let (label, prompt) = (prompt_label(area, shape), prompt_area(area, shape));
    let dim = Style::default().fg(view.muted);
    let lit = if start.focus == Focus::Prompt { Style::default().fg(Color::Cyan) } else { dim };
    f.render_widget(Paragraph::new(Span::styled("prompt", lit)), label);
    let cursor = draw_text(f, view, prompt, &start.prompt).filter(|_| start.focus == Focus::Prompt);
    if start.prompt.is_empty() {
        let at = text_inside(prompt);
        let placeholder = truncate_right(PLACEHOLDER, usize::from(at.width));
        f.render_widget(Paragraph::new(Span::styled(placeholder, dim.bg(view.surface()))), Rect { height: 1, ..at });
    }
    cursor
}

fn draw_start_accounts(f: &mut Frame, view: &View, start: &StartFlow, area: Rect) -> bool {
    let shape = start.shape();
    let dim = Style::default().fg(view.muted);
    for (n, field) in start.accounts.iter().enumerate() {
        let focused = start.focused_account == Some(n);
        let name = format!("{:<ACCOUNT_LABEL$}", format!("{} account:", field.kind));
        let lit = if focused { Style::default().fg(Color::Cyan) } else { dim };
        let label =
            Line::from(vec![Span::styled(INPUT_PROMPT, Style::default().fg(Color::Cyan)), Span::styled(name, lit)]);
        f.render_widget(Paragraph::new(label), account_row(area, shape, n));
        let at = start_account(area, shape, n);
        combo::draw_field(f, view, at, combo::Field { combo: &field.combo, placeholder: "", focused });
    }
    start.focused_account.is_some_and(|n| n < start.accounts.len())
}

fn draw_start_names(f: &mut Frame, view: &View, start: &StartFlow, [branch, workspace]: [Rect; 2]) -> Option<Position> {
    let label = if start.workspace.is_some() { "branch:   " } else { "branch:" };
    let mut cursor = draw_input(f, view.muted, branch, label, &start.branch).filter(|_| start.focus == Focus::Branch);
    if let Some(name) = &start.workspace {
        let typed = draw_input(f, view.muted, workspace, "workspace:", name);
        if start.focus == Focus::Workspace {
            cursor = typed;
        }
    }
    cursor
}

pub(in crate::ui) fn draw_start_flow(f: &mut Frame, view: &View, start: &StartFlow) {
    let area = f.area();
    let r = flows_area(area);
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, &start.title), r);
    let shape = start.shape();
    let [heading, _, _, _, branch, workspace, _, warning_rows, note, _] = start_rows(area, shape);
    let dim = Style::default().fg(view.muted);
    let heading_text = truncate_right(&start.heading, usize::from(heading.width));
    f.render_widget(Paragraph::new(Span::styled(heading_text, dim.add_modifier(Modifier::BOLD))), heading);
    if let Some(preview) = start.preview_area(area) {
        draw_preview(
            f,
            view,
            preview,
            Previewed { flow: start.preview.as_ref(), scroll: start.preview_scroll, focus: None },
        );
    }
    if !start.list_hidden(area) {
        draw_items(f, view, start.list(area), start);
    }
    let mut cursor = draw_start_prompt(f, view, start, area);
    cursor = draw_start_names(f, view, start, [branch, workspace]).or(cursor);
    draw_warnings(f, &start.warnings, warning_rows);
    draw_hint(f, view, start.note.as_ref(), &start.hint, note);
    draw_buttons(f, view, &start_buttons(area, shape, &start.buttons), &start.buttons, START);
    if draw_start_accounts(f, view, start, area) {
        cursor = None;
    }
    for (n, field) in start.accounts.iter().enumerate() {
        if let Some(list) = start_account_list(area, shape, n, &field.combo) {
            combo::draw_list(f, view, list, &field.combo);
        }
    }
    if let Some(pos) = cursor {
        f.set_cursor_position(pos);
    }
}
