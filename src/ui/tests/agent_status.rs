use super::*;

fn tab(name: &str, status: Option<Status>) -> TabEntry {
    TabEntry { status, ..name.into() }
}

fn with_agents() -> View<'static> {
    let mut v = View {
        has_project: true,
        workspaces: vec![
            WorkspaceEntry {
                shared: 0,
                collapsed: false,
                name: "login".into(),
                tabs: vec![tab("claude", Some(Status::Working)), tab("claude", Some(Status::Waiting)), "nvim".into()],
                behind: 2,
                pull: None,
                removing: false,
                setting_up: false,
                closed: 0,
                machine: None,
            },
            WorkspaceEntry {
                shared: 0,
                collapsed: false,
                name: "main".into(),
                tabs: vec![tab("claude", Some(Status::Done)), tab("claude", Some(Status::Idle))],
                behind: 0,
                pull: None,
                removing: false,
                setting_up: false,
                closed: 0,
                machine: None,
            },
        ],
        active_tab: Some(0),
        ..view(&["shop", "api", "web", "docs"])
    };
    v.groups = vec![GroupEntry { name: "work".into(), icon: '●', colour: 4, collapsed: true }];
    for (p, status) in [(0, Status::Waiting), (1, Status::Done), (3, Status::Waiting)] {
        v.projects[p].status = Some(status);
    }
    for p in [2, 3] {
        v.projects[p].group = Some(0);
    }
    v
}

fn tab_row(v: &View, w: usize, t: usize) -> Rect {
    workspace_row(areas().workspaces_list, areas().pitch, &v.tab_lines(), 0, WorkspaceRow::Tab(w, t))
}

fn workspace_line(v: &View, w: usize) -> String {
    let r = workspace_row(areas().workspaces_list, areas().pitch, &v.tab_lines(), 0, WorkspaceRow::Workspace(w));
    row_text(&render(v), r).trim_end().to_string()
}

fn sidebar_row(v: &View, row: SidebarRow) -> Rect {
    let sidebar = v.sidebar_rows();
    let i = sidebar.iter().position(|r| *r == row).expect("the row is in the sidebar");
    project_rows(list(), areas().pitch, &sidebar, 0).item(i)
}

fn mark_cell(v: &View, r: Rect) -> (String, Color) {
    let t = render(v);
    let cell = &t.backend().buffer()[(r.right() - CLOSE_BUTTON_WIDTH - MENU_BUTTON_WIDTH - 2, r.y)];
    (cell.symbol().to_string(), cell.fg)
}

#[test]
fn renders_icons_on_tabs_and_marks_on_the_rows_above() {
    insta::assert_snapshot!(render(&with_agents()).backend());
}

#[rstest]
#[case::working(Status::Working, "◐", Color::Yellow)]
#[case::waiting(Status::Waiting, "!", WAITING_COLOR)]
#[case::done(Status::Done, "✓", Color::Green)]
#[case::idle(Status::Idle, "○", Color::DarkGray)]
fn a_tab_shows_its_agent_before_the_name(#[case] status: Status, #[case] icon: &str, #[case] colour: Color) {
    let mut v = with_agents();
    v.workspaces[0].tabs[1] = tab("claude", Some(status));
    let r = tab_row(&v, 0, 1);
    let t = render(&v);
    let cell = &t.backend().buffer()[(r.x + 4, r.y)];
    assert_eq!((row_text(&t, r).trim_end().to_string(), cell.fg), (format!("  ├ {icon} claude"), colour));
}

#[rstest]
#[case::at_its_prompt(false, "›")]
#[case::running_a_program(true, "▸")]
fn a_tab_without_an_agent_shows_its_shell_before_the_name(#[case] running: bool, #[case] icon: &str) {
    let mut v = with_agents();
    v.workspaces[0].tabs[2].running = running;
    let r = tab_row(&v, 0, 2);
    let t = render(&v);
    let cell = &t.backend().buffer()[(r.x + 4, r.y)];
    assert_eq!((row_text(&t, r).trim_end().to_string(), cell.fg), (format!("  ├ {icon} nvim"), Color::DarkGray));
}

#[test]
fn a_working_icon_walks_the_spinner() {
    let mut v = with_agents();
    v.workspaces[0].tabs[1] = tab("claude", Some(Status::Working));
    let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let shown: Vec<String> = frames
        .iter()
        .enumerate()
        .map(|(i, _)| {
            v.spin = Some(u8::try_from(i).expect("a frame"));
            let r = tab_row(&v, 0, 1);
            let t = render(&v);
            t.backend().buffer()[(r.x + 4, r.y)].symbol().to_string()
        })
        .collect();
    assert_eq!(shown, frames);
}

#[test]
fn every_tab_icon_takes_one_cell() {
    let agents = [Status::Idle, Status::Working, Status::Done, Status::Waiting].map(|s| (Some(s), false));
    let widths: Vec<usize> = agents
        .into_iter()
        .chain([(None, false), (None, true)])
        .map(|(status, running)| TabEntry { status, running, ..TabEntry::from("") }.icon(Color::DarkGray, None).width())
        .collect();
    assert_eq!(widths, vec![1; 6]);
}

#[test]
fn agent_and_shell_tabs_start_their_names_in_the_same_column() {
    let mut v = with_agents();
    v.workspaces[0].tabs[1] = TabEntry { running: true, ..TabEntry::from("yarn") };
    let t = render(&v);
    let rows: Vec<String> = (0..3).map(|i| row_text(&t, tab_row(&v, 0, i)).trim_end().to_string()).collect();
    assert_eq!(rows, ["▌ ├ ◐ claude", "  ├ ▸ yarn", "  ├ › nvim"]);
}

#[test]
fn a_split_tab_shows_how_many_other_panes_it_has_at_the_end_of_its_row() {
    let mut v = with_agents();
    v.workspaces[0].tabs[2].others = 2;
    let r = tab_row(&v, 0, 2);
    let t = render(&v);
    let end = r.right() - CLOSE_BUTTON_WIDTH - MENU_BUTTON_WIDTH - 2;
    let count: String = (end - 1..=end).map(|x| t.backend().buffer()[(x, r.y)].symbol().to_string()).collect();
    let text = row_text(&t, r).trim_end().to_string();
    assert_eq!(
        (text.starts_with("  ├ › nvim "), count.as_str(), t.backend().buffer()[(end, r.y)].fg),
        (true, "+2", Color::DarkGray),
        "{text}"
    );
}

#[test]
fn a_long_name_is_cut_before_the_count_is() {
    let mut v = with_agents();
    v.workspaces[0].tabs[1] = TabEntry { others: 1, ..tab("a-very-long-program-name", Some(Status::Working)) };
    let text = row_text(&render(&v), tab_row(&v, 0, 1)).trim_end().to_string();
    assert!(text.contains('…') && text.ends_with(" +1"), "{text}");
}

#[test]
fn a_row_too_narrow_for_the_name_still_shows_the_count() {
    let v = with_agents();
    let r = Rect::new(0, 0, 14, 1);
    let band = Band { r, pitch: 1, lead: vec![Span::raw("  ├ ")], bg: Style::default() };
    let tab = TabEntry { others: 1, .."claude".into() };
    let mut t = Terminal::new(TestBackend::new(r.width, 1)).expect("test backend");
    t.draw(|f| draw_tab_band(f, &v, band, (&tab, None), false)).expect("draw");
    assert_eq!(row_text(&t, r).trim_end(), "  ├ › c +1");
}

#[test]
fn a_tab_with_one_pane_shows_no_count() {
    let v = with_agents();
    assert!(!row_text(&render(&v), tab_row(&v, 0, 2)).contains('+'));
}

#[test]
fn the_workspace_mark_comes_before_the_commits_to_pull() {
    assert_eq!(workspace_line(&with_agents(), 0), "  ▾ login      ! ↓2");
}

#[test]
fn only_what_needs_you_reaches_the_workspace() {
    let mut v = with_agents();
    v.workspaces[0].tabs = vec![tab("claude", Some(Status::Working)), tab("claude", Some(Status::Idle))];
    assert_eq!(workspace_line(&v, 0), "  ▾ login        ↓2");
}

#[test]
fn done_waits_behind_what_needs_you() {
    let mut v = with_agents();
    v.workspaces[0].tabs = vec![tab("claude", Some(Status::Done)), tab("claude", Some(Status::Waiting))];
    assert_eq!(workspace_line(&v, 0), "  ▾ login      ! ↓2");
}

#[rstest]
#[case::waiting(0, ("!", WAITING_COLOR))]
#[case::done(1, ("✓", Color::Green))]
fn a_project_shows_its_mark_at_the_end_of_the_row(#[case] p: usize, #[case] expected: (&str, Color)) {
    let v = with_agents();
    let (symbol, fg) = mark_cell(&v, sidebar_row(&v, SidebarRow::Project(p)));
    assert_eq!((symbol.as_str(), fg), expected);
}

#[test]
fn a_collapsed_group_shows_the_mark_of_its_projects() {
    let v = with_agents();
    let (symbol, fg) = mark_cell(&v, sidebar_row(&v, SidebarRow::Group(0)));
    assert_eq!((symbol.as_str(), fg), ("!", WAITING_COLOR));
}

#[test]
fn an_expanded_group_leaves_the_mark_to_its_projects() {
    let mut v = with_agents();
    v.groups[0].collapsed = false;
    v.projects[2].status = Some(Status::Waiting);
    let (group, _) = mark_cell(&v, sidebar_row(&v, SidebarRow::Group(0)));
    let (project, _) = mark_cell(&v, sidebar_row(&v, SidebarRow::Project(2)));
    assert_eq!((group.as_str(), project.as_str()), (" ", "!"));
}

#[rstest]
#[case::nothing(None, "   ≡   ")]
#[case::done_elsewhere(Some(Status::Done), "   ≡ ✓ ")]
#[case::waiting_elsewhere(Some(Status::Waiting), "   ≡ ! ")]
fn the_menu_button_says_when_another_tab_needs_you(#[case] attention: Option<Status>, #[case] expected: &str) {
    let v = View { attention, ..with_agents() };
    let mut t = Terminal::new(TestBackend::new(SMALL.width, SMALL.height)).expect("test backend");
    t.draw(|f| draw(f, &v)).expect("draw");
    assert_eq!(row_text(&t, Rect::new(0, 1, COMPACT_BUTTON_WIDTH, 1)), expected);
}
