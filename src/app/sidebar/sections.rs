use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::layout::{Position, Rect};

use crate::agents::activity::{self, Status};
use crate::app::App;
use crate::app::interaction::{Drag, Grab};
use crate::state::Section;
use crate::ui;

impl App {
    pub(in crate::app) fn set_section_fold(&mut self, section: Section, folded: bool) {
        if self.screen.sections.folded(section) == folded {
            return;
        }
        self.screen.sections.set(section, folded);
        self.drop_sidebar_drag();
        if !folded {
            self.scroll.reveal_again();
        }
    }

    pub(in crate::app) fn toggle_section(&mut self, section: Section) {
        self.set_section_fold(section, !self.screen.sections.folded(section));
    }

    pub(in crate::app) fn section_title_mouse(
        &mut self,
        areas: &ui::Areas,
        pos: Position,
        kind: MouseEventKind,
        area: Rect,
    ) -> bool {
        let mut titles = Section::ALL.into_iter().map(|section| (section, ui::section_title(areas, section)));
        let Some((section, title)) = titles.find(|(_, title)| title.contains(pos)) else { return false };
        if kind == MouseEventKind::Down(MouseButton::Left) {
            self.grab_with(Grab::Section(section), title, area, false);
        }
        true
    }

    fn drop_sidebar_drag(&mut self) {
        let stale = match &self.screen.drag {
            Some(Drag::Row(drag)) => {
                !matches!(drag.target, Grab::Todo(_)) && !self.screen.drawn.tab_bar.contains(drag.row.as_position())
            }
            Some(Drag::Border(border)) => matches!(border, ui::Border::Stack | ui::Border::Agents),
            _ => false,
        };
        if stale {
            self.screen.drag = None;
        }
    }
}

pub(in crate::app) fn section_tags(view: &ui::View) -> Vec<(Section, ui::SectionTags)> {
    view.folded.listed().into_iter().filter_map(|section| Some((section, tags_of(view, section)?))).collect()
}

fn tags_of(view: &ui::View, section: Section) -> Option<ui::SectionTags> {
    match section {
        Section::Projects => Some(counted(view.projects.iter().map(|project| project.status))),
        Section::Workspaces => {
            Some(counted(view.workspaces.iter().map(|w| activity::attention(w.tabs.iter().map(|tab| tab.status)))))
        }
        Section::Agents => view.agents.as_ref().map(agent_tags),
    }
}

fn counted(statuses: impl Iterator<Item = Option<Status>>) -> ui::SectionTags {
    let statuses: Vec<Option<Status>> = statuses.collect();
    ui::SectionTags { count: statuses.len(), attention: activity::attention(statuses) }
}

fn agent_tags(agents: &ui::AgentsView) -> ui::SectionTags {
    let count = agents.entries.iter().map(|entry| 1 + entry.hidden).sum();
    let statuses = agents.entries.iter().flat_map(|entry| [entry.status, entry.attention]);
    ui::SectionTags { count, attention: activity::attention(statuses) }
}
