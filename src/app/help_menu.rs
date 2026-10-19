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
        let keys = crate::shortcuts::Prefix::parse(&self.config.prefix_key).is_some().then_some(MenuAction::Help(HelpItem::Keys));
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
        let started = !self.tour.saved.progress(core.id).done.is_empty() && !self.tour.saved.finished(core.id);
        match item {
            HelpItem::Tour if started => "continue the tour".into(),
            HelpItem::Tour => "tour".into(),
            HelpItem::Catalogue => "catalogue…".into(),
            HelpItem::Docs => "docs".into(),
            HelpItem::Keys => "keys menu".into(),
            HelpItem::Bug => "report a bug".into(),
        }
    }

    pub(super) fn tour_label(&self, index: u8) -> String {
        let Some(tour) = TOURS.get(usize::from(index)) else { return String::new() };
        let progress = self.tour.saved.progress(tour.id);
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
            HelpItem::Docs => self.copy_link(tour::DOCS),
            HelpItem::Keys => self.overlay = Some(Overlay::Keys(None)),
            HelpItem::Bug => self.copy_link(BUG_URL),
        }
    }

    fn copy_link(&mut self, url: &str) {
        self.copy(url);
        self.toast = Some(Toast::new(format!("copied {url}"), ui::ToastIcon::Check));
    }

    pub(super) fn tour_target(&self, areas: &ui::Areas, target: Target) -> (Rect, bool) {
        let icon = |item: ActivityItem| {
            ui::activity_items(areas.activity, self.activity_shown())
                .into_iter()
                .find_map(|(i, r)| (i == item).then_some(r))
                .unwrap_or_default()
        };
        let icons = !areas.activity.is_empty();
        let button = |r: Rect| (r, true);
        let region = |r: Rect| (r, false);
        let found = match target {
            Target::None => region(Rect::default()),
            Target::NewProject if areas.tree => region(areas.list),
            Target::NewProject => button(ui::new_project_button(areas.list, areas.pitch, &self.sidebar_rows())),
            Target::Projects => region(areas.list),
            Target::Workspaces if areas.tree => region(areas.list),
            Target::Workspaces => region(areas.workspaces_list),
            Target::Tabs if !areas.tab_bar.is_empty() => region(areas.tab_bar),
            Target::Tabs if areas.tree => region(areas.list),
            Target::Tabs => region(areas.workspaces_list),
            Target::Pane if areas.pane.x > 0 && !areas.compact() => {
                region(Rect { x: areas.pane.x - 1, width: 1, ..areas.pane })
            }
            Target::Pane => region(areas.pane),
            Target::Agents => region(areas.agents_list),
            Target::Issues if icons => button(icon(ActivityItem::Issues)),
            Target::Issues => button(label_cells(areas.issues, "Issues")),
            Target::Changes if icons => button(icon(ActivityItem::Changes)),
            Target::Changes if areas.compact() => button(areas.changes_button),
            Target::Changes => {
                button(self.changes_label().map(|l| ui::changes_button(areas.issues, &l)).unwrap_or_default())
            }
            Target::Files if icons => button(icon(ActivityItem::Files)),
            Target::Files => button(areas.files_button),
            Target::Todo if icons => button(icon(ActivityItem::Todo)),
            Target::Todo => button(areas.todo_button),
            Target::Search if areas.compact() => button(areas.search_button),
            Target::Search => region(areas.search),
            Target::Settings if icons => button(icon(ActivityItem::Settings)),
            Target::Settings => button(label_cells(areas.settings, "Settings")),
            Target::Usage if icons => button(icon(ActivityItem::Usage)),
            Target::Usage => button(label_cells(areas.usage, "Usage")),
            Target::Remote if icons => button(icon(ActivityItem::Remote)),
            Target::Remote => button(self.remote_button().map(|on| ui::remote_button(areas.usage, on)).unwrap_or_default()),
            Target::Help if icons => button(icon(ActivityItem::Help)),
            Target::Help => button(ui::help_button(areas.quit)),
        };
        if found.0.is_empty() && areas.compact() && target != Target::None {
            let width = ui::COMPACT_BUTTON_WIDTH.min(areas.bar.width);
            return button(Rect { width, ..areas.bar });
        }
        found
    }
}
