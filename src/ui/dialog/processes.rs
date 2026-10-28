use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::ports::ConfirmBody;
use super::settings::settings_area;
use super::{
    Note, draw_input, draw_note, draw_row_button, draw_secondary, draw_submit, draw_tabs, form_inner, item_at,
    list_item_by, pick_background, right_aligned, row_button_rects, tabs_in,
};
use crate::processes::dialog::Sort;
use crate::text::{memory_size, truncate_right};
use crate::ui::View;
use crate::ui::widgets::{button_width, hovered, overlay_block, rail};

const LEAD: u16 = 3;

const COMPACT_PITCH: u16 = 3;

const CPU_WIDTH: usize = 7;

const MEMORY_WIDTH: usize = 8;

const PROGRAM_WIDTH: usize = 24;

const INDENT: usize = 2;

const MAX_DEPTH: usize = 4;

const GO_LABEL: &str = "go";

const CLOSE_LABEL: &str = "close";

const STOP_LABEL: &str = "stop";

const DONE_LABEL: &str = "done";

const BACK_LABEL: &str = "back";

pub(crate) struct Processes {
    pub(crate) filter: String,
    pub(crate) sort: usize,
    pub(crate) totals: String,
    pub(crate) lines: Vec<ProcessLine>,
    pub(crate) selected: Option<usize>,
    pub(crate) scroll: usize,
    pub(crate) empty: Option<&'static str>,
    pub(crate) note: Option<Note>,
    pub(crate) confirm: Option<ConfirmBody>,
    pub(crate) compact: bool,
}

pub(crate) struct ProcessLine {
    pub(crate) depth: Option<usize>,
    pub(crate) open: bool,
    pub(crate) text: String,
    pub(crate) program: String,
    pub(crate) cpu: f64,
    pub(crate) memory: u64,
    pub(crate) pane: bool,
    pub(crate) stop: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProcessesHit {
    Filter,
    Sort(usize),
    Row(usize),
    Go(usize),
    Close(usize),
    StopRow(usize),
    Stop,
    Done,
    Back,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowButton {
    Go,
    Close,
    Stop,
}

impl RowButton {
    fn label(self) -> &'static str {
        match self {
            Self::Go => GO_LABEL,
            Self::Close => CLOSE_LABEL,
            Self::Stop => STOP_LABEL,
        }
    }

    fn hit(self, row: usize) -> ProcessesHit {
        match self {
            Self::Go => ProcessesHit::Go(row),
            Self::Close => ProcessesHit::Close(row),
            Self::Stop => ProcessesHit::StopRow(row),
        }
    }
}

pub(crate) fn processes_area(screen: Rect, compact: bool) -> Rect {
    if compact { screen } else { settings_area(screen) }
}

pub(crate) fn processes_room(screen: Rect, compact: bool) -> (usize, Rect) {
    let list = rows(processes_area(screen, compact))[2];
    (usize::from(list.height / pitch(compact)), list)
}

fn rows(area: Rect) -> [Rect; 5] {
    let [filter, totals, list, note, buttons] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(form_inner(area));
    [filter, totals, list, note, buttons]
}

fn pitch(compact: bool) -> u16 {
    if compact { COMPACT_PITCH } else { 1 }
}

fn sort_names() -> Vec<&'static str> {
    Sort::ALL.iter().map(|sort| sort.label()).collect()
}

fn sort_rects(area: Rect) -> Vec<Rect> {
    let names = sort_names();
    let filter = rows(area)[0];
    let width: u16 = names.iter().map(|name| button_width(name) + 1).sum();
    let tabs =
        Rect { x: filter.right().saturating_sub(width.saturating_sub(1)), width: width.saturating_sub(1), ..filter };
    tabs_in(tabs.intersection(filter), &names)
}

fn row_buttons(line: &ProcessLine, rect: Rect) -> Vec<(RowButton, Rect)> {
    let mut buttons = Vec::new();
    if line.pane {
        buttons.extend([RowButton::Go, RowButton::Close]);
    }
    buttons.extend(line.stop.then_some(RowButton::Stop));
    let labels: Vec<&str> = buttons.iter().map(|button| button.label()).collect();
    buttons.into_iter().zip(row_button_rects(rect, &labels)).collect()
}

fn buttons(area: Rect, view: &Processes) -> (Option<Rect>, Rect) {
    let row = rows(area)[4];
    if view.confirm.is_some() {
        let rects = right_aligned(row, &[BACK_LABEL, STOP_LABEL], button_width, 1);
        (Some(rects[1]), rects[0])
    } else {
        (None, right_aligned(row, &[DONE_LABEL], button_width, 1)[0])
    }
}

pub(crate) fn processes_hit(screen: Rect, view: &Processes, pos: Position) -> Option<ProcessesHit> {
    let area = processes_area(screen, view.compact);
    let (stop_button, other) = buttons(area, view);
    if stop_button.is_some_and(|stop| stop.contains(pos)) {
        return Some(ProcessesHit::Stop);
    }
    if other.contains(pos) {
        return Some(if view.confirm.is_some() { ProcessesHit::Back } else { ProcessesHit::Done });
    }
    if view.confirm.is_some() {
        return None;
    }
    if let Some(at) = sort_rects(area).iter().position(|rect| rect.contains(pos)) {
        return Some(ProcessesHit::Sort(at));
    }
    let [filter, _, list, ..] = rows(area);
    if filter.contains(pos) {
        return Some(ProcessesHit::Filter);
    }
    let (i, rect) = item_at(list, (view.lines.len(), view.scroll), pitch(view.compact), pos)?;
    let line = view.lines.get(i)?;
    let button = row_buttons(line, rect).into_iter().find(|(_, r)| r.contains(pos));
    Some(button.map_or(ProcessesHit::Row(i), |(button, _)| button.hit(i)))
}

pub(in crate::ui) fn draw_processes(f: &mut Frame, view: &View, processes: &Processes) {
    let area = processes_area(f.area(), processes.compact);
    f.render_widget(Clear, area);
    f.render_widget(overlay_block(view.muted, "Processes"), area);
    let [filter, totals, list, note, _] = rows(area);
    let cursor = draw_input(f, view.muted, filter, "filter", &processes.filter);
    draw_sort(f, view, area, processes.sort);
    let dim = Style::default().fg(view.muted);
    f.render_widget(Paragraph::new(Span::styled(format!(" {}", processes.totals), dim)), totals);
    match &processes.confirm {
        Some(confirm) => {
            let scroll = u16::try_from(confirm.scroll).unwrap_or(u16::MAX);
            f.render_widget(Paragraph::new(confirm.lines.clone()).scroll((scroll, 0)), list);
        }
        None => draw_list(f, view, processes, list),
    }
    draw_note(f, view.muted, processes.note.as_ref(), note);
    let (stop, other) = buttons(area, processes);
    if let Some(stop) = stop {
        draw_submit(f, view, stop, STOP_LABEL);
    }
    draw_secondary(f, view, other, if processes.confirm.is_some() { BACK_LABEL } else { DONE_LABEL });
    if processes.confirm.is_none()
        && let Some(pos) = cursor
    {
        f.set_cursor_position(pos);
    }
}

fn draw_sort(f: &mut Frame, view: &View, area: Rect, active: usize) {
    draw_tabs(f, view, &sort_rects(area), &sort_names(), active);
}

fn draw_list(f: &mut Frame, view: &View, processes: &Processes, list: Rect) {
    if let Some(text) = processes.empty {
        f.render_widget(Paragraph::new(Span::styled(format!(" {text}"), Style::default().fg(view.muted))), list);
    }
    let step = pitch(processes.compact);
    let count = processes.lines.len();
    for (i, line) in processes.lines.iter().enumerate() {
        let rect = list_item_by(list, count, processes.scroll, i, step);
        if !rect.is_empty() {
            draw_line(f, view, processes, (line, processes.selected == Some(i)), rect);
        }
    }
}

fn figures(line: &ProcessLine) -> (String, String) {
    (format!("{:.1}%", line.cpu), memory_size(line.memory))
}

fn lead(line: &ProcessLine, bg: Style, selected: bool, first: bool, soft: Color) -> Vec<Span<'static>> {
    let mark = match (line.depth, line.open, first) {
        (None, true, true) => Span::styled("▾", bg.fg(soft)),
        (None, false, true) => Span::styled("▸", bg.fg(soft)),
        _ => Span::styled(" ", bg),
    };
    vec![rail(selected, bg), mark, Span::styled(" ", bg)]
}

fn draw_line(f: &mut Frame, view: &View, processes: &Processes, (line, selected): (&ProcessLine, bool), rect: Rect) {
    let bg = pick_background(view, selected, rect);
    let dim = bg.fg(view.muted);
    let base = match (line.depth, selected) {
        (None, true) => bg.add_modifier(Modifier::BOLD),
        (None, false) => bg,
        (Some(_), _) => dim,
    };
    let indent = " ".repeat(line.depth.map_or(0, |depth| (depth.min(MAX_DEPTH) + 1) * INDENT));
    let room = usize::from(rect.width.saturating_sub(LEAD + 1));
    let (cpu, memory) = figures(line);
    let lines = if processes.compact {
        let text = truncate_right(&format!("{indent}{}", line.text), room);
        let detail = format!("{indent}{} · {cpu} · {memory}", line.program);
        vec![
            Line::from([lead(line, bg, selected, true, view.soft), vec![Span::styled(text, base)]].concat()),
            Line::from(
                [lead(line, bg, selected, false, view.soft), vec![Span::styled(truncate_right(&detail, room), dim)]]
                    .concat(),
            ),
            Line::from(lead(line, bg, selected, false, view.soft)),
        ]
    } else {
        let fixed = CPU_WIDTH + MEMORY_WIDTH + 6;
        let program = PROGRAM_WIDTH.min(room.saturating_sub(fixed) / 3);
        let text = room.saturating_sub(fixed + program);
        let shown = truncate_right(&format!("{indent}{}", line.text), text);
        let mut spans = lead(line, bg, selected, true, view.soft);
        spans.push(Span::styled(format!("{shown:<text$}  "), base));
        spans.push(Span::styled(format!("{:<program$}  ", truncate_right(&line.program, program)), dim));
        spans.push(Span::styled(format!("{cpu:>CPU_WIDTH$}  "), base));
        spans.push(Span::styled(format!("{memory:>MEMORY_WIDTH$}"), base));
        vec![Line::from(spans)]
    };
    f.render_widget(Paragraph::new(lines).style(bg), rect);
    if processes.compact || selected || hovered(view, rect) {
        for (button, r) in row_buttons(line, rect) {
            draw_row_button(f, view, r, button.label(), bg);
        }
    }
}
