use super::*;

#[test]
fn a_project_offers_rename_project() {
    let (mut app, _rx, _dirs) = app_with(1);
    right_click(&mut app, entry_pos());
    assert_eq!(menu_labels(&app), ["rename project", "copy path"]);
}

#[test]
fn a_workspace_offers_rename_workspace() {
    let (mut app, _rx, _dirs) = app_with(1);
    right_click_row(&mut app, WorkspaceRow::Workspace(0));
    assert_eq!(menu_labels(&app), ["rename workspace", "copy path"]);
}

#[test]
fn a_tab_offers_rename_tab() {
    let (mut app, _rx, _dirs) = app_with(1);
    right_click_row(&mut app, WorkspaceRow::Tab(0, 0));
    assert_eq!(menu_labels(&app), ["rename tab"]);
}

#[test]
fn the_menu_button_of_a_project_opens_its_menu() {
    let (mut app, _rx, _dirs) = app_with(1);
    let row = ui::entry_row(list(), 1, &app.sidebar_rows(), 0, SidebarRow::Project(0));
    click(&mut app, ui::row_menu_button(row, 1).as_position());
    assert_eq!(menu_labels(&app), ["rename project", "copy path"]);
}

#[rstest::rstest]
#[case::a_workspace(WorkspaceRow::Workspace(0), vec!["rename workspace", "copy path"])]
#[case::a_tab(WorkspaceRow::Tab(0, 0), vec!["rename tab"])]
fn the_menu_button_of_a_row_opens_its_menu(#[case] row: WorkspaceRow, #[case] labels: Vec<&str>) {
    let (mut app, _rx, _dirs) = app_with(1);
    let menu = ui::row_menu_button(row_rect(&app, row), areas().pitch);
    click(&mut app, menu.as_position());
    assert_eq!(menu_labels(&app), labels);
}

#[test]
fn clicking_elsewhere_closes_it_without_acting() {
    let (mut app, _rx, _dirs) = app_with(1);
    right_click(&mut app, entry_pos());

    click(&mut app, ui::new_project_button(list(), 1, &plain(1)).as_position());

    assert_eq!((app.overlay.is_none(), app.projects.len()), (true, 1));
}

#[test]
fn esc_closes_it() {
    let (mut app, _rx, _dirs) = app_with(1);
    right_click(&mut app, entry_pos());

    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    assert!(app.overlay.is_none());
}

mod marking {
    use super::*;
    use crate::agents::activity::{Activity, Status};

    fn with_claude() -> (App, Receiver<AppEvent>, Vec<TempDir>) {
        let (mut app, rx, dirs) = app_with(1);
        let term = app.term_mut().expect("a pane");
        term.agent.follow(Some(agents::Known::Claude.name()));
        term.agent.update(Some(Activity::Idle), true, Instant::now());
        (app, rx, dirs)
    }

    fn status(app: &App) -> Option<Status> {
        app.term().and_then(|t| t.agent.status())
    }

    #[test]
    fn a_tab_running_an_agent_offers_marking_it_and_then_reviewing_it() {
        let (mut app, _rx, _dirs) = with_claude();
        right_click_row(&mut app, WorkspaceRow::Tab(0, 0));
        assert_eq!(menu_labels(&app), ["rename tab", "mark as pending review"]);
        pick(&mut app, "mark as pending review");
        let marked = status(&app);

        right_click_row(&mut app, WorkspaceRow::Tab(0, 0));
        assert_eq!(menu_labels(&app), ["rename tab", "mark as reviewed"]);
        pick(&mut app, "mark as reviewed");

        assert_eq!((marked, status(&app)), (Some(Status::Pending), Some(Status::Idle)));
    }

    #[test]
    fn the_menu_of_an_agents_pane_offers_marking_it() {
        let (mut app, _rx, _dirs) = with_claude();
        let pane = areas().pane;
        right_click(&mut app, Position::new(pane.x + 1, pane.y + 1));
        pick(&mut app, "mark as pending review");

        assert_eq!(status(&app), Some(Status::Pending));
    }

    #[test]
    fn an_agents_row_in_the_agents_section_offers_marking_its_pane() {
        let (mut app, _rx, _dirs) = with_claude();
        right_click_first_agent(&mut app);
        assert_eq!(menu_labels(&app), ["mark as pending review", "close agent"]);
        pick(&mut app, "mark as pending review");

        assert_eq!(status(&app), Some(Status::Pending));
    }
}
