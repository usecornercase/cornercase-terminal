use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::View;
use super::chrome::{draw_back, draw_border};
use super::layout::{Areas, Border};
use super::rows::{Rows, draw_hidden, draw_rail, marker, row_close_button};
use super::theme::status_icon;
use super::widgets::{button_style, dim, draw_band, draw_button, draw_separator, draw_title, middle};
use super::workspaces::{Details, Guide, draw_details, draw_guide};
use crate::agents::activity::Status;
use crate::text::{CRUMB_SEPARATOR, truncate_right};

pub(crate) const AGENTS_LABEL: &str = "Agents ›";

const AGENTS_TITLE: &str = "Agents";

const NO_AGENTS: &str = "no agents running";

const AGENT_LINES: u16 = 2;

const MIN_WORKSPACE_WIDTH: usize = 4;

const MIN_ROLE_WIDTH: usize = 4;

const ROLE_SEPARATOR: &str = " · ";

const GUIDE_STEP: usize = 2;

pub(crate) fn agent_rows(list: Rect, pitch: u16, agents: usize, scroll: usize) -> Rows {
    Rows { list, heights: vec![pitch.max(AGENT_LINES); agents], button: 0, scroll }
}

pub(crate) fn agent_row(list: Rect, pitch: u16, agents: usize, scroll: usize, i: usize) -> Rect {
    agent_rows(list, pitch, agents, scroll).item(i)
}

pub(crate) fn agent_hit(list: Rect, pitch: u16, agents: usize, scroll: usize, pos: Position) -> Option<usize> {
    agent_rows(list, pitch, agents, scroll).at(pos)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentEntry {
    pub(crate) status: Option<Status>,
    pub(crate) agent: String,
    pub(crate) project: String,
    pub(crate) workspace: Option<String>,
    pub(crate) role: Option<String>,
    pub(crate) place: bool,
    pub(crate) details: Details,
    pub(crate) active: bool,
    pub(crate) depth: usize,
    pub(crate) last: bool,
    pub(crate) rails: Vec<bool>,
}

pub(crate) struct AgentsView {
    pub(crate) entries: Vec<AgentEntry>,
    pub(crate) scroll: usize,
}

pub(super) fn draw_agents(f: &mut Frame, view: &View, agents: &AgentsView, areas: &Areas) {
    if areas.agents.is_empty() {
        if !areas.agents_button.is_empty() {
            let style = button_style(view, areas.agents_button, Style::default().fg(Color::Cyan), Color::Cyan);
            draw_button(f, areas.agents_button, "", AGENTS_LABEL, style);
        }
        return;
    }
    draw_separator(f, view.line(), areas.agents_border);
    draw_border(f, view, areas.agents_border, Border::Agents);
    if areas.compact() {
        draw_back(f, view, areas, AGENTS_TITLE);
    } else {
        draw_title(f, view.muted, areas.agents_title, AGENTS_TITLE);
    }
    let list = areas.agents_list;
    let rows = agent_rows(list, areas.pitch, agents.entries.len(), agents.scroll);
    if agents.entries.is_empty() {
        let first = Rect { height: areas.pitch.min(list.height), ..list };
        f.render_widget(Paragraph::new(Span::styled(format!("  {NO_AGENTS}"), dim(view.muted))), middle(first));
    }
    for (i, entry) in agents.entries.iter().enumerate() {
        let r = rows.item(i);
        if !r.is_empty() {
            draw_agent(f, view, r, areas.pitch, entry);
        }
    }
    let indices: Vec<usize> = (0..agents.entries.len()).collect();
    draw_hidden(f, view.muted, &rows, &indices, |_| true);
}

pub(super) fn draw_agent(f: &mut Frame, view: &View, r: Rect, pitch: u16, entry: &AgentEntry) {
    let bg = view.row_background(r, entry.active);
    let mut line = vec![marker(entry.active), Span::raw(" ".repeat(GUIDE_STEP * entry.depth))];
    if let Some(status) = entry.status {
        line.extend([status_icon(view.muted, status, view.spin), Span::raw(" ")]);
    }
    let indent = line.iter().map(Span::width).sum::<usize>();
    let room = usize::from(r.width).saturating_sub(indent + 1);
    let colour = if entry.active { Color::Reset } else { view.soft };
    line.extend(agent_place(entry, room, Style::default().fg(colour), dim(view.muted)));
    draw_band(f, r, Line::from(line), bg);
    draw_details(f, view.muted, (r, row_close_button(r, pitch)), &entry.details, Some(&entry.agent), indent);
    draw_guides(f, view.line(), r, entry);
    if entry.active {
        draw_rail(f, r);
    }
}

fn draw_guides(f: &mut Frame, colour: Color, r: Rect, entry: &AgentEntry) {
    if entry.depth == 0 {
        return;
    }
    let column = |depth: usize| r.x.saturating_add(u16::try_from(GUIDE_STEP * depth).unwrap_or(u16::MAX));
    for (k, _) in entry.rails.iter().enumerate().filter(|(_, rail)| **rail) {
        draw_guide(f, colour, column(k + 1), r, Guide::Bar);
    }
    let own = if entry.last { Guide::End } else { Guide::Tee };
    draw_guide(f, colour, column(entry.depth), r, own);
}

fn agent_place(entry: &AgentEntry, room: usize, name: Style, separator: Style) -> Vec<Span<'static>> {
    if let Some(role) = entry.role.as_deref().filter(|_| !entry.place) {
        return vec![Span::styled(truncate_right(role, room), name)];
    }
    let project = entry.project.chars().count();
    let rest = room.saturating_sub(project + CRUMB_SEPARATOR.chars().count());
    let mut spans = match entry.workspace.as_deref().filter(|_| rest >= MIN_WORKSPACE_WIDTH) {
        Some(workspace) => vec![
            Span::styled(entry.project.clone(), name),
            Span::styled(CRUMB_SEPARATOR, separator),
            Span::styled(truncate_right(workspace, rest), name),
        ],
        None => vec![Span::styled(truncate_right(&entry.project, room), name)],
    };
    if let Some(role) = &entry.role {
        let used: usize = spans.iter().map(|span| span.content.chars().count()).sum();
        let left = room.saturating_sub(used + ROLE_SEPARATOR.chars().count());
        if left >= MIN_ROLE_WIDTH {
            spans.push(Span::styled(ROLE_SEPARATOR, separator));
            spans.push(Span::styled(truncate_right(role, left), name));
        }
    }
    spans
}
