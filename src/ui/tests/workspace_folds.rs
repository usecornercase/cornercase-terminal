use super::*;

fn tab(status: Option<Status>) -> TabEntry {
    TabEntry { status, .."claude".into() }
}

fn workspace(name: &str, collapsed: bool, statuses: &[Option<Status>]) -> WorkspaceEntry {
    WorkspaceEntry {
        collapsed,
        name: name.into(),
        tabs: statuses.iter().map(|s| tab(*s)).collect(),
        behind: 0,
        pull: None,
        removing: false,
        setting_up: false,
        closed: 0,
    }
}

fn folded(statuses: &[Option<Status>]) -> View<'static> {
    View {
        has_project: true,
        workspaces: vec![workspace("login", true, statuses), workspace("main", false, &[None, None])],
        active_tab: Some(0),
        active_workspace: 1,
        ..view(&["shop"])
    }
}

fn row(v: &View, w: usize) -> Rect {
    workspace_row(areas().workspaces_list, areas().pitch, &v.tab_lines(), 0, WorkspaceRow::Workspace(w))
}

fn hit(v: &View, pos: Position) -> Option<WorkspaceHit> {
    workspace_hit(areas().workspaces_list, areas().pitch, &v.tab_lines(), 0, pos)
}

#[test]
fn a_folded_workspace_lists_no_tabs() {
    let tabs = TabLines::from(vec![vec![1, 1], vec![1]]).folded(vec![true, false]);
    assert_eq!(
        workspace_rows(&tabs),
        [
            WorkspaceRow::Workspace(0),
            WorkspaceRow::Gap,
            WorkspaceRow::Workspace(1),
            WorkspaceRow::Tab(1, 0),
            WorkspaceRow::NewTab(1),
        ]
    );
}

#[test]
fn folds_are_ignored_when_tabs_are_not_listed() {
    let tabs = TabLines::new(vec![vec![1]], false).folded(vec![true]);
    assert_eq!(workspace_rows(&tabs), [WorkspaceRow::Workspace(0)]);
    assert!(!tabs.is_folded(0));
}

#[test]
fn the_arrow_folds_and_the_name_selects() {
    let v = folded(&[Some(Status::Idle)]);
    let r = row(&v, 0);
    assert_eq!(hit(&v, Position::new(r.x + 2, r.y)), Some(WorkspaceHit::Fold(0)));
    assert_eq!(hit(&v, Position::new(r.x + 6, r.y)), Some(WorkspaceHit::Workspace(0)));
}

#[test]
fn without_a_tab_list_there_is_no_fold_hit() {
    let tabs = TabLines::new(vec![vec![1]], false).folded(vec![true]);
    let r = workspace_row(areas().workspaces_list, areas().pitch, &tabs, 0, WorkspaceRow::Workspace(0));
    let hit = workspace_hit(areas().workspaces_list, areas().pitch, &tabs, 0, Position::new(r.x + 2, r.y));
    assert_eq!(hit, Some(WorkspaceHit::Workspace(0)));
}

#[rstest]
#[case::waiting(&[Some(Status::Working); 10], Some(Status::Waiting), "!", Some(WAITING_COLOR))]
#[case::failed(&[Some(Status::Failed), Some(Status::Pending)], None, "!", Some(Color::Red))]
#[case::pending(&[Some(Status::Pending), Some(Status::Done)], None, "●", None)]
#[case::done(&[Some(Status::Working), Some(Status::Done)], None, "✓", Some(Color::Green))]
#[case::working(&[Some(Status::Working)], None, "◐", Some(Color::Yellow))]
#[case::idle(&[Some(Status::Idle)], None, "○", None)]
fn a_folded_workspace_shows_its_most_urgent_agent(
    #[case] statuses: &[Option<Status>],
    #[case] extra: Option<Status>,
    #[case] icon: &str,
    #[case] colour: Option<Color>,
) {
    let mut all = statuses.to_vec();
    all.extend(extra.map(Some));
    let v = folded(&all);
    let r = row(&v, 0);
    let t = render(&v);
    let text = row_text(&t, r);
    assert!(text.contains(icon), "{text:?} has no {icon}");
    if let Some(colour) = colour {
        let x = text.chars().position(|c| c.to_string() == icon && c != 'l').expect("icon");
        let x = u16::try_from(x).expect("column");
        let cell = &t.backend().buffer()[(r.x + x, r.y)];
        assert_eq!(cell.fg, colour);
    }
}

#[test]
fn a_folded_workspace_with_working_agents_shows_the_spinner_frame() {
    let v = View { spin: Some(3), ..folded(&[Some(Status::Working)]) };
    assert!(row_text(&render(&v), row(&v, 0)).contains('⠸'));
}

#[test]
fn a_folded_workspace_with_only_shells_shows_no_icon() {
    let v = folded(&[None, None]);
    assert_eq!(row_text(&render(&v), row(&v, 0)).trim_end(), "  ▸ login");
}

#[test]
fn an_open_workspace_with_only_working_agents_shows_nothing() {
    let mut v = folded(&[Some(Status::Working)]);
    v.workspaces[0].collapsed = false;
    assert_eq!(row_text(&render(&v), row(&v, 0)).trim_end(), "  ▾ login");
}

#[test]
fn a_folded_active_workspace_takes_the_surface_and_the_rail() {
    let mut v = folded(&[Some(Status::Idle)]);
    v.active_workspace = 0;
    let r = row(&v, 0);
    let t = render(&v);
    let buf = t.backend().buffer();
    assert_eq!(buf[(r.x, r.y)].symbol(), "▌");
    assert_eq!(buf[(r.x + 8, r.y)].bg, DARK_SURFACE);
}

#[test]
fn a_folded_workspace_snapshot() {
    insta::assert_snapshot!(render(&folded(&[Some(Status::Working), Some(Status::Waiting)])).backend());
}

#[test]
fn a_folded_workspace_snapshot_in_compact_mode() {
    let v = View { nav: Some(Nav::Workspaces), ..folded(&[Some(Status::Working)]) };
    let mut t = Terminal::new(TestBackend::new(SMALL.width, SMALL.height)).expect("test backend");
    t.draw(|f| draw(f, &v)).expect("draw");
    insta::assert_snapshot!(t.backend());
}
