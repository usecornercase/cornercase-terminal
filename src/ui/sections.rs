use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::View;
use super::layout::Areas;
use super::rows::{Tags, arrow};
use super::theme::status_icon;
use super::widgets::draw_band;
use super::workspaces::WORKSPACES_TITLE;
use crate::agents::activity::Status;
use crate::state::Section;
use crate::text::{cells, truncate_cells};

const LEAD: u16 = 3;

const MIN_NAME: usize = 2;

const NAME_BESIDE_COUNT: usize = WORKSPACES_TITLE.len();

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SectionTags {
    pub(crate) count: usize,
    pub(crate) attention: Option<Status>,
}

pub(super) fn title_width(title: &str) -> u16 {
    LEAD.saturating_add(u16::try_from(title.chars().count()).unwrap_or(u16::MAX))
}

pub(crate) fn section_title(areas: &Areas, section: Section) -> Rect {
    match section {
        Section::Projects => left_of(areas.title, [areas.bots_button, areas.agents_button]),
        Section::Workspaces if bots_page(areas) => Rect::default(),
        Section::Workspaces => right_of(areas.workspaces_title, areas.back),
        Section::Agents => right_of(areas.agents_title, areas.back),
    }
}

fn bots_page(areas: &Areas) -> bool {
    !areas.compact() && !areas.bots.is_empty()
}

fn left_of(title: Rect, buttons: [Rect; 2]) -> Rect {
    let edge = buttons.into_iter().filter(|b| !b.is_empty()).map(|b| b.x).fold(title.right(), u16::min);
    Rect { width: edge.saturating_sub(title.x), ..title }
}

fn right_of(title: Rect, button: Rect) -> Rect {
    let x = button.right().clamp(title.x, title.right());
    Rect { x, width: title.right() - x, ..title }
}

pub(super) fn draw_section_title(f: &mut Frame, view: &View, areas: &Areas, section: Section, title: &str) {
    let target = section_title(areas, section);
    if target.is_empty() {
        return;
    }
    let folded = areas.folded.folded(section);
    let tags = view.tags_of(section).filter(|_| folded);
    let line = title_line(view, target.width, (title, folded), tags);
    draw_band(f, target, line, view.row_background(target, false));
}

fn title_line(view: &View, width: u16, (title, folded): (&str, bool), tags: Option<SectionTags>) -> Line<'static> {
    let bold = Style::default().fg(view.muted).add_modifier(Modifier::BOLD);
    let mut line = vec![Span::raw(" "), arrow(view.muted, folded)];
    let width = usize::from(width.saturating_sub(LEAD));
    let Some(tags) = tags else {
        line.push(Span::styled(cut(title, width), bold));
        return Line::from(line);
    };
    let room = width.saturating_sub(1);
    let badge = tags.attention.map(|status| status_icon(view.muted, status, view.spin));
    let marks = Tags::fit_whole(badge.into_iter().collect(), room);
    let left = room.saturating_sub(marks.reserved());
    let kept = cells(title).clamp(MIN_NAME, NAME_BESIDE_COUNT);
    let count = count_text(view.counts, tags.count, left.saturating_sub(kept));
    let bare = marks.is_empty() && count.is_empty();
    let name = cut(title, if bare { width } else { left.saturating_sub(cells(&count)) });
    let used = cells(&name) + cells(&count);
    line.extend([Span::styled(name, bold), Span::styled(count, Style::default().fg(view.muted))]);
    marks.push_onto(&mut line, used, room);
    Line::from(line)
}

fn count_text(counts: bool, count: usize, room: usize) -> String {
    let text = format!(" ({count})");
    if counts && cells(&text) <= room { text } else { String::new() }
}

fn cut(title: &str, max: usize) -> String {
    if max < MIN_NAME { String::new() } else { truncate_cells(title, max) }
}
