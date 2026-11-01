use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use super::View;
use super::dialog::search::mode_button;
use super::files;
use super::icons::Glyphs;
use super::layout::{Areas, Border, COMPACT_BUTTON_WIDTH, changes_button};
use super::pane::wordmark;
use super::theme::status_icon;
use super::widgets::{
    button_style, button_width, centered, draw_band, draw_button, draw_separator, middle, sidebar_hovered,
};
use crate::text::{CRUMB_SEPARATOR, truncate_left, truncate_right};

#[cfg(test)]
pub(super) const SEARCH_ICON: &str = " ⌕ ";

pub(super) const SEARCH_PLACEHOLDER: &str = "search projects, workspaces, tabs";

const NAMES_PLACEHOLDER: &str = "projects, workspaces, tabs";

const SESSIONS_PLACEHOLDER: &str = "what was said in agents and panes";

pub(super) const BACK_LABEL: &str = "‹ Projects";

pub(super) const ISSUES_LABEL: &str = "Issues";

pub(super) const USAGE_LABEL: &str = "Usage";

pub(super) const HELP_LABEL: &str = "Help";

pub const MORE_LABEL: &str = "More ▴";

pub(crate) const WORKTREES_LABEL: &str = "Worktrees";

pub(crate) const TODO_LABEL: &str = "TODO";

const TODO_DOT: &str = "●";

const REMOTE_DOT: &str = "●";

pub(crate) const PLUGINS_MORE: &str = "⋯";

pub(crate) struct ChangesButton {
    pub(crate) label: String,
    pub(crate) files: usize,
    pub(crate) open: bool,
}

pub(super) fn draw_footer(f: &mut Frame, view: &View, areas: &Areas) {
    draw_separator(f, view.line(), areas.separator);
    draw_separator(f, view.line(), areas.more_separator);
    let help = help_button(areas.more);
    if !help.is_empty() {
        let style = button_style(view, help, Style::default().fg(view.muted), Color::Cyan);
        draw_button(f, help, "", HELP_LABEL, style);
    }
    draw_more_button(f, view, more_button(areas.more));
    if !areas.worktrees.is_empty() {
        let style = button_style(view, areas.worktrees, Style::default().fg(view.muted), Color::Cyan);
        draw_button(f, areas.worktrees, " ", WORKTREES_LABEL, style);
    }
}

pub(crate) fn help_button(more: Rect) -> Rect {
    let width = button_width(HELP_LABEL);
    if more.width < button_width(MORE_LABEL) + width + 2 {
        return Rect::default();
    }
    Rect { x: more.right() - width - 1, width, ..more }
}

pub(crate) fn more_button(more: Rect) -> Rect {
    let help = help_button(more);
    if help.is_empty() { more } else { Rect { width: help.x - more.x, ..more } }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PluginRow {
    pub(crate) buttons: Vec<Rect>,
    pub(crate) more: Rect,
}

pub(crate) fn plugin_buttons<'a>(row: Rect, titles: impl IntoIterator<Item = &'a str>) -> PluginRow {
    let widths: Vec<u16> = titles.into_iter().map(button_width).collect();
    let start = row.x.saturating_add(1).min(row.right());
    let room = row.right().saturating_sub(1).saturating_sub(start);
    let total = widths.iter().fold(0u16, |sum, &w| sum.saturating_add(w));
    let overflows = total > room && widths.len() > 1;
    let room = if overflows { room.saturating_sub(button_width(PLUGINS_MORE)) } else { room };
    let buttons = fitting_buttons(Rect { x: start, width: room, ..row }, &widths);
    let more = if overflows {
        let x = buttons.last().map_or(start, |b| b.right());
        Rect { x, width: button_width(PLUGINS_MORE), ..row }.intersection(row)
    } else {
        Rect::default()
    };
    PluginRow { buttons, more }
}

fn fitting_buttons(room: Rect, widths: &[u16]) -> Vec<Rect> {
    let mut x = room.x;
    let mut buttons = Vec::new();
    for (i, &width) in widths.iter().enumerate() {
        let width = if i == 0 { width.min(room.width) } else { width };
        if width == 0 || x.saturating_add(width) > room.right() {
            break;
        }
        buttons.push(Rect { x, width, ..room });
        x += width;
    }
    buttons
}

pub(super) fn draw_plugin_buttons(f: &mut Frame, view: &View, r: Rect) {
    let row = plugin_buttons(r, view.plugin_buttons.iter().map(|b| b.title.as_str()));
    for (&button, face) in row.buttons.iter().zip(&view.plugin_buttons) {
        let label = truncate_right(&face.title, usize::from(button.width.saturating_sub(2)));
        draw_button(f, button, "", &label, plugin_button_style(view, button));
    }
    if !row.more.is_empty() {
        draw_button(f, row.more, "", PLUGINS_MORE, plugin_button_style(view, row.more));
    }
}

fn plugin_button_style(view: &View, r: Rect) -> Style {
    button_style(view, r, Style::default().fg(view.muted), Color::Cyan)
}

pub(super) fn draw_column_border(f: &mut Frame, colour: Color, r: Rect) {
    let line = Paragraph::new(vec![Line::from("│"); usize::from(r.height)]).style(Style::default().fg(colour));
    f.render_widget(line, r);
}

pub(super) fn draw_borders(f: &mut Frame, view: &View, areas: &Areas) {
    for border in [Border::Projects, Border::Workspaces, Border::Stack] {
        draw_border(f, view, areas.border(border), border);
    }
}

pub(super) fn draw_border(f: &mut Frame, view: &View, r: Rect, border: Border) {
    if view.resizing == Some(border) || sidebar_hovered(view, r) {
        let buf = f.buffer_mut();
        for y in r.top()..r.bottom() {
            for x in r.left()..r.right() {
                buf[(x, y)].set_fg(Color::Cyan);
            }
        }
    }
}

pub(super) fn draw_search_bar(f: &mut Frame, view: &View, r: Rect) {
    let dim = Style::default().fg(view.muted);
    let accent = Style::default().fg(Color::Cyan);
    let search_icon = format!(" {} ", view.glyphs().search);
    let icon_width = search_icon.chars().count();
    let line = if let Some(search) = view.search() {
        let button = mode_button(r);
        let max = usize::from(r.width.saturating_sub(button.width)).saturating_sub(icon_width + 1);
        let query = truncate_left(&search.query, max);
        let cursor = icon_width + query.chars().count();
        if let Ok(offset) = u16::try_from(cursor)
            && offset < r.width
        {
            f.set_cursor_position(Position::new(r.x + offset, r.y));
        }
        let lit = Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD);
        let (said, style) = (centered(view.glyphs().said, button.width), if search.sessions { lit } else { dim });
        let mut spans = vec![Span::styled(search_icon, accent)];
        if search.query.is_empty() {
            let hint = if search.sessions { SESSIONS_PLACEHOLDER } else { NAMES_PLACEHOLDER };
            spans.push(Span::styled(truncate_right(hint, max), dim));
        } else {
            spans.push(Span::styled(query, Style::default().add_modifier(Modifier::BOLD)));
        }
        f.render_widget(Paragraph::new(Line::from(spans)).style(Style::default().bg(view.surface())), r);
        f.render_widget(Paragraph::new(Span::styled(said, style)), button);
        return;
    } else {
        let icon = if sidebar_hovered(view, r) { accent } else { dim };
        let room = usize::from(r.width).saturating_sub(icon_width + 1);
        Line::from(vec![Span::styled(search_icon, icon), Span::styled(truncate_right(SEARCH_PLACEHOLDER, room), dim)])
    };
    f.render_widget(Paragraph::new(line).style(Style::default().bg(view.surface())), r);
}

pub(super) fn draw_bar(f: &mut Frame, view: &View, areas: &Areas) {
    let surface = Style::default().bg(view.surface());
    f.render_widget(Block::new().style(surface), areas.bar);
    if view.search().is_some() {
        draw_search_bar(f, view, middle(areas.bar));
        return;
    }
    let glyphs = view.glyphs();
    let pressed = Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD);
    let r = areas.todo_button;
    let mut right = areas.search_button.width + r.width;
    let lit = view.todo.is_some() || sidebar_hovered(view, r);
    let style = if lit { pressed } else { surface.fg(view.muted) };
    draw_band(f, r, todo_icon(glyphs, view.todo_dot && view.todo_pending, r.width, style), style);
    if view.has_project {
        let r = areas.files_button;
        right += r.width;
        let style = if view.files.is_some() || sidebar_hovered(view, r) { pressed } else { surface.fg(view.muted) };
        draw_band(f, r, Span::styled(centered(glyphs.files, r.width), style), style);
    }
    if let Some(button) = &view.changes_button {
        let r = areas.changes_button;
        right += r.width;
        let style = if button.open || sidebar_hovered(view, r) { pressed } else { surface.fg(view.muted) };
        draw_band(f, r, Span::styled(centered(glyphs.changes, r.width), style), style);
    }
    let menu = Rect { width: areas.bar.width.saturating_sub(right), ..areas.bar };
    let icon = Rect { width: COMPACT_BUTTON_WIDTH.min(menu.width), ..menu };
    let lit = view.nav.is_some() || sidebar_hovered(view, menu);
    let style = if lit { pressed } else { surface.fg(Color::Cyan) };
    let label = menu_label(glyphs, view, icon.width, (style, lit));
    draw_band(f, icon, label, style);
    let crumbs = Rect { x: icon.right() + 2, width: menu.width.saturating_sub(icon.width + 2), ..middle(menu) };
    f.render_widget(Paragraph::new(Line::from(breadcrumb(view, usize::from(crumbs.width)))), crumbs);
    let r = areas.search_button;
    let style = if sidebar_hovered(view, r) { pressed } else { surface.fg(view.muted) };
    draw_band(f, r, Span::styled(centered(glyphs.search, r.width), style), style);
}

fn breadcrumb(view: &View, room: usize) -> Vec<Span<'static>> {
    let Some(project) = view.shown_project().and_then(|p| view.projects.get(p)) else {
        return wordmark();
    };
    let workspace = view.workspaces.get(view.active_workspace);
    let tab = workspace.zip(view.active_tab).and_then(|(w, t)| w.tabs.get(t));
    let crumbs: Vec<&str> =
        [Some(project.name.as_str()), workspace.map(|w| w.name.as_str()), tab.map(|t| t.name.as_str())]
            .into_iter()
            .flatten()
            .collect();
    let text = truncate_right(&crumbs.join(CRUMB_SEPARATOR), room);
    let project_len = project.name.chars().count().min(text.chars().count());
    let bold = Style::default().fg(Color::Reset).add_modifier(Modifier::BOLD);
    let (head, rest): (String, String) =
        (text.chars().take(project_len).collect(), text.chars().skip(project_len).collect());
    vec![Span::styled(head, bold), Span::styled(rest, Style::default().fg(view.soft))]
}

pub(super) fn draw_back(f: &mut Frame, view: &View, areas: &Areas, name: &str) {
    let style = button_style(view, areas.back, Style::default().fg(Color::Cyan), Color::Cyan);
    draw_button(f, areas.back, "", BACK_LABEL, style);
    let rest =
        Rect { x: areas.back.right(), width: areas.bar.right().saturating_sub(areas.back.right()), ..areas.back };
    let max = usize::from(rest.width).saturating_sub(1);
    let style = Style::default().fg(view.muted).add_modifier(Modifier::BOLD);
    f.render_widget(Paragraph::new(Span::styled(format!(" {}", truncate_right(name, max)), style)), middle(rest));
}

pub(super) fn draw_issues_row(f: &mut Frame, view: &View, areas: &Areas) {
    let changes = view.changes_button.as_ref().filter(|_| !areas.compact());
    if view.issues {
        draw_separator(f, view.line(), areas.workspaces_separator);
        let issues = match changes {
            Some(button) => {
                let start = changes_button(areas.issues, &button.label).x;
                Rect { width: start.saturating_sub(areas.issues.x), ..areas.issues }
            }
            None => areas.issues,
        };
        let style = button_style(view, issues, Style::default().fg(view.muted), Color::Cyan);
        draw_button(f, issues, " ", ISSUES_LABEL, style);
    }
    if let Some(button) = changes {
        draw_panel_button(f, view, changes_button(areas.issues, &button.label), &button.label, button.open);
    }
    if !areas.compact() {
        draw_todo_button(f, view, areas);
        draw_panel_button(f, view, areas.files_button, files::LABEL, view.files.is_some());
    }
}

fn panel_button_style(view: &View, r: Rect, open: bool) -> Style {
    let idle = if open {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(view.muted)
    };
    button_style(view, r, idle, Color::Cyan)
}

fn draw_panel_button(f: &mut Frame, view: &View, r: Rect, label: &str, open: bool) {
    draw_button(f, r, "", label, panel_button_style(view, r, open));
}

fn draw_todo_button(f: &mut Frame, view: &View, areas: &Areas) {
    let r = areas.todo_with_dot(view.todo_dot && view.todo_pending);
    if r == areas.todo_button {
        draw_panel_button(f, view, r, TODO_LABEL, view.todo.is_some());
        return;
    }
    let style = panel_button_style(view, r, view.todo.is_some());
    let spans = vec![
        Span::styled(format!(" {TODO_LABEL} "), style),
        Span::styled(TODO_DOT, style.fg(view.muted)),
        Span::styled(" ", style),
    ];
    f.render_widget(Paragraph::new(Line::from(spans)), middle(r));
}

fn todo_icon(glyphs: &Glyphs, pending: bool, width: u16, style: Style) -> Line<'static> {
    let icon = glyphs.todo;
    if !pending {
        return Line::from(Span::styled(centered(icon, width), style));
    }
    let width = usize::from(width);
    let left = width.saturating_sub(1) / 2;
    Line::from(Span::styled(
        format!("{}{icon} {TODO_DOT}{}", " ".repeat(left), " ".repeat(width.saturating_sub(left + 3))),
        style,
    ))
}

fn menu_label(glyphs: &Glyphs, view: &View, width: u16, (style, lit): (Style, bool)) -> Line<'static> {
    let menu = glyphs.menu;
    let Some(status) = view.attention else { return Line::from(Span::styled(centered(menu, width), style)) };
    let width = usize::from(width);
    let left = width.saturating_sub(1) / 2;
    let badge = status_icon(view.muted, status, view.spin);
    let badge = if lit { Span::styled(badge.content, style) } else { badge };
    Line::from(vec![
        Span::styled(format!("{}{menu} ", " ".repeat(left)), style),
        badge,
        Span::styled(" ".repeat(width.saturating_sub(left + 3)), style),
    ])
}

fn draw_more_button(f: &mut Frame, view: &View, r: Rect) {
    let idle = match view.update {
        Some(_) => Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        None => Style::default().fg(view.muted),
    };
    let style = button_style(view, r, idle, Color::Cyan);
    if r.height > 1
        && let Some(bg) = style.bg
    {
        f.render_widget(Block::new().style(Style::default().bg(bg)), r);
    }
    let mut spans = vec![Span::raw(" "), Span::styled(format!(" {MORE_LABEL}"), style)];
    if view.remote == Some(true) {
        spans.push(Span::styled(format!(" {REMOTE_DOT}"), style.fg(Color::Green)));
    }
    spans.push(Span::styled(" ", style));
    f.render_widget(Paragraph::new(Line::from(spans)), middle(r));
}
