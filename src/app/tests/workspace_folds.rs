use super::*;
use crate::agents::activity::Status;
use crate::agents::claude::fake::FakeClaude;

fn with_two_workspaces() -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, dirs) = app_with(1);
    let path = app.projects[0].path.clone();
    let workspace = app.new_workspace(AREA, path, Some("second".into()), false).expect("workspace");
    app.projects[0].workspaces.push(workspace);
    render(&mut app, AREA);
    (app, rx, dirs)
}

fn arrow_pos(app: &App, w: usize) -> Position {
    let r = row_rect(app, WorkspaceRow::Workspace(w));
    Position::new(r.x + 2, r.y)
}

fn click_arrow(app: &mut App, w: usize) {
    let pos = arrow_pos(app, w);
    click(app, pos);
}

fn right_click_arrow(app: &mut App, w: usize) {
    let pos = arrow_pos(app, w);
    right_click(app, pos);
}

fn pane_ids(app: &App) -> Vec<u64> {
    app.projects[0].workspaces[0].tabs.iter().flat_map(|t| t.panes.iter().map(|p| p.id)).collect()
}

#[test]
fn the_arrow_collapses_and_expands_without_stopping_anything() {
    let (mut app, _rx, _dirs) = with_two_workspaces();
    let panes = pane_ids(&app);

    click_arrow(&mut app, 0);
    assert!(app.projects[0].workspaces[0].collapsed);
    let rows = ui::workspace_rows(&app.tab_lines());
    assert!(!rows.iter().any(|r| matches!(r, WorkspaceRow::Tab(0, _) | WorkspaceRow::NewTab(0))));
    assert_eq!(pane_ids(&app), panes);

    click_arrow(&mut app, 0);
    assert!(!app.projects[0].workspaces[0].collapsed);
    assert!(ui::workspace_rows(&app.tab_lines()).contains(&WorkspaceRow::Tab(0, 0)));
}

#[test]
fn clicking_the_name_of_a_collapsed_workspace_activates_and_expands_it() {
    let (mut app, _rx, _dirs) = with_two_workspaces();
    click_arrow(&mut app, 0);
    click_row(&mut app, WorkspaceRow::Workspace(1));
    assert_eq!(app.projects[0].active, 1);
    assert!(app.projects[0].workspaces[0].collapsed);

    click_row(&mut app, WorkspaceRow::Workspace(0));
    assert_eq!(app.projects[0].active, 0);
    assert!(!app.projects[0].workspaces[0].collapsed);
}

#[test]
fn dragging_from_the_arrow_moves_the_row_and_does_not_fold() {
    let (mut app, _rx, _dirs) = with_two_workspaces();
    let to = row_pos(&app, WorkspaceRow::Tab(1, 0));
    let from = arrow_pos(&app, 0);
    drag(&mut app, from, to);
    assert!(app.projects[0].workspaces.iter().all(|w| !w.collapsed));
}

#[test]
fn a_right_click_on_the_arrow_opens_the_workspace_menu() {
    let (mut app, _rx, _dirs) = with_two_workspaces();
    right_click_arrow(&mut app, 0);
    assert!(matches!(app.overlay, Some(Overlay::Menu { .. })));
}

#[test]
fn a_workspace_being_removed_ignores_the_arrow() {
    let (mut app, _rx, _dirs) = with_two_workspaces();
    app.projects[0].workspaces[1].phase = Phase::Removing("second".into());
    click_arrow(&mut app, 1);
    assert!(!app.projects[0].workspaces[1].collapsed);
}

#[test]
fn folds_are_saved() {
    let (mut app, _rx, _dirs) = with_two_workspaces();
    click_arrow(&mut app, 0);
    let saved: Vec<bool> = app.state().projects[0].workspaces.iter().map(|w| w.collapsed).collect();
    assert_eq!(saved, [true, false]);
}

#[test]
fn a_collapsed_row_follows_its_agents() {
    let (mut app, rx, _dirs) = app_with(1);
    let claude = FakeClaude::new();
    app.agents.claude_dir = Some(claude.dir().to_path_buf());
    type_line(&mut app, &claude.command_line(""));
    let shown = |app: &mut App| {
        let r = row_rect(app, WorkspaceRow::Workspace(0));
        let t = render(app, AREA);
        (r.x..r.right()).map(|x| t.backend().buffer()[(x, r.y)].symbol().to_string()).collect::<String>()
    };
    click_arrow(&mut app, 0);
    assert!(app.projects[0].workspaces[0].collapsed);
    wait_until("claude works", || {
        drain(&mut app, &rx);
        let now = app.agents.watched.map_or_else(Instant::now, |at| at + WATCH_AGENTS_EVERY);
        app.watch_agents(now);
        app.projects[0].workspaces[0].tabs[0].status() == Some(Status::Working)
    });
    assert!(shown(&mut app).contains('◐'));
}
