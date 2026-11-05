use super::*;
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
    fn picks_its_tab_where_a_local_row_has_its_buttons() {
        let Seeing { mut rig, .. } = seeing_two();
        let w = theirs_at(&rig.app);

        click_close(&mut rig.app, WorkspaceRow::Tab(w, 0));
        right_click_row(&mut rig.app, WorkspaceRow::Tab(w, 0));

        assert_eq!((picked(&rig.app), rig.app.overlay.is_none()), (Some((10, Some(11))), true));
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
