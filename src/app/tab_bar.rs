use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::menus::MenuAction;
use super::menus::Target;
use super::overlay::Overlay;
use crate::error::Result;
use crate::project::{Project, Tab, Workspace};
use crate::ui::{self};

impl App {
    pub(super) fn reveal_in_tab_bar(&mut self, areas: &ui::Areas, moved: bool) {
        if moved {
            self.tab_bar_scroll = 0;
        }
        let Some(t) = self.project().and_then(Project::workspace).filter(|w| !w.tabs.is_empty()).map(|w| w.active)
        else {
            return;
        };
        self.tab_bar_scroll = self.tab_strip(areas).reveal(t);
    }

    pub(super) fn tab_strip(&self, areas: &ui::Areas) -> ui::tab_bar::Strip {
        ui::tab_bar::Strip::new(areas.tab_bar, &self.bar_tabs(), self.tab_bar_scroll)
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
            details: self.details(pane.and_then(|t| t.context.context()), pane.and_then(|t| t.memory.bytes())),
            scroll: self.tab_bar_scroll,
        }
    }

    pub(super) fn tab_bar_mouse(
        &mut self,
        areas: &ui::Areas,
        pos: Position,
        kind: MouseEventKind,
        area: Rect,
    ) -> Result<()> {
        let Some(project) = self.project() else { return Ok(()) };
        let (p, w) = (self.active, project.active);
        let Some(workspace) = project.workspace() else { return Ok(()) };
        let strip = self.tab_strip(areas);
        let Some(hit) = strip.hit(pos) else { return Ok(()) };
        let target = |t: usize| Target::Tab(project.id, workspace.id, workspace.tabs[t].id);
        match (kind, hit) {
            (MouseEventKind::Down(MouseButton::Left), ui::tab_bar::Hit::Tab(t)) => {
                self.grab(target(t), strip.item(t), area, false);
            }
            (MouseEventKind::Down(MouseButton::Left), ui::tab_bar::Hit::Close(t)) => self.close_tab(p, w, t),
            (MouseEventKind::Down(MouseButton::Left), ui::tab_bar::Hit::New) => self.add_tab(p, w, area)?,
            (MouseEventKind::Down(MouseButton::Left), ui::tab_bar::Hit::Scroll(delta)) => {
                self.tab_bar_scroll = strip.scrolled(delta);
            }
            (MouseEventKind::Down(MouseButton::Left), ui::tab_bar::Hit::Menu(t))
            | (
                MouseEventKind::Down(MouseButton::Right),
                ui::tab_bar::Hit::Tab(t) | ui::tab_bar::Hit::Close(t) | ui::tab_bar::Hit::Menu(t),
            ) => {
                self.overlay = Some(Overlay::Menu { at: pos, actions: vec![MenuAction::Rename(target(t))] });
            }
            _ => {}
        }
        Ok(())
    }
}
