use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

use super::{TabEntry, WorkspaceEntry};
use crate::text::{cells, truncate_cells};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MachineTag {
    pub(crate) icon: String,
    pub(crate) name: String,
    pub(crate) dim: bool,
    pub(crate) seen: Option<String>,
}

impl WorkspaceEntry {
    pub(crate) fn tab_lines(&self) -> Vec<u16> {
        self.tabs.iter().map(|tab| tab.lines(self.machine.as_ref())).collect()
    }
}

impl TabEntry {
    pub(crate) fn lines(&self, machine: Option<&MachineTag>) -> u16 {
        if seen_note(self, machine).is_some() { 2 } else { self.details.lines() }
    }
}

pub(super) fn seen_note(tab: &TabEntry, machine: Option<&MachineTag>) -> Option<String> {
    let machine = machine.filter(|machine| machine.dim)?;
    let status = tab.status?.name();
    Some(machine.seen.as_ref().map_or_else(|| status.to_string(), |ago| format!("{status} · seen {ago} ago")))
}

pub(in crate::ui) fn machine_span(machine: &MachineTag, room: usize, muted: Color) -> Span<'static> {
    let icon = cells(&machine.icon);
    let name = truncate_cells(&machine.name, (room / 2).saturating_sub(icon + 1));
    Span::styled(format!("{} {name}", machine.icon), Style::default().fg(muted))
}

pub(in crate::ui) fn spin(machine: Option<&MachineTag>, spin: Option<u8>) -> Option<u8> {
    if machine.is_some_and(|machine| machine.dim) { spin.map(|_| 0) } else { spin }
}

pub(in crate::ui) fn dim_row(f: &mut Frame, r: Rect, machine: Option<&MachineTag>) {
    if machine.is_some_and(|machine| machine.dim) {
        f.buffer_mut().set_style(r, Style::default().add_modifier(Modifier::DIM));
    }
}
