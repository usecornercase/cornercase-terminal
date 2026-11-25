use super::*;
use crate::config;
use crate::control::TabInfo;
use crate::state;
use crate::ui::{SidebarRow, TreeRow, WorkspaceRow};

#[derive(Clone, Copy)]
pub(super) enum Place {
    Column,
    Tree,
}

#[derive(Clone, Copy)]
enum Way {
    Button,
    RightClick,
}

#[derive(Clone, Copy)]
enum Spot {
    Workspace,
    Tab(usize),
    NewTab,
}

fn with_worktree() -> Remote {
    let found = || json!({ "workspace": WORKSPACE, "path": PATH });
    seeing_in(Reply::Found(found), holding(BOOT, WorkspaceInfo { worktree: true, ..workspace() }))
}

pub(super) fn opened(app: &App) -> Option<&'static str> {
    match &app.overlay {
        Some(Overlay::MachineWorkspace(dialog)) => Some(dialog.name()),
        _ => None,
    }
}

fn shown(app: &App) -> Option<u64> {
    app.shown_remote().map(|shown| shown.workspace)
}

impl Remote {
    pub(super) fn shown_in(mut self, place: Place) -> Self {
        match place {
            Place::Column => {
                self.rig.app.config.sidebar = config::Sidebar::SideBySide.id().into();
                let pos = sidebar_pos(&self.rig.app, SidebarRow::Project(0));
                click(&mut self.rig.app, pos);
            }
            Place::Tree => self.rig.app.config.sidebar = config::Sidebar::Tree.id().into(),
        }
        render(&mut self.rig.app, AREA);
        self
    }

    fn row_at(&self, place: Place, spot: Spot) -> (Rect, u16) {
        let app = &self.rig.app;
        match place {
            Place::Column => {
                let areas = app_areas(app);
                let scroll = app.scroll.of(List::Workspaces);
                let row = match spot {
                    Spot::Workspace => WorkspaceRow::Workspace(0),
                    Spot::Tab(t) => WorkspaceRow::Tab(0, t),
                    Spot::NewTab => WorkspaceRow::NewTab(0),
                };
                (ui::workspace_row(areas.workspaces_list, areas.pitch, &app.tab_lines(), scroll, row), areas.pitch)
            }
            Place::Tree => {
                let list = ui::layout_with(AREA, state::Widths::default(), false, config::Sidebar::Tree).list;
                let row = match spot {
                    Spot::Workspace => TreeRow::Workspace(0, 0),
                    Spot::Tab(t) => TreeRow::Tab(0, 0, t),
                    Spot::NewTab => TreeRow::NewTab(0, 0),
                };
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
        self.close_on(place, Spot::Workspace);
    }

    fn close_on(&mut self, place: Place, spot: Spot) {
        let (row, pitch) = self.row_at(place, spot);
        click(&mut self.rig.app, ui::row_close_button(row, pitch).as_position());
    }

    fn new_tab_at(&mut self, place: Place) {
        let (row, _) = self.row_at(place, Spot::NewTab);
        click(&mut self.rig.app, Position::new(row.x + 6, row.y));
    }

    fn menu_at(&mut self, place: Place, way: Way) {
        self.menu_of(place, way, Spot::Workspace);
    }

    fn menu_of(&mut self, place: Place, way: Way, spot: Spot) {
        let (row, pitch) = self.row_at(place, spot);
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
            (vec!["rename workspace".into(), "ports".into()], vec![false, false])
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

        let labels: Vec<String> = vec!["vps unreachable".into(), "rename workspace".into(), "ports".into()];
        assert_eq!((menu_labels(&remote.rig.app), menu_disabled(&remote.rig.app)), (labels, vec![true, true, true]));
        let pos = menu_item_at(&remote.rig.app, 1);
        click(&mut remote.rig.app, pos);
        assert!(remote.rig.app.overlay.is_none());
    }

    #[test]
    fn greys_its_rename_while_an_action_on_it_waits() {
        let mut remote = seeing(Reply::Agree).shown_in(Place::Column);
        remote.send(Act::Rename("web".into()));

        remote.menu_at(Place::Column, Way::Button);

        assert_eq!(menu_disabled(&remote.rig.app), vec![true, false]);
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

mod the_new_tab_row {
    use super::*;

    #[rstest]
    #[case::column(Place::Column)]
    #[case::tree(Place::Tree)]
    fn opens_a_tab_in_the_workspace_there_with_the_boot_it_was_seen_with(#[case] place: Place) {
        let mut remote = with_tabs(Reply::Made(made_tab)).shown_in(place);

        remote.new_tab_at(place);
        remote.heard();

        let asked = remote.asked();
        let [asked] = &asked[..] else { panic!("one question, not {asked:?}") };
        assert_eq!((asked["command"].as_str(), asked["boot"].as_str()), (Some("new-tab"), Some(BOOT)));
        assert_eq!(asked["args"]["workspace"], WORKSPACE);
    }

    #[test]
    fn picks_the_new_tab_once_the_machine_lists_it() {
        let mut remote = with_tabs(Reply::Made(made_tab)).shown_in(Place::Column);

        remote.new_tab_at(Place::Column);
        remote.heard();
        assert_ne!(remote.rig.app.shown_remote().and_then(|shown| shown.tab), Some(MADE));
        serve(&mut remote.theirs, 2, listing_the_made_tab());

        remote.rig.until("the new tab shows", |app| app.shown_remote().and_then(|shown| shown.tab) == Some(MADE));
    }

    #[test]
    fn picks_nothing_when_something_else_was_picked_meanwhile() {
        let mut remote = with_tabs(Reply::Made(made_tab)).shown_in(Place::Column);

        remote.new_tab_at(Place::Column);
        let (row, _) = remote.row_at(Place::Column, Spot::Tab(1));
        click(&mut remote.rig.app, Position::new(row.x + 8, row.y));
        remote.heard();
        serve(&mut remote.theirs, 2, listing_the_made_tab());
        remote.rig.until("the new tab is listed", |app| {
            app.machines.held()[0].seen.as_ref().is_some_and(|seen| seen.projects[0].workspaces[0].tabs.len() == 3)
        });

        assert_eq!(remote.rig.app.shown_remote().and_then(|shown| shown.tab), Some(AGENT));
    }

    #[test]
    fn closes_the_compact_menu_as_ours_does() {
        let mut remote = with_tabs(Reply::Made(made_tab)).shown_in(Place::Column);
        let app = &mut remote.rig.app;
        app.screen.nav = Some(ui::Nav::Workspaces);
        let (list, tabs) = (Rect { height: 30, ..app_areas(app).workspaces_list }, app.tab_lines());
        let row = ui::workspace_row(list, 3, &tabs, app.scroll.of(List::Workspaces), WorkspaceRow::NewTab(0));

        assert!(app.press_remote_row(list, 3, &tabs, (Position::new(row.x + 6, row.y), AREA)));

        assert_eq!(app.screen.nav, None);
        remote.heard();
        assert_eq!(remote.asked()[0]["command"], "new-tab");
    }

    #[test]
    fn of_a_fallen_machine_says_why_and_sends_nothing() {
        let mut remote = with_tabs(Reply::Agree).shown_in(Place::Column);
        remote.falls();

        remote.new_tab_at(Place::Column);

        remote.sent_nothing_while_down();
    }

    #[test]
    fn opens_no_menu_on_a_right_click() {
        let mut remote = with_tabs(Reply::Agree).shown_in(Place::Column);

        remote.menu_of(Place::Column, Way::RightClick, Spot::NewTab);

        assert_eq!((remote.rig.app.overlay.is_none(), remote.rig.seen("ask")), (true, 0));
    }
}

mod a_tab_s_close_button {
    use super::*;

    #[rstest]
    #[case::column(Place::Column)]
    #[case::tree(Place::Tree)]
    fn closes_a_shell_at_once_and_reads_closing_until_the_machine_drops_it(#[case] place: Place) {
        let mut remote = with_tabs(Reply::Agree).shown_in(place);
        let before = shown(&remote.rig.app);

        remote.close_on(place, Spot::Tab(0));
        assert_eq!((remote.rig.app.overlay.is_none(), remote.closing()), (true, vec![true, false]));
        remote.heard();

        let asked = remote.asked();
        assert_eq!(
            (asked[0]["command"].as_str(), &asked[0]["args"]["item"]),
            (Some("close"), &json!({ "tab": SHELL }))
        );
        assert_eq!((remote.closing(), shown(&remote.rig.app)), (vec![true, false], before));
    }

    #[rstest]
    #[case::column(Place::Column)]
    #[case::tree(Place::Tree)]
    fn asks_first_for_a_tab_with_an_agent(#[case] place: Place) {
        let mut remote = with_tabs(Reply::Agree).shown_in(place);

        remote.close_on(place, Spot::Tab(1));

        assert_eq!(opened(&remote.rig.app), Some("close machine tab"));
        assert_eq!(remote.rig.seen("ask"), 0);
    }

    #[test]
    fn asks_first_in_the_compact_layout() {
        let mut remote = with_tabs(Reply::Agree).shown_in(Place::Column);
        let app = &mut remote.rig.app;
        let (list, tabs) = (app_areas(app).workspaces_list, app.tab_lines());
        let row = ui::workspace_row(list, 3, &tabs, app.scroll.of(List::Workspaces), WorkspaceRow::Tab(0, 0));

        assert!(app.press_remote_row(list, 3, &tabs, (ui::row_close_button(row, 3).as_position(), AREA)));

        assert_eq!(opened(app), Some("close machine tab"));
        assert_eq!(remote.rig.seen("ask"), 0);
    }

    #[test]
    fn of_a_fallen_machine_says_why_and_sends_nothing() {
        let mut remote = with_tabs(Reply::Agree).shown_in(Place::Column);
        remote.falls();

        remote.close_on(Place::Column, Spot::Tab(0));

        remote.sent_nothing_while_down();
    }

    #[test]
    fn and_its_menu_take_no_click_while_it_closes() {
        let mut remote = with_tabs(Reply::Agree).shown_in(Place::Column);
        remote.close_on(Place::Column, Spot::Tab(0));
        remote.heard();
        let before = shown(&remote.rig.app);

        remote.close_on(Place::Column, Spot::Tab(0));
        remote.menu_of(Place::Column, Way::RightClick, Spot::Tab(0));

        assert_eq!((remote.rig.app.overlay.is_none(), remote.rig.seen("ask")), (true, 1));
        assert_eq!(shown(&remote.rig.app), before);
    }
}

mod a_tab_s_menu {
    use super::*;

    #[rstest]
    #[case::column_button(Place::Column, Way::Button)]
    #[case::column_right_click(Place::Column, Way::RightClick)]
    #[case::tree_button(Place::Tree, Way::Button)]
    #[case::tree_right_click(Place::Tree, Way::RightClick)]
    fn offers_to_rename_and_close_it_and_keeps_the_selection(#[case] place: Place, #[case] way: Way) {
        let mut remote = with_tabs(Reply::Agree).shown_in(place);
        let before = shown(&remote.rig.app);

        remote.menu_of(place, way, Spot::Tab(1));

        let labels: Vec<String> = vec!["rename tab".into(), "close tab".into()];
        assert_eq!((menu_labels(&remote.rig.app), menu_disabled(&remote.rig.app)), (labels, vec![false, false]));
        assert_eq!(shown(&remote.rig.app), before);
        let pos = menu_item_at(&remote.rig.app, 0);
        click(&mut remote.rig.app, pos);
        assert_eq!(opened(&remote.rig.app), Some("rename machine tab"));
    }

    #[rstest]
    #[case::a_shell(Spot::Tab(0), None, 1)]
    #[case::an_agent(Spot::Tab(1), Some("close machine tab"), 0)]
    fn closes_it_as_its_close_button_does_in_the_wide_layout(
        #[case] spot: Spot,
        #[case] dialog: Option<&str>,
        #[case] asked: usize,
    ) {
        let mut remote = with_tabs(Reply::Agree).shown_in(Place::Column);
        remote.menu_of(Place::Column, Way::Button, spot);

        let pos = menu_item_at(&remote.rig.app, 1);
        click(&mut remote.rig.app, pos);
        if asked > 0 {
            remote.heard();
        }

        assert_eq!((opened(&remote.rig.app), remote.asked().len()), (dialog, asked));
    }

    #[test]
    fn of_a_fallen_machine_says_why_and_greys_both() {
        let mut remote = with_tabs(Reply::Agree).shown_in(Place::Column);
        remote.falls();

        remote.menu_of(Place::Column, Way::Button, Spot::Tab(0));

        let labels: Vec<String> = vec!["vps unreachable".into(), "rename tab".into(), "close tab".into()];
        assert_eq!((menu_labels(&remote.rig.app), menu_disabled(&remote.rig.app)), (labels, vec![true, true, true]));
        let pos = menu_item_at(&remote.rig.app, 2);
        click(&mut remote.rig.app, pos);
        assert_eq!((remote.rig.app.overlay.is_none(), remote.rig.seen("ask")), (true, 0));
    }

    #[test]
    fn greys_both_while_an_action_on_the_tab_waits_but_not_its_workspace_s_rename() {
        let mut remote = with_tabs(Reply::Agree).shown_in(Place::Column);
        remote.send(Act::RenameTab { tab: SHELL, name: "logs".into() });

        remote.menu_of(Place::Column, Way::Button, Spot::Tab(0));
        assert_eq!(menu_disabled(&remote.rig.app), vec![true, true]);
        let pos = menu_item_at(&remote.rig.app, 0);
        click(&mut remote.rig.app, pos);
        assert!(remote.rig.app.overlay.is_none());
        remote.menu_at(Place::Column, Way::Button);

        let labels: Vec<String> = vec!["rename workspace".into(), "ports".into()];
        assert_eq!((menu_labels(&remote.rig.app), menu_disabled(&remote.rig.app)), (labels, vec![false, false]));
        remote.heard();
    }
}
