use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::layout::List;
use super::overlay::Overlay;
use crate::error::Result;
use crate::project::{Hierarchy, Project, Tab, Workspace};
use crate::ui::{self};

impl App {
    pub(super) fn reveal_in_tab_bar(&mut self, areas: &ui::Areas, moved: bool) {
        if moved {
            self.scroll.set(List::TabBar, 0);
        }
        let Some(t) = self.project().and_then(Project::workspace).filter(|w| !w.tabs.is_empty()).map(|w| w.active)
        else {
            return;
        };
        self.scroll.set(List::TabBar, self.tab_strip(areas).reveal(t));
    }

    pub(super) fn tab_strip(&self, areas: &ui::Areas) -> ui::tab_bar::Strip {
        ui::tab_bar::Strip::new(areas.tab_bar, &self.bar_tabs(), self.scroll.of(List::TabBar))
    }

    fn bar_tabs(&self) -> Vec<ui::TabEntry> {
        let workspace = self.project().and_then(Project::workspace);
        workspace.map(|w| w.tabs.iter().map(|t| self.tab_entry(t)).collect()).unwrap_or_default()
    }

    pub(super) fn tab_bar_view(&self) -> ui::tab_bar::TabBar {
        let workspace = self.project().and_then(Project::workspace);
        let pane = workspace.and_then(Workspace::tab).and_then(Tab::pane);
        ui::tab_bar::TabBar {
            tabs: self.bar_tabs(),
            active: workspace.filter(|w| !w.tabs.is_empty()).map(|w| w.active),
            details: ui::Details {
                compacted: self.shows_compaction(pane.is_some_and(|t| t.context.compaction().hides)),
                ..self.details(
                    pane.and_then(|t| t.context.context()),
                    pane.and_then(|t| t.memory.bytes()),
                    pane.map_or(0, |t| t.context.subagents()),
                    pane.and_then(|t| self.pane_account(t)),
                )
            },
            scroll: self.scroll.of(List::TabBar),
        }
    }

    pub(super) fn tab_bar_mouse(
        &mut self,
        areas: &ui::Areas,
        pos: Position,
        kind: MouseEventKind,
        area: Rect,
    ) -> Result<()> {
        let Some(at) = self.projects.active_workspace(self.active) else { return Ok(()) };
        let strip = self.tab_strip(areas);
        let Some(hit) = strip.hit(pos) else { return Ok(()) };
        let tab = match hit {
            ui::tab_bar::Hit::Tab(t) | ui::tab_bar::Hit::Close(t) | ui::tab_bar::Hit::Menu(t) => {
                self.projects.tab_at(at.project(), at.index(), t)
            }
            ui::tab_bar::Hit::New | ui::tab_bar::Hit::Scroll(_) => None,
        };
        match (kind, hit, tab) {
            (MouseEventKind::Down(MouseButton::Left), ui::tab_bar::Hit::Tab(t), Some(tab)) => {
                self.grab(self.tab_target(tab), strip.item(t), area, false);
            }
            (MouseEventKind::Down(MouseButton::Left), ui::tab_bar::Hit::Close(_), Some(tab)) => self.close_tab(tab),
            (MouseEventKind::Down(MouseButton::Left), ui::tab_bar::Hit::New, _) => self.add_tab(at, area)?,
            (MouseEventKind::Down(MouseButton::Left), ui::tab_bar::Hit::Scroll(delta), _) => {
                self.scroll.set(List::TabBar, strip.scrolled(delta));
            }
            (MouseEventKind::Down(MouseButton::Left), ui::tab_bar::Hit::Menu(_), Some(tab))
            | (MouseEventKind::Down(MouseButton::Right), _, Some(tab)) => {
                self.overlay = Some(Overlay::Menu { at: pos, actions: self.target_menu(self.tab_target(tab)) });
            }
            _ => {}
        }
        Ok(())
    }
}
