use super::*;
use crate::config;
use crate::control::TabInfo;
use crate::state;
use crate::ui::{SidebarRow, TreeRow, WorkspaceRow};

#[derive(Clone, Copy)]
enum Place {
    Column,
    Tree,
}

#[derive(Clone, Copy)]
enum Way {
    Button,
    RightClick,
}

fn with_worktree() -> Remote {
    let found = || json!({ "workspace": WORKSPACE, "path": PATH });
    seeing_in(Reply::Found(found), holding(BOOT, WorkspaceInfo { worktree: true, ..workspace() }))
}

fn opened(app: &App) -> Option<&'static str> {
    match &app.overlay {
        Some(Overlay::MachineWorkspace(dialog)) => Some(dialog.name()),
        _ => None,
    }
}

fn shown(app: &App) -> Option<u64> {
    app.shown_remote().map(|shown| shown.workspace)
}

impl Remote {
    fn shown_in(mut self, place: Place) -> Self {
        match place {
            Place::Column => {
                let pos = sidebar_pos(&self.rig.app, SidebarRow::Project(0));
                click(&mut self.rig.app, pos);
            }
            Place::Tree => self.rig.app.config.sidebar = config::Sidebar::Tree.id().into(),
        }
        render(&mut self.rig.app, AREA);
        self
    }

    fn row(&self, place: Place) -> (Rect, u16) {
        let app = &self.rig.app;
        match place {
            Place::Column => {
                let areas = app_areas(app);
                let scroll = app.scroll.of(List::Workspaces);
                let row = WorkspaceRow::Workspace(0);
                (ui::workspace_row(areas.workspaces_list, areas.pitch, &app.tab_lines(), scroll, row), areas.pitch)
            }
            Place::Tree => {
                let list = ui::layout_with(AREA, state::Widths::default(), false, config::Sidebar::Tree).list;
                let row = TreeRow::Workspace(0, 0);
                (ui::tree_row(list, &app.tree_shape(), app.scroll.of(List::Projects), row), 1)
            }
        }
    }

    fn click_tree(&mut self, row: TreeRow) {
        let list = ui::layout_with(AREA, state::Widths::default(), false, config::Sidebar::Tree).list;
        let rect = ui::tree_row(list, &self.rig.app.tree_shape(), self.rig.app.scroll.of(List::Projects), row);
        click(&mut self.rig.app, Position::new(rect.x + rect.width / 2, rect.y));
    }

    fn close_at(&mut self, place: Place) {
        let (row, pitch) = self.row(place);
        click(&mut self.rig.app, ui::row_close_button(row, pitch).as_position());
    }

    fn menu_at(&mut self, place: Place, way: Way) {
        let (row, pitch) = self.row(place);
        match way {
            Way::Button => click(&mut self.rig.app, ui::row_menu_button(row, pitch).as_position()),
            Way::RightClick => right_click(&mut self.rig.app, Position::new(row.x + 6, row.y)),
        }
    }
}

mod its_close_button {
    use super::*;

    #[rstest]
    #[case::column(Place::Column)]
    #[case::tree(Place::Tree)]
    fn asks_before_closing_a_plain_one_and_keeps_the_selection(#[case] place: Place) {
        let mut remote = seeing(Reply::Agree).shown_in(place);
        let before = shown(&remote.rig.app);

        remote.close_at(place);

        assert_eq!(opened(&remote.rig.app), Some("close machine workspace"));
        assert_eq!((shown(&remote.rig.app), remote.rig.seen("ask")), (before, 0));
    }

    #[rstest]
    #[case::column(Place::Column)]
    #[case::tree(Place::Tree)]
    fn checks_a_worktree_with_the_machine_before_asking(#[case] place: Place) {
        let mut remote = with_worktree().shown_in(place);

        remote.close_at(place);
        remote.heard();

        assert_eq!(opened(&remote.rig.app), Some("remove machine workspace"));
        let asked = remote.asked();
        assert_eq!(asked.iter().map(|asked| asked["command"].clone()).collect::<Vec<_>>(), ["removal-check"]);
    }

    #[test]
    fn of_a_fallen_machine_says_why_and_sends_nothing() {
        let mut remote = seeing(Reply::Agree).shown_in(Place::Column);
        remote.falls();

        remote.close_at(Place::Column);

        remote.sent_nothing_while_down();
    }

    #[test]
    fn and_its_menu_do_nothing_while_the_machine_removes_it() {
        let mut remote = seeing(Reply::Agree).shown_in(Place::Column);
        remote.send(REMOVE);
        remote.heard();

        remote.close_at(Place::Column);
        remote.menu_at(Place::Column, Way::RightClick);

        assert_eq!((remote.rig.app.overlay.is_none(), remote.rig.seen("ask")), (true, 1));
    }

    #[rstest]
    #[case::its_row(TreeRow::Workspace(0, 0))]
    #[case::its_tab(TreeRow::Tab(0, 0, 0))]
    fn nor_its_rows_show_it_while_the_machine_removes_it(#[case] row: TreeRow) {
        let tabs = vec![TabInfo { id: 11, ..TabInfo::default() }];
        let mut remote = seeing_in(Reply::Agree, holding(BOOT, WorkspaceInfo { tabs, ..workspace() }));
        remote = remote.shown_in(Place::Tree);
        remote.send(REMOVE);
        remote.heard();

        remote.click_tree(row);

        assert_eq!(shown(&remote.rig.app), None);
    }
}

mod its_menu {
    use super::*;

    #[rstest]
    #[case::column_button(Place::Column, Way::Button)]
    #[case::column_right_click(Place::Column, Way::RightClick)]
    #[case::tree_button(Place::Tree, Way::Button)]
    #[case::tree_right_click(Place::Tree, Way::RightClick)]
    fn offers_to_rename_it_and_keeps_the_selection(#[case] place: Place, #[case] way: Way) {
        let mut remote = seeing(Reply::Agree).shown_in(place);
        let before = shown(&remote.rig.app);

        remote.menu_at(place, way);

        assert_eq!(
            (menu_labels(&remote.rig.app), menu_disabled(&remote.rig.app)),
            (vec!["rename workspace".into()], vec![false])
        );
        assert_eq!(shown(&remote.rig.app), before);
        let pos = menu_item_at(&remote.rig.app, 0);
        click(&mut remote.rig.app, pos);
        assert_eq!(opened(&remote.rig.app), Some("rename machine workspace"));
    }

    #[test]
    fn of_a_fallen_machine_says_why_and_greys_its_rename() {
        let mut remote = seeing(Reply::Agree).shown_in(Place::Column);
        remote.falls();

        remote.menu_at(Place::Column, Way::Button);

        let labels: Vec<String> = vec!["vps unreachable".into(), "rename workspace".into()];
        assert_eq!((menu_labels(&remote.rig.app), menu_disabled(&remote.rig.app)), (labels, vec![true, true]));
        let pos = menu_item_at(&remote.rig.app, 1);
        click(&mut remote.rig.app, pos);
        assert!(remote.rig.app.overlay.is_none());
    }

    #[test]
    fn greys_its_rename_while_an_action_on_it_waits() {
        let mut remote = seeing(Reply::Agree).shown_in(Place::Column);
        remote.send(Act::Rename("web".into()));

        remote.menu_at(Place::Column, Way::Button);

        assert_eq!(menu_disabled(&remote.rig.app), vec![true]);
        let pos = menu_item_at(&remote.rig.app, 0);
        click(&mut remote.rig.app, pos);
        assert!(remote.rig.app.overlay.is_none());
        remote.heard();
    }

    #[test]
    fn says_nothing_was_sent_once_the_machine_restarted() {
        let mut remote = seeing(Reply::Agree).shown_in(Place::Column);
        remote.menu_at(Place::Column, Way::Button);

        remote.restarted();
        let pos = menu_item_at(&remote.rig.app, 0);
        click(&mut remote.rig.app, pos);

        assert_eq!(opened(&remote.rig.app), None);
        assert_eq!(toast(&remote.rig.app), Some("vps restarted; nothing was sent"));
    }
}
