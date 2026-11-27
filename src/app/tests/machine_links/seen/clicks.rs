use super::*;
use crate::app::menus::MenuAction;
use crate::ui::{SidebarRow, WorkspaceRow};

fn marked(app: &App) -> (usize, Option<usize>, usize, bool) {
    let view = app.lists_view(AREA, Instant::now());
    (view.active_workspace, view.active_tab, view.active, view.remote_project)
}

fn listed(app: &App) -> Vec<(String, Option<String>)> {
    let view = app.lists_view(AREA, Instant::now());
    view.workspaces.iter().map(|w| (w.name.clone(), w.machine.as_ref().map(|m| m.name.clone()))).collect()
}

fn click_project(rig: &mut Rig, p: usize) {
    let pos = sidebar_pos(&rig.app, SidebarRow::Project(p));
    click(&mut rig.app, pos);
}

fn arrow(app: &App, w: usize) -> Position {
    let r = row_rect(app, WorkspaceRow::Workspace(w));
    Position::new(r.x + 2, r.y)
}

fn add_workspaces(app: &mut App, names: &[&str]) {
    let path = app.projects[0].path.clone();
    for name in names {
        let workspace = app.new_workspace(AREA, path.clone(), Some((*name).into()), false).expect("workspace");
        app.projects[0].workspaces.push(workspace);
    }
}

fn our_ids(app: &App) -> Vec<u64> {
    app.projects[0].workspaces.iter().map(|w| w.id).collect()
}

mod a_machine_s_row {
    use super::*;

    #[test]
    fn picks_its_workspace_and_marks_its_row() {
        let Seeing { mut rig, .. } = seeing_two();
        let w = theirs_at(&rig.app);

        click_row(&mut rig.app, WorkspaceRow::Workspace(w));

        assert_eq!(picked(&rig.app), Some((10, Some(13))));
        assert_eq!(marked(&rig.app), (w, Some(1), 0, false));
    }

    #[test]
    fn picks_the_tab_clicked() {
        let Seeing { mut rig, .. } = seeing_two();
        let w = theirs_at(&rig.app);

        click_row(&mut rig.app, WorkspaceRow::Tab(w, 0));

        assert_eq!(picked(&rig.app), Some((10, Some(11))));
        assert_eq!(marked(&rig.app), (w, Some(0), 0, false));
    }

    #[test]
    fn opens_its_tab_s_menu_on_a_right_click_instead_of_picking_it() {
        let Seeing { mut rig, .. } = seeing_two();
        let w = theirs_at(&rig.app);

        right_click_row(&mut rig.app, WorkspaceRow::Tab(w, 0));

        let Some(Overlay::Menu { actions, .. }) = &rig.app.overlay else { panic!("a menu, not {:?}", rig.app.overlay) };
        let tabs: Vec<u64> = actions
            .iter()
            .filter_map(|action| match action {
                MenuAction::RemoteTabRename(at) | MenuAction::RemoteTabClose(at) => Some(at.tab),
                _ => None,
            })
            .collect();
        assert_eq!((tabs, picked(&rig.app)), (vec![11, 11], None));
    }

    #[test]
    fn dragged_onto_a_local_row_does_nothing() {
        let Seeing { mut rig, .. } = seeing_two();
        let w = theirs_at(&rig.app);
        let before = listed(&rig.app);

        let (from, to) = (row_pos(&rig.app, WorkspaceRow::Workspace(w)), row_pos(&rig.app, WorkspaceRow::Workspace(0)));
        drag(&mut rig.app, from, to);

        assert_eq!((listed(&rig.app), picked(&rig.app)), (before, None));
    }

    #[test]
    fn takes_a_local_workspace_dropped_on_it_to_the_end_of_this_computer_s_own() {
        let Seeing { mut rig, _dir, .. } = seeing_two();
        add_workspaces(&mut rig.app, &["feature"]);
        let (before, w) = (our_ids(&rig.app), theirs_at(&rig.app));

        let (from, to) = (row_pos(&rig.app, WorkspaceRow::Workspace(0)), row_pos(&rig.app, WorkspaceRow::Tab(w, 1)));
        drag(&mut rig.app, from, to);

        assert_eq!(our_ids(&rig.app), [before[1], before[0]]);
    }

    #[test]
    fn lets_a_local_workspace_dropped_on_ours_land_where_it_points_while_scrolling_the_column() {
        let Seeing { mut rig, mut theirs, _dir, .. } = seeing_two();
        add_workspaces(&mut rig.app, &["feature", "fix"]);
        let mut many = worked(report(&[("web-shop", Some(SHOP)), ("blog", None)]));
        let main = many.projects[0].workspaces[0].clone();
        many.projects[0].workspaces = (0..6).map(|n| WorkspaceInfo { id: 100 + n, ..main.clone() }).collect();
        serve(&mut theirs, 2, many);
        rig.until("their six workspaces are listed", |app| listed(app).len() == 9);
        let rows = ui::workspace_rows(&rig.app.tab_lines());
        let second = rows.iter().position(|row| *row == WorkspaceRow::Workspace(1)).expect("our second workspace");
        rig.app.scroll.set(List::Workspaces, second);
        let before = our_ids(&rig.app);

        let (from, to) = (row_pos(&rig.app, WorkspaceRow::Workspace(2)), row_pos(&rig.app, WorkspaceRow::Workspace(1)));
        drag(&mut rig.app, from, to);

        assert_eq!(our_ids(&rig.app), [before[0], before[2], before[1]]);
    }

    #[test]
    fn folds_here_with_its_arrow_and_stays_folded_in_the_next_snapshot() {
        let Seeing { mut rig, mut theirs, .. } = seeing_two();
        let w = theirs_at(&rig.app);

        let pos = arrow(&rig.app, w);
        click(&mut rig.app, pos);
        serve(&mut theirs, 2, worked(report(&[("web-shop", Some(SHOP)), ("blog", None), ("docs", None)])));
        seeing(&mut rig, &["web-shop", "blog", "docs"]);

        let rows = ui::workspace_rows(&rig.app.tab_lines());
        assert!(!rows.iter().any(|row| matches!(row, WorkspaceRow::Tab(at, _) if *at == w)), "{rows:?}");
        assert_eq!(picked(&rig.app), None);
        let pos = arrow(&rig.app, w);
        click(&mut rig.app, pos);
        assert!(ui::workspace_rows(&rig.app.tab_lines()).contains(&WorkspaceRow::Tab(w, 1)));
    }
}

mod a_project_only_the_machine_has {
    use super::*;

    #[test]
    fn lists_only_its_workspaces_and_picks_the_first() {
        let Seeing { mut rig, .. } = seeing_two();

        click_project(&mut rig, 1);

        assert_eq!(picked(&rig.app), Some((20, Some(23))));
        assert_eq!(listed(&rig.app), [("main".to_string(), Some("vps".to_string()))]);
        assert_eq!((marked(&rig.app), rig.app.active), ((0, Some(1), 1, true), 0));
    }

    #[test]
    fn takes_no_local_click_in_its_column() {
        let Seeing { mut rig, .. } = seeing_two();
        let workspaces = rig.app.project().map(|p| p.workspaces.len());
        click_project(&mut rig, 1);

        let pos = new_workspace_pos(&rig.app);
        click(&mut rig.app, pos);

        assert_eq!((rig.app.overlay.is_none(), rig.app.project().map(|p| p.workspaces.len())), (true, workspaces));
    }

    #[test]
    fn gives_way_to_a_local_project_clicked() {
        let Seeing { mut rig, .. } = seeing_two();
        click_project(&mut rig, 1);

        click_project(&mut rig, 0);

        assert_eq!((picked(&rig.app), marked(&rig.app).3), (None, false));
        assert_eq!(listed(&rig.app).first().map(|(_, machine)| machine.clone()), Some(None));
    }
}

#[test]
fn a_local_row_clicked_takes_the_selection_back() {
    let Seeing { mut rig, .. } = seeing_two();
    let w = theirs_at(&rig.app);
    click_row(&mut rig.app, WorkspaceRow::Workspace(w));

    click_row(&mut rig.app, WorkspaceRow::Workspace(0));

    assert_eq!((picked(&rig.app), marked(&rig.app).0), (None, 0));
}

mod the_tab_bar {
    use super::*;
    use crate::app::machine_links::RemoteItem;
    use crate::config;

    fn on_top(rig: &mut Rig) {
        rig.app.config.tabs = config::Tabs::Top.id().into();
        render(&mut rig.app, AREA);
    }

    fn tab_ids(app: &App) -> Vec<u64> {
        app.projects[0].workspaces[0].tabs.iter().map(|tab| tab.id).collect()
    }

    #[test]
    fn goes_back_to_this_computer_s_tabs() {
        let Seeing { mut rig, .. } = seeing_two();
        on_top(&mut rig);
        let w = theirs_at(&rig.app);
        click_row(&mut rig.app, WorkspaceRow::Workspace(w));
        render(&mut rig.app, AREA);
        assert_eq!(rig.app.tab_bar_view().machine.map(|machine| machine.name), Some("vps".into()));

        click_row(&mut rig.app, WorkspaceRow::Workspace(0));
        render(&mut rig.app, AREA);

        let bar = rig.app.tab_bar_view();
        assert_eq!((bar.machine, bar.tabs.len(), rig.app.scroll.of(List::TabBar)), (None, 1, 0));
    }

    #[test]
    fn never_moves_a_tab_of_ours_into_a_machine_s_strip() {
        let Seeing { mut rig, .. } = seeing_two();
        for _ in 0..2 {
            rig.app.add_tab(workspace_at(&rig.app, 0, 0), AREA).expect("add a tab");
        }
        on_top(&mut rig);
        let before = tab_ids(&rig.app);
        let from = rig.app.tab_strip(&rig.app.layout(AREA)).item(0).as_position();
        press(&mut rig.app, from);

        assert!(rig.app.select_remote("vps", RemoteItem::Workspace(10)));
        render(&mut rig.app, AREA);
        let to = rig.app.tab_strip(&rig.app.layout(AREA)).item(1).as_position();
        mouse(&mut rig.app, MouseEventKind::Drag(MouseButton::Left), to);
        mouse(&mut rig.app, MouseEventKind::Up(MouseButton::Left), to);

        assert_eq!(tab_ids(&rig.app), before);
    }
}
