use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::settings::settings_area;
use super::{
    Note, draw_note, draw_row_button, draw_secondary, draw_submit, draw_waiting, form_inner, item_at, list_item_by,
    pick_background, right_aligned, row_button_rects,
};
use crate::text::truncate_right;
use crate::ui::View;
use crate::ui::widgets::{button_width, hovered, overlay_block, rail};

const LEAD: u16 = 3;

const COMPACT_PITCH: u16 = 3;

const PID_WIDTH: usize = 11;

const EMPTY: &str = "Nothing in this workspace is listening on a port.";

const DONE_LABEL: &str = "done";

const BACK_LABEL: &str = "back";

const STOP_LABEL: &str = "stop";

pub(crate) struct Ports {
    pub(crate) title: String,
    pub(crate) rows: Vec<PortRow>,
    pub(crate) selected: Option<usize>,
    pub(crate) scroll: usize,
    pub(crate) ticked: usize,
    pub(crate) note: Option<Note>,
    pub(crate) empty: bool,
    pub(crate) confirm: Option<ConfirmBody>,
    pub(crate) can_open: bool,
    pub(crate) compact: bool,
}

pub(crate) struct PortRow {
    pub(crate) ticked: bool,
    pub(crate) url: String,
    pub(crate) command: String,
    pub(crate) pid: i32,
    pub(crate) place: String,
    pub(crate) go: bool,
}

pub(crate) struct ConfirmBody {
    pub(crate) lines: Vec<Line<'static>>,
    pub(crate) scroll: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PortsHit {
    Tick(usize),
    Row(usize),
    Copy(usize),
    Open(usize),
    Go(usize),
    StopRow(usize),
    Stop,
    Done,
    Back,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowButton {
    Copy,
    Open,
    Go,
    Stop,
}

impl RowButton {
    fn label(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Open => "open",
            Self::Go => "go",
            Self::Stop => STOP_LABEL,
        }
    }

    fn hit(self, row: usize) -> PortsHit {
        match self {
            Self::Copy => PortsHit::Copy(row),
            Self::Open => PortsHit::Open(row),
            Self::Go => PortsHit::Go(row),
            Self::Stop => PortsHit::StopRow(row),
        }
    }
}

pub(crate) fn ports_area(screen: Rect, compact: bool) -> Rect {
    if compact { screen } else { settings_area(screen) }
}

pub(crate) fn ports_room(screen: Rect, compact: bool) -> (usize, Rect) {
    let [list, ..] = rows(ports_area(screen, compact));
    (usize::from(list.height / pitch(compact)), list)
}

fn rows(area: Rect) -> [Rect; 3] {
    let inner = form_inner(area);
    let line = |up: u16| Rect { y: inner.bottom().saturating_sub(up), height: 1.min(inner.height), ..inner };
    [Rect { height: inner.height.saturating_sub(2), ..inner }, line(2), line(1)]
}

fn pitch(compact: bool) -> u16 {
    if compact { COMPACT_PITCH } else { 1 }
}

fn row_buttons(view: &Ports, row: &PortRow, rect: Rect) -> Vec<(RowButton, Rect)> {
    let mut buttons = vec![RowButton::Copy];
    buttons.extend(view.can_open.then_some(RowButton::Open));
    buttons.extend(row.go.then_some(RowButton::Go));
    buttons.push(RowButton::Stop);
    let labels: Vec<&str> = buttons.iter().map(|b| b.label()).collect();
    buttons.into_iter().zip(row_button_rects(rect, &labels)).collect()
}

fn stop_label(view: &Ports) -> String {
    match (view.confirm.is_some(), view.ticked) {
        (true, _) => STOP_LABEL.to_string(),
        (false, 0) => format!("{STOP_LABEL} all"),
        (false, n) => format!("{STOP_LABEL} {n}"),
    }
}

fn buttons(area: Rect, view: &Ports) -> (Rect, Rect) {
    let stop = stop_label(view);
    let row = rows(area)[2];
    if view.confirm.is_some() {
        let rects = right_aligned(row, &[BACK_LABEL, stop.as_str()], button_width, 1);
        (rects[1], rects[0])
    } else {
        let rects = right_aligned(row, &[stop.as_str(), DONE_LABEL], button_width, 1);
        (rects[0], rects[1])
    }
}

fn can_stop(view: &Ports) -> bool {
    view.confirm.is_some() || !view.rows.is_empty()
}

pub(crate) fn ports_hit(screen: Rect, view: &Ports, pos: Position) -> Option<PortsHit> {
    let area = ports_area(screen, view.compact);
    let (stop_button, other) = buttons(area, view);
    if can_stop(view) && stop_button.contains(pos) {
        return Some(PortsHit::Stop);
    }
    if other.contains(pos) {
        return Some(if view.confirm.is_some() { PortsHit::Back } else { PortsHit::Done });
    }
    if view.confirm.is_some() {
        return None;
    }
    let list = rows(area)[0];
    let (i, rect) = item_at(list, (view.rows.len(), view.scroll), pitch(view.compact), pos)?;
    let row = view.rows.get(i)?;
    if let Some((button, _)) = row_buttons(view, row, rect).into_iter().find(|(_, r)| r.contains(pos)) {
        return Some(button.hit(i));
    }
    Some(if pos.x < list.x + LEAD { PortsHit::Tick(i) } else { PortsHit::Row(i) })
}

pub(in crate::ui) fn draw_ports(f: &mut Frame, view: &View, ports: &Ports) {
    let area = ports_area(f.area(), ports.compact);
    f.render_widget(Clear, area);
    f.render_widget(overlay_block(view.muted, &ports.title), area);
    let [list, note, _] = rows(area);
    match &ports.confirm {
        Some(confirm) => {
            let scroll = u16::try_from(confirm.scroll).unwrap_or(u16::MAX);
            let body = Paragraph::new(confirm.lines.clone()).scroll((scroll, 0));
            f.render_widget(body, list);
        }
        None => draw_list(f, view, ports, list),
    }
    draw_note(f, view.muted, ports.note.as_ref(), note);
    let (stop, other) = buttons(area, ports);
    if can_stop(ports) {
        draw_submit(f, view, stop, &stop_label(ports));
    } else {
        draw_waiting(f, view, stop, &stop_label(ports));
    }
    draw_secondary(f, view, other, if ports.confirm.is_some() { BACK_LABEL } else { DONE_LABEL });
}

fn draw_list(f: &mut Frame, view: &View, ports: &Ports, list: Rect) {
    if ports.empty {
        f.render_widget(Paragraph::new(Span::styled(format!(" {EMPTY}"), Style::default().fg(view.muted))), list);
    }
    let step = pitch(ports.compact);
    let longest = |text: fn(&PortRow) -> &str| ports.rows.iter().map(|row| text(row).chars().count()).max();
    let widths = (longest(|row| &row.url).unwrap_or(0), longest(|row| &row.command).unwrap_or(0));
    for (i, row) in ports.rows.iter().enumerate() {
        let rect = list_item_by(list, ports.rows.len(), ports.scroll, i, step);
        if !rect.is_empty() {
            draw_row(f, view, ports, (row, ports.selected == Some(i)), (rect, widths));
        }
    }
}

fn draw_row(
    f: &mut Frame,
    view: &View,
    ports: &Ports,
    (row, selected): (&PortRow, bool),
    (rect, (url, longest)): (Rect, (usize, usize)),
) {
    let bg = pick_background(view, selected, rect);
    let dim = bg.fg(view.muted);
    let base = if selected { bg.add_modifier(Modifier::BOLD) } else { bg };
    let tick =
        if row.ticked { Span::styled("✓", bg.fg(Color::Green)) } else { Span::styled("○", bg.fg(view.soft)) };
    let lead = |first: bool| {
        let mark = if first { tick.clone() } else { Span::styled(" ", bg) };
        vec![rail(selected, bg), mark, Span::styled(" ", bg)]
    };
    let room = usize::from(rect.width.saturating_sub(LEAD + 1));
    let pid = format!("pid {:<w$}", row.pid, w = PID_WIDTH - 4);
    let lines = if ports.compact {
        let mut first = lead(true);
        first.push(Span::styled(truncate_right(&row.url, room), base));
        let mut second = lead(false);
        second.push(Span::styled(truncate_right(&format!("{}  {pid}", row.command), room), dim));
        let mut third = lead(false);
        third.push(Span::styled(truncate_right(&row.place, room), dim));
        vec![Line::from(first), Line::from(second), Line::from(third)]
    } else {
        let mut line = lead(true);
        line.push(Span::styled(format!("{:<url$}  ", row.url), base));
        let command = longest.min(room.saturating_sub(url + 2 + PID_WIDTH + 2) / 2);
        line.push(Span::styled(format!("{:<command$}  ", truncate_right(&row.command, command)), dim));
        line.push(Span::styled(format!("{pid}  "), dim));
        line.push(Span::styled(row.place.clone(), dim));
        vec![Line::from(line)]
    };
    f.render_widget(Paragraph::new(lines).style(bg), rect);
    if ports.compact || selected || hovered(view, rect) {
        draw_row_buttons(f, view, ports, (row, rect), bg);
    }
}

fn draw_row_buttons(f: &mut Frame, view: &View, ports: &Ports, (row, rect): (&PortRow, Rect), bg: Style) {
    for (button, r) in row_buttons(ports, row, rect) {
        draw_row_button(f, view, r, button.label(), bg);
    }
}
