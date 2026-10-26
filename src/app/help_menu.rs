use ratatui::layout::{Position, Rect};

use super::App;
use super::menus::MenuAction;
use super::overlay::Overlay;
use super::toast::Toast;
use crate::tour::{self, TOURS, Target};
use crate::ui::{self, ActivityItem};

pub(super) const BUG_URL: &str = "https://github.com/usecornercase/cornercase-terminal/issues/new";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HelpItem {
    Tour,
    Catalogue,
    Docs,
    Keys,
    Bug,
}

fn label_cells(r: Rect, label: &str) -> Rect {
    let row = Rect { y: r.y + r.height.saturating_sub(1) / 2, height: r.height.min(1), ..r };
    let width = u16::try_from(label.chars().count()).unwrap_or(u16::MAX).saturating_add(2);
    Rect { x: row.x.saturating_add(1), width, ..row }.intersection(row)
}

impl App {
    pub(super) fn open_help(&mut self, at: Position) {
        let keys = crate::shortcuts::Prefix::parse(&self.config.prefix_key)
            .is_some()
            .then_some(MenuAction::Help(HelpItem::Keys));
        let actions = [HelpItem::Tour, HelpItem::Catalogue, HelpItem::Docs]
            .into_iter()
            .map(MenuAction::Help)
            .chain(keys)
            .chain(std::iter::once(MenuAction::Help(HelpItem::Bug)))
            .collect();
        self.overlay = Some(Overlay::Menu { at, actions });
    }

    pub(super) fn help_label(&self, item: HelpItem) -> String {
        let core = &TOURS[0];
        let started = !self.messages.tour.saved().progress(core.id).done.is_empty()
            && !self.messages.tour.saved().finished(core.id);
        match item {
            HelpItem::Tour if started => "continue the tour".into(),
            HelpItem::Tour => "tour".into(),
            HelpItem::Catalogue => "catalogue…".into(),
            HelpItem::Docs => "docs".into(),
            HelpItem::Keys => "keys menu".into(),
            HelpItem::Bug => "report a bug".into(),
        }
    }

    pub(super) fn help_menu_label(&self, action: MenuAction) -> Option<String> {
        match action {
            MenuAction::Help(item) => Some(self.help_label(item)),
            MenuAction::Tour(index) => Some(self.tour_label(index)),
            _ => None,
        }
    }

    pub(super) fn tour_label(&self, index: u8) -> String {
        let Some(tour) = TOURS.get(usize::from(index)) else { return String::new() };
        let progress = self.messages.tour.saved().progress(tour.id);
        if progress.finished {
            format!("{} ✓", tour.title)
        } else if progress.done.is_empty() {
            tour.title.into()
        } else {
            format!("{} {}/{}", tour.title, progress.done.len(), tour.steps.len())
        }
    }

    pub(super) fn help_action(&mut self, item: HelpItem, at: Position) {
        match item {
            HelpItem::Tour => self.start_tour(0, None, true),
            HelpItem::Catalogue => {
                let actions = (0..TOURS.len()).filter_map(|i| u8::try_from(i).ok()).map(MenuAction::Tour).collect();
                self.overlay = Some(Overlay::Menu { at, actions });
            }
            HelpItem::Docs => self.copy_address(tour::DOCS),
            HelpItem::Keys => self.overlay = Some(Overlay::Keys(None)),
            HelpItem::Bug => self.copy_address(BUG_URL),
        }
    }

    fn copy_address(&mut self, url: &str) {
        self.copy(url);
        self.messages.toast = Some(Toast::new(format!("copied {url}"), ui::ToastIcon::Check));
    }

    pub(super) fn tour_target(&self, areas: &ui::Areas, target: Target) -> (Rect, bool) {
        let found = match self.icon_target(areas, target) {
            Some(r) => (r, true),
            None => Self::region_target(areas, target)
                .map_or_else(|| (self.button_target(areas, target), true), |r| (r, false)),
        };
        if found.0.is_empty() && areas.compact() && target != Target::None {
            let width = ui::COMPACT_BUTTON_WIDTH.min(areas.bar.width);
            return (Rect { width, ..areas.bar }, true);
        }
        found
    }

    fn icon_target(&self, areas: &ui::Areas, target: Target) -> Option<Rect> {
        let item = match target {
            Target::Issues => ActivityItem::Issues,
            Target::Changes => ActivityItem::Changes,
            Target::Files => ActivityItem::Files,
            Target::Todo => ActivityItem::Todo,
            Target::Settings | Target::Usage | Target::Remote => ActivityItem::More,
            Target::Help => ActivityItem::Help,
            _ => return None,
        };
        let items = ui::activity_items(areas.activity, self.activity_shown());
        (!areas.activity.is_empty())
            .then(|| items.into_iter().find_map(|(i, r)| (i == item).then_some(r)).unwrap_or_default())
    }

    fn region_target(areas: &ui::Areas, target: Target) -> Option<Rect> {
        Some(match target {
            Target::None => Rect::default(),
            Target::NewProject if !areas.tree => return None,
            Target::NewProject | Target::Projects => areas.list,
            Target::Tabs if !areas.tab_bar.is_empty() => areas.tab_bar,
            Target::Workspaces | Target::Tabs if areas.tree => areas.list,
            Target::Workspaces | Target::Tabs => areas.workspaces_list,
            Target::Pane if areas.pane.x > 0 && !areas.compact() => {
                Rect { x: areas.pane.x - 1, width: 1, ..areas.pane }
            }
            Target::Pane => areas.pane,
            Target::Agents => areas.agents_list,
            Target::Search if !areas.compact() => areas.search,
            _ => return None,
        })
    }

    fn button_target(&self, areas: &ui::Areas, target: Target) -> Rect {
        match target {
            Target::NewProject => ui::new_project_button(areas.list, areas.pitch, &self.sidebar_rows()),
            Target::Issues => label_cells(areas.issues, "Issues"),
            Target::Changes if areas.compact() => areas.changes_button,
            Target::Changes => self.changes_label().map(|l| ui::changes_button(areas.issues, &l)).unwrap_or_default(),
            Target::Files => areas.files_button,
            Target::Todo => areas.todo_button,
            Target::Search => areas.search_button,
            Target::Settings | Target::Usage | Target::Remote => label_cells(areas.more, ui::MORE_LABEL),
            _ => ui::help_button(areas.more),
        }
    }
}
