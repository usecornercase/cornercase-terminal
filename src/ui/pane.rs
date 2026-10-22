use std::collections::HashMap;
use std::ops::Range;

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::View;
use super::accounts_bar::{Row as AccountRow, draw_row};
use super::dialog::dialog_button_style;
use super::sidebar::NEW_BUTTON;
use super::theme::{BRAND_COLOR, status_icon};
use super::widgets::{action_style, button_width, sidebar_hovered};
use super::workspaces::REOPEN_ICON;
use crate::agents::activity::Status;
use crate::project::split::{Dir, Divider, Node, Place};
use crate::term::emulator::Snapshot;
use crate::text::truncate_right;

const MOVE_LABEL: &str = "move here";

const SWAP_LABEL: &str = "swap";

const NO_TAB: &str = "no tab open";

const TAB_BUTTON: &str = "+ Tab";

const NO_TAB_HINT: &str = " opens a shell here";

pub(super) const TAGLINE: &str = "every agent in its own corner";

const WELCOME_HINT: &str = " opens a folder";

const LOGO: [(&str, &str); 6] =
    [("   █████████", ""), ("▄▄▄▀▀▀▀▀▀▀▀▀", ""), ("███", ""), ("███   ", "██"), ("███   ", "██"), ("███   ", "▀▀")];

pub(crate) struct TabView {
    pub(crate) layout: Node<usize>,
    pub(crate) screens: Vec<Snapshot>,
    pub(crate) active: usize,
    pub(crate) dim_inactive: bool,
    pub(crate) dragging: Option<Vec<bool>>,
    pub(crate) link: Option<(u16, Range<u16>)>,
    pub(crate) landing: Option<(Rect, Place)>,
    pub(crate) bands: Vec<Option<PaneBand>>,
    pub(crate) footer: bool,
    pub(crate) accounts: Vec<AccountRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PaneBand {
    Resuming(String),
    Restarts(String),
    Held { text: String, queued: bool },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct NoTab {
    pub(crate) closed: usize,
    pub(crate) removing: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct NoTabButtons {
    pub(crate) reopen: Option<Rect>,
    pub(crate) new_tab: Option<Rect>,
}

fn centered_rows(area: Rect, lines: &[Line]) -> Option<Vec<Rect>> {
    let height = u16::try_from(lines.len()).ok()?;
    let widths: Vec<u16> = lines.iter().map(|line| u16::try_from(line.width()).unwrap_or(u16::MAX)).collect();
    if height > area.height || widths.iter().any(|&w| w > area.width) {
        return None;
    }
    let top = area.y + (area.height - height) / 2;
    Some(widths.into_iter().zip(top..).map(|(w, y)| Rect::new(area.x + area.width / 2 - w / 2, y, w, 1)).collect())
}

fn draw_centered(f: &mut Frame, area: Rect, lines: Vec<Line<'static>>) {
    let Some(rows) = centered_rows(area, &lines) else { return };
    for (line, row) in lines.into_iter().zip(rows) {
        f.render_widget(Paragraph::new(line), row);
    }
}

fn padded(row: Rect, label: &str, pane: Rect) -> Rect {
    Rect::new(row.x.saturating_sub(1), row.y, button_width(label), 1).intersection(pane)
}

fn light(f: &mut Frame, view: &View, button: Option<Rect>) {
    if let Some(button) = button.filter(|r| sidebar_hovered(view, *r)) {
        f.buffer_mut().set_style(button, action_style(true));
    }
}

fn closed_count(closed: usize) -> String {
    if closed == 1 { "1 agent closed".to_string() } else { format!("{closed} agents closed") }
}

fn reopen_label(closed: usize) -> String {
    let what = if closed == 1 { "agent" } else { "agents" };
    format!(" {REOPEN_ICON} reopen {what} ")
}

fn no_tab_lines(muted: Color, shown: NoTab) -> Vec<Line<'static>> {
    let dim = Style::default().fg(muted);
    let bold = dim.add_modifier(Modifier::BOLD);
    let mut lines = if shown.closed == 0 {
        vec![Line::from(Span::styled(NO_TAB, bold))]
    } else {
        let pill = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
        vec![
            Line::from(Span::styled(closed_count(shown.closed), bold)),
            Line::from(Span::styled(reopen_label(shown.closed), pill)),
        ]
    };
    if !shown.removing {
        lines.push(Line::from(vec![Span::styled(TAB_BUTTON, action_style(false)), Span::styled(NO_TAB_HINT, dim)]));
    }
    lines
}

pub(crate) fn no_tab_buttons(pane: Rect, shown: NoTab) -> NoTabButtons {
    let Some(rows) = centered_rows(pane, &no_tab_lines(Color::Reset, shown)) else { return NoTabButtons::default() };
    NoTabButtons {
        reopen: rows.get(1).copied().filter(|_| shown.closed > 0),
        new_tab: rows.last().filter(|_| !shown.removing).map(|row| padded(*row, TAB_BUTTON, pane)),
    }
}

pub(super) fn draw_no_tab(f: &mut Frame, view: &View, pane: Rect) {
    let buttons = no_tab_buttons(pane, view.no_tab);
    draw_centered(f, pane, no_tab_lines(view.muted, view.no_tab));
    light(f, view, buttons.reopen);
    light(f, view, buttons.new_tab);
}

fn welcome_text(muted: Color) -> Vec<Line<'static>> {
    let dim = Style::default().fg(muted);
    vec![
        Line::from(wordmark()),
        Line::from(Span::styled(TAGLINE, dim)),
        Line::default(),
        Line::from(vec![Span::styled(NEW_BUTTON, action_style(false)), Span::styled(WELCOME_HINT, dim)]),
    ]
}

fn welcome_lines(muted: Color, pane: Rect) -> Vec<Line<'static>> {
    let corner = Style::default().fg(BRAND_COLOR);
    let cursor = Style::default();
    let width = LOGO.iter().map(|(c, k)| c.chars().count() + k.chars().count()).max().unwrap_or(0);
    let mut lines: Vec<Line<'static>> = LOGO
        .iter()
        .map(|(c, k)| {
            let pad = width - c.chars().count() - k.chars().count();
            Line::from(vec![Span::styled(*c, corner), Span::styled(*k, cursor), Span::raw(" ".repeat(pad))])
        })
        .collect();
    lines.push(Line::default());
    lines.extend(welcome_text(muted));
    if usize::from(pane.height) >= lines.len() { lines } else { welcome_text(muted) }
}

pub(crate) fn welcome_button(pane: Rect) -> Option<Rect> {
    let rows = centered_rows(pane, &welcome_lines(Color::Reset, pane))?;
    rows.last().map(|row| padded(*row, NEW_BUTTON, pane))
}

pub(super) fn draw_welcome(f: &mut Frame, view: &View, pane: Rect) {
    draw_centered(f, pane, welcome_lines(view.muted, pane));
    light(f, view, welcome_button(pane));
}

pub(super) fn wordmark() -> Vec<Span<'static>> {
    let mark = Style::default().fg(BRAND_COLOR).add_modifier(Modifier::BOLD);
    vec![Span::styled("c", mark), Span::styled("ornercase", Style::default().add_modifier(Modifier::BOLD))]
}

pub(crate) fn split_footer(leaf: Rect, footer: bool) -> (Rect, Option<Rect>) {
    if !footer || leaf.height < 2 {
        return (leaf, None);
    }
    let terminal = Rect { height: leaf.height - 1, ..leaf };
    (terminal, Some(Rect { y: terminal.bottom(), height: 1, ..leaf }))
}

pub(crate) fn scrollback_label(below: usize) -> String {
    format!(" ↓ {below} more ")
}

pub(crate) fn scrollback_badge(pane: Rect, below: usize) -> Option<Rect> {
    if below == 0 || pane.height == 0 {
        return None;
    }
    let width = u16::try_from(scrollback_label(below).chars().count()).unwrap_or(u16::MAX);
    if width.saturating_add(1) > pane.width {
        return None;
    }
    Some(Rect::new(pane.right() - width - 1, pane.bottom() - 1, width, 1))
}

const RESUME_CANCEL: &str = " cancel ";
const HELD_SEND: &str = " send now ";
const HELD_WAIT: &str = " wait turn ";
const HELD_TEXT_MIN: u16 = 6;

fn resume_label(text: &str) -> String {
    format!(" {text}… ")
}

pub(crate) fn resume_notice(pane: Rect, agent: &str) -> Option<(Rect, Rect)> {
    notice(pane, &resume_label(agent))
}

pub(crate) fn restart_notice(pane: Rect, text: &str) -> Option<(Rect, Rect)> {
    notice(pane, &restart_label(text))
}

fn restart_label(text: &str) -> String {
    format!(" {text} ")
}

fn notice(pane: Rect, label: &str) -> Option<(Rect, Rect)> {
    let label = width_of(label);
    let button = width_of(RESUME_CANCEL);
    let width = label.saturating_add(button).saturating_add(1);
    if pane.height == 0 || width > pane.width {
        return None;
    }
    let (x, y) = (pane.x + (pane.width - width) / 2, pane.y + pane.height / 2);
    Some((Rect::new(x, y, label, 1), Rect::new(x + label + 1, y, button, 1)))
}

pub(crate) struct HeldBar {
    pub(crate) text: Rect,
    pub(crate) send: Rect,
    pub(crate) wait: Option<Rect>,
}

fn width_of(text: &str) -> u16 {
    u16::try_from(text.chars().count()).unwrap_or(u16::MAX)
}

pub(crate) fn held_notice(pane: Rect, below: usize, queued: bool) -> Option<HeldBar> {
    let room = match scrollback_badge(pane, below) {
        Some(badge) => badge.x.saturating_sub(pane.x).saturating_sub(1),
        None => pane.width,
    };
    let (send_w, wait_w) = (width_of(HELD_SEND), width_of(HELD_WAIT));
    if pane.height == 0 || room < send_w {
        return None;
    }
    let y = pane.bottom() - 1;
    let fits = !queued && room >= send_w + 1 + wait_w;
    let wait = fits.then(|| Rect::new(pane.x + room - wait_w, y, wait_w, 1));
    let send_x = wait.map_or(pane.x + room - send_w, |wait| wait.x - 1 - send_w);
    let space = send_x.saturating_sub(pane.x).saturating_sub(1);
    let text = Rect::new(pane.x, y, if space >= HELD_TEXT_MIN { space } else { 0 }, 1);
    Some(HeldBar { text, send: Rect::new(send_x, y, send_w, 1), wait })
}

fn draw_notice(f: &mut Frame, view: &View, pane: Rect, text: String) {
    let Some((label, button)) = notice(pane, &text) else { return };
    let band = Rect { y: label.y, height: 1, ..pane };
    f.buffer_mut().set_style(band, Style::default().bg(view.surface()));
    let bold = Style::default().bg(view.surface()).add_modifier(Modifier::BOLD);
    f.render_widget(Paragraph::new(Span::styled(text, bold)), label);
    let style = dialog_button_style(view, button, false);
    f.render_widget(Paragraph::new(Span::styled(RESUME_CANCEL, style)), button);
}

fn draw_held_notice(f: &mut Frame, view: &View, pane: (Rect, usize), label: &str, queued: bool) {
    let Some(bar) = held_notice(pane.0, pane.1, queued) else { return };
    let x = pane.0.x;
    let end = bar.wait.unwrap_or(bar.send).right();
    f.buffer_mut().set_style(Rect::new(x, bar.send.y, end - x, 1), Style::default().bg(view.surface()));
    let muted = Style::default().fg(view.muted).bg(view.surface());
    let mut spans = vec![Span::styled(" ", muted)];
    if queued {
        spans.extend([status_icon(view.muted, Status::Working, view.spin), Span::styled(" ", muted)]);
    }
    let used = spans.iter().map(Span::width).sum::<usize>();
    let room = usize::from(bar.text.width).saturating_sub(used + 1);
    spans.push(Span::styled(truncate_right(label, room), muted));
    f.render_widget(Paragraph::new(Line::from(spans)).style(muted), bar.text);
    let send = dialog_button_style(view, bar.send, true);
    f.render_widget(Paragraph::new(Span::styled(HELD_SEND, send)), bar.send);
    if let Some(wait) = bar.wait {
        f.render_widget(Paragraph::new(Span::styled(HELD_WAIT, dialog_button_style(view, wait, false))), wait);
    }
}

fn draw_scrollback_badge(f: &mut Frame, view: &View, pane: Rect, below: usize) {
    let Some(badge) = scrollback_badge(pane, below) else { return };
    let lit = view.overlay.is_none() && view.hover.is_some_and(|p| badge.contains(p));
    let style = if lit {
        Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(view.muted).bg(view.surface())
    };
    f.render_widget(Paragraph::new(Span::styled(scrollback_label(below), style)), badge);
}

pub(super) fn draw_tab(f: &mut Frame, view: &View, tab: &TabView, area: Rect) {
    let panes = tab.layout.visible(area, tab.active);
    let split = panes.len() > 1;
    for (i, leaf) in panes {
        let Some(screen) = tab.screens.get(i) else { continue };
        let (pane, row) = split_footer(leaf, tab.footer);
        let active = i == tab.active;
        let dim = split && !active && tab.dim_inactive;
        draw_screen(f, screen, pane, active && view.overlay.is_none(), dim);
        if let (Some(row), Some(shown)) = (row, tab.accounts.get(i)) {
            draw_row(f, view, row, shown, dim);
        }
        if let Some((row, cols)) = tab.link.as_ref().filter(|_| active) {
            let r = Rect::new(pane.x + cols.start, pane.y + row, cols.end - cols.start, 1).intersection(pane);
            f.buffer_mut().set_style(r, Style::default().fg(Color::Cyan).add_modifier(Modifier::UNDERLINED));
        }
        draw_scrollback_badge(f, view, pane, screen.below);
        match tab.bands.get(i) {
            Some(Some(PaneBand::Resuming(agent))) => draw_notice(f, view, pane, resume_label(agent)),
            Some(Some(PaneBand::Restarts(text))) => draw_notice(f, view, pane, restart_label(text)),
            Some(Some(PaneBand::Held { text, queued })) => {
                draw_held_notice(f, view, (pane, screen.below), text, *queued);
            }
            _ => {}
        }
    }
    if let Some((r, place)) = tab.landing {
        draw_pane_landing(f, view, r, place);
    }
    draw_dividers(f, view, tab, area);
}

fn draw_pane_landing(f: &mut Frame, view: &View, r: Rect, place: Place) {
    f.buffer_mut().set_style(r, Style::default().bg(view.surface()));
    let label = format!(" {} ", if place == Place::Swap { SWAP_LABEL } else { MOVE_LABEL });
    let width = u16::try_from(label.chars().count()).unwrap_or(u16::MAX);
    if width > r.width || r.height == 0 {
        return;
    }
    let at = Rect::new(r.x + (r.width - width) / 2, r.y + r.height / 2, width, 1);
    let style = Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD);
    f.render_widget(Paragraph::new(Span::styled(label, style)), at);
}

pub(super) fn draw_screen(f: &mut Frame, screen: &Snapshot, pane: Rect, show_cursor: bool, dim: bool) {
    let buf = f.buffer_mut();
    for (y, row) in (pane.y..pane.bottom()).zip(&screen.rows) {
        for (x, cell) in (pane.x..pane.right()).zip(row) {
            let style = if dim { cell.style.add_modifier(Modifier::DIM) } else { cell.style };
            buf[(x, y)].set_symbol(&cell.symbol).set_style(style);
        }
    }
    if let Some(c) = screen.cursor.filter(|_| show_cursor)
        && c.x < pane.width
        && c.y < pane.height
    {
        f.set_cursor_position(Position::new(pane.x + c.x, pane.y + c.y));
    }
}

const UP: u8 = 1;

const DOWN: u8 = 2;

const LEFT: u8 = 4;

const RIGHT: u8 = 8;

fn divider_symbol(links: u8) -> &'static str {
    match links {
        l if l == UP | DOWN | LEFT | RIGHT => "┼",
        l if l == UP | DOWN | RIGHT => "├",
        l if l == UP | DOWN | LEFT => "┤",
        l if l == LEFT | RIGHT | DOWN => "┬",
        l if l == LEFT | RIGHT | UP => "┴",
        l if l & (UP | DOWN) != 0 => "│",
        _ => "─",
    }
}

type DividerCells = HashMap<(u16, u16), (u8, bool)>;

fn draw_dividers(f: &mut Frame, view: &View, tab: &TabView, area: Rect) {
    let dividers = tab.layout.dividers(area);
    let mut cells = DividerCells::new();
    for d in &dividers {
        let lit = tab.dragging.as_ref() == Some(&d.path) || (tab.dragging.is_none() && sidebar_hovered(view, d.line));
        mark_line(&mut cells, d, lit);
    }
    bridge_dividers(&mut cells, &dividers);
    for d in &dividers {
        join_ends(&mut cells, d);
    }
    let buf = f.buffer_mut();
    for ((x, y), (links, lit)) in cells {
        let color = if lit { Color::Cyan } else { view.line() };
        buf[(x, y)].set_symbol(divider_symbol(links)).set_style(Style::default().fg(color));
    }
}

fn mark_line(cells: &mut DividerCells, d: &Divider, lit: bool) {
    let links = match d.dir {
        Dir::Right => UP | DOWN,
        Dir::Down => LEFT | RIGHT,
    };
    for y in d.line.top()..d.line.bottom() {
        for x in d.line.left()..d.line.right() {
            let cell = cells.entry((x, y)).or_default();
            cell.0 |= links;
            cell.1 |= lit;
        }
    }
}

fn bridge_dividers(cells: &mut DividerCells, dividers: &[Divider]) {
    let bridges: Vec<(u16, u16)> = dividers
        .iter()
        .filter(|d| d.dir == Dir::Down)
        .filter_map(|d| {
            let pad = d.line.x.checked_sub(1)?;
            let bar = pad.checked_sub(1)?;
            let joins =
                !cells.contains_key(&(pad, d.line.y)) && cells.get(&(bar, d.line.y)).is_some_and(|c| c.0 & UP != 0);
            joins.then_some((pad, d.line.y))
        })
        .collect();
    for (x, y) in bridges {
        cells.insert((x, y), (LEFT | RIGHT, false));
        if let Some(bar) = cells.get_mut(&(x - 1, y)) {
            bar.0 |= RIGHT;
        }
    }
}

fn join_ends(cells: &mut DividerCells, d: &Divider) {
    let Rect { x, y, .. } = d.line;
    let ends = match d.dir {
        Dir::Right => [(Some(x), y.checked_sub(1), DOWN), (Some(x), Some(d.line.bottom()), UP)],
        Dir::Down => [(x.checked_sub(1), Some(y), RIGHT), (Some(d.line.right()), Some(y), LEFT)],
    };
    for (x, y, link) in ends {
        if let (Some(x), Some(y)) = (x, y)
            && let Some(cell) = cells.get_mut(&(x, y))
        {
            cell.0 |= link;
        }
    }
}
