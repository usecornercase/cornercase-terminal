use std::collections::HashMap;
use std::ops::Range;

use ratatui::Frame;
use ratatui::layout::{Alignment, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::View;
use super::dialog::dialog_button_style;
use super::sidebar::NEW_BUTTON;
use super::theme::BRAND_COLOR;
use super::widgets::sidebar_hovered;
use crate::project::split::{Dir, Node, Place};
use crate::term::emulator::Snapshot;
use crate::text::truncate_right;

const MOVE_LABEL: &str = "move here";

const SWAP_LABEL: &str = "swap";

const NO_TAB: &str = "no tab open";

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PaneBand {
    Resuming(String),
    Held(String),
}

fn draw_centered(f: &mut Frame, area: Rect, lines: Vec<Line<'static>>) {
    let height = u16::try_from(lines.len()).unwrap_or(u16::MAX);
    let width = lines.iter().map(Line::width).max().unwrap_or(0);
    if height > area.height || width > usize::from(area.width) {
        return;
    }
    let r = Rect { y: area.y + (area.height - height) / 2, height, ..area };
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), r);
}

pub(super) fn draw_no_tab(f: &mut Frame, muted: Color, pane: Rect) {
    let dim = Style::default().fg(muted);
    draw_centered(
        f,
        pane,
        vec![
            Line::from(Span::styled(NO_TAB, dim.add_modifier(Modifier::BOLD))),
            Line::from(vec![Span::styled("+ Tab", Style::default().fg(Color::Cyan)), Span::styled(NO_TAB_HINT, dim)]),
        ],
    );
}

fn welcome_text(muted: Color) -> Vec<Line<'static>> {
    let dim = Style::default().fg(muted);
    vec![
        Line::from(wordmark()),
        Line::from(Span::styled(TAGLINE, dim)),
        Line::default(),
        Line::from(vec![Span::styled(NEW_BUTTON, Style::default().fg(Color::Cyan)), Span::styled(WELCOME_HINT, dim)]),
    ]
}

pub(super) fn draw_welcome(f: &mut Frame, muted: Color, pane: Rect) {
    let corner = Style::default().fg(BRAND_COLOR);
    let cursor = Style::default().fg(Color::Cyan);
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
    if usize::from(pane.height) >= lines.len() {
        draw_centered(f, pane, lines);
    } else {
        draw_centered(f, pane, welcome_text(muted));
    }
}

pub(super) fn wordmark() -> Vec<Span<'static>> {
    let mark = Style::default().fg(BRAND_COLOR).add_modifier(Modifier::BOLD);
    vec![Span::styled("c", mark), Span::styled("ornercase", Style::default().add_modifier(Modifier::BOLD))]
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
const HELD_HIDE: &str = " hide ";
const HELD_TEXT_MIN: u16 = 6;

fn resume_label(agent: &str) -> String {
    format!(" resuming {agent}… ")
}

pub(crate) fn resume_notice(pane: Rect, agent: &str) -> Option<(Rect, Rect)> {
    let label = u16::try_from(resume_label(agent).chars().count()).unwrap_or(u16::MAX);
    let button = u16::try_from(RESUME_CANCEL.chars().count()).unwrap_or(u16::MAX);
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
    pub(crate) hide: Option<Rect>,
}

fn width_of(text: &str) -> u16 {
    u16::try_from(text.chars().count()).unwrap_or(u16::MAX)
}

pub(crate) fn held_notice(pane: Rect, below: usize) -> Option<HeldBar> {
    let room = match scrollback_badge(pane, below) {
        Some(badge) => badge.x.saturating_sub(pane.x).saturating_sub(1),
        None => pane.width,
    };
    let (send_w, hide_w) = (width_of(HELD_SEND), width_of(HELD_HIDE));
    if pane.height == 0 || room < send_w {
        return None;
    }
    let y = pane.bottom() - 1;
    let hide = (room >= send_w + 1 + hide_w).then(|| Rect::new(pane.x + room - hide_w, y, hide_w, 1));
    let send_x = hide.map_or(pane.x + room - send_w, |hide| hide.x - 1 - send_w);
    let space = send_x.saturating_sub(pane.x).saturating_sub(1);
    let text = Rect::new(pane.x, y, if space >= HELD_TEXT_MIN { space } else { 0 }, 1);
    Some(HeldBar { text, send: Rect::new(send_x, y, send_w, 1), hide })
}

fn draw_resume_notice(f: &mut Frame, view: &View, pane: Rect, agent: &str) {
    let Some((label, button)) = resume_notice(pane, agent) else { return };
    let band = Rect { y: label.y, height: 1, ..pane };
    f.buffer_mut().set_style(band, Style::default().bg(view.surface()));
    let text = Style::default().bg(view.surface()).add_modifier(Modifier::BOLD);
    f.render_widget(Paragraph::new(Span::styled(resume_label(agent), text)), label);
    let style = dialog_button_style(view, button, false);
    f.render_widget(Paragraph::new(Span::styled(RESUME_CANCEL, style)), button);
}

fn draw_held_notice(f: &mut Frame, view: &View, pane: Rect, below: usize, label: &str) {
    let Some(bar) = held_notice(pane, below) else { return };
    let end = bar.hide.unwrap_or(bar.send).right();
    let row = Rect::new(pane.x, bar.send.y, end - pane.x, 1);
    f.buffer_mut().set_style(row, Style::default().bg(view.surface()));
    let room = usize::from(bar.text.width.saturating_sub(2));
    let text = format!(" {}", truncate_right(label, room));
    f.render_widget(Paragraph::new(Span::styled(text, Style::default().fg(view.muted).bg(view.surface()))), bar.text);
    let send = dialog_button_style(view, bar.send, true);
    f.render_widget(Paragraph::new(Span::styled(HELD_SEND, send)), bar.send);
    if let Some(hide) = bar.hide {
        f.render_widget(Paragraph::new(Span::styled(HELD_HIDE, dialog_button_style(view, hide, false))), hide);
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
    for (i, pane) in panes {
        let Some(screen) = tab.screens.get(i) else { continue };
        let active = i == tab.active;
        draw_screen(f, screen, pane, active && view.overlay.is_none(), split && !active && tab.dim_inactive);
        if let Some((row, cols)) = tab.link.as_ref().filter(|_| active) {
            let r = Rect::new(pane.x + cols.start, pane.y + row, cols.end - cols.start, 1).intersection(pane);
            f.buffer_mut().set_style(r, Style::default().fg(Color::Cyan).add_modifier(Modifier::UNDERLINED));
        }
        draw_scrollback_badge(f, view, pane, screen.below);
        match tab.bands.get(i) {
            Some(Some(PaneBand::Resuming(agent))) => draw_resume_notice(f, view, pane, agent),
            Some(Some(PaneBand::Held(label))) => draw_held_notice(f, view, pane, screen.below, label),
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

fn draw_screen(f: &mut Frame, screen: &Snapshot, pane: Rect, show_cursor: bool, dim: bool) {
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

fn draw_dividers(f: &mut Frame, view: &View, tab: &TabView, area: Rect) {
    let dividers = tab.layout.dividers(area);
    let mut cells: HashMap<(u16, u16), (u8, bool)> = HashMap::new();
    for d in &dividers {
        let lit = tab.dragging.as_ref() == Some(&d.path) || (tab.dragging.is_none() && sidebar_hovered(view, d.line));
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
    for d in &dividers {
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
    let buf = f.buffer_mut();
    for ((x, y), (links, lit)) in cells {
        let color = if lit { Color::Cyan } else { view.line() };
        buf[(x, y)].set_symbol(divider_symbol(links)).set_style(Style::default().fg(color));
    }
}
