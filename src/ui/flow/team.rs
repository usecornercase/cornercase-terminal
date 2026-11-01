use ratatui::buffer::Buffer;
use ratatui::layout::Position;
use ratatui::style::{Color, Modifier, Style};

use super::stack::{Look, Slot, cells, count};
use super::{Hit, State, dim};
use crate::text;
use crate::ui::theme::status_icon;

const TITLE: &str = "Team";
const LEAD: u16 = 2;
const MIN_ROLE: u16 = 4;
const MIN_TEXT: u16 = 4;
const MIN_BRANCH: u16 = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Member {
    pub(crate) role: String,
    pub(crate) agent: String,
    pub(crate) colour: Color,
    pub(crate) state: State,
    pub(crate) branch: Option<String>,
    pub(crate) pane: Option<u64>,
}

pub(super) fn height(team: &[Member]) -> u16 {
    if team.is_empty() { 0 } else { LEAD.saturating_add(count(team.len())) }
}

pub(super) fn hit(slot: &Slot, team: &[Member], pos: Position) -> Option<Hit> {
    let index = usize::from(slot.row_at(pos)?.checked_sub(LEAD)?);
    let member = team.get(index).filter(|member| member.state != State::Closed)?;
    Some(Hit::Role(member.role.clone()))
}

struct Fit {
    role: String,
    role_w: u16,
    agent: Option<String>,
    branch: Option<String>,
    pane: String,
}

fn pane_text(member: &Member) -> String {
    match (member.state, member.pane) {
        (State::Closed, _) | (_, None) => String::new(),
        (_, Some(pane)) => format!("pane {pane}"),
    }
}

fn cut(text: &str, room: u16, least: u16) -> Option<String> {
    (room >= least).then(|| text::truncate_cells(text, usize::from(room)))
}

fn fit(member: &Member, width: u16, longest: u16) -> Fit {
    let pane = pane_text(member);
    let pane_w = cells(&pane);
    let gap = u16::from(pane_w > 0);
    let after_icon = width.saturating_sub(2).saturating_sub(pane_w.saturating_add(gap));
    let role_w = longest.min(after_icon).max(MIN_ROLE.min(longest));
    let role = text::truncate_cells(&member.role, usize::from(role_w));
    let left = after_icon.saturating_sub(role_w.saturating_add(2));
    let said = if member.state == State::Closed { "closed" } else { member.agent.as_str() };
    let agent = cut(said, left, MIN_TEXT);
    let left = left.saturating_sub(agent.as_deref().map_or(0, |a| cells(a).saturating_add(2)));
    let branch = member.branch.as_deref().filter(|_| member.state != State::Closed);
    let branch = branch.filter(|b| cells(b).saturating_add(2) <= left).and_then(|b| cut(b, left - 2, MIN_BRANCH));
    Fit { role, role_w, agent, branch, pane }
}

fn icon(member: &Member, muted: Color, spin: Option<u8>) -> ratatui::text::Span<'static> {
    match member.state {
        State::Agent(Some(status)) => status_icon(muted, status, spin),
        State::Agent(None) => status_icon(muted, crate::agents::activity::Status::Idle, spin),
        State::Closed => ratatui::text::Span::styled("·", dim(muted)),
    }
}

fn draw_row(buf: &mut Buffer, slot: &Slot, (row, member): (u16, &Member), longest: (u16, Look, Option<u8>)) {
    let (longest, look, spin) = longest;
    let closed = member.state == State::Closed;
    let fit = fit(member, slot.area.width, longest);
    let icon = icon(member, look.muted, spin);
    slot.put(buf, (row, 0), &icon.content, icon.style);
    let role = if closed { dim(look.muted) } else { Style::default().fg(member.colour).add_modifier(Modifier::BOLD) };
    slot.put(buf, (row, 2), &fit.role, role);
    let mut x = fit.role_w.saturating_add(4);
    for part in [&fit.agent, &fit.branch].into_iter().flatten() {
        slot.put(buf, (row, x), part, dim(look.muted));
        x = x.saturating_add(cells(part)).saturating_add(2);
    }
    slot.put_right(buf, row, &fit.pane, dim(look.muted));
}

pub(super) fn draw(buf: &mut Buffer, slot: &Slot, team: &[Member], (look, spin): (Look, Option<u8>)) {
    if team.is_empty() {
        return;
    }
    slot.title(buf, 1, (TITLE, &team.len().to_string()), look.muted);
    let longest = team.iter().map(|member| cells(&member.role)).max().unwrap_or(0);
    for (i, member) in team.iter().enumerate() {
        let row = LEAD + count(i);
        if member.state != State::Closed {
            slot.shade(buf, row..row + 1, look);
        }
        draw_row(buf, slot, (row, member), (longest, look, spin));
    }
}
