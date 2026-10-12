use super::*;

const TALL: Rect = Rect { x: 0, y: 0, width: W, height: 30 };

fn with_agents(sidebar: Sidebar) -> Areas {
    full_layout(TALL, Widths::default(), false, sidebar, true)
}

fn agent(status: Status, agent: &str, project: &str, workspace: Option<&str>, details: Details) -> AgentEntry {
    AgentEntry {
        status: Some(status),
        agent: agent.into(),
        project: project.into(),
        workspace: workspace.map(String::from),
        details,
        active: false,
    }
}

fn details(model: &str, percent: Option<u16>) -> Details {
    Details { model: Some(model.into()), percent, memory: None }
}

fn entries() -> Vec<AgentEntry> {
    vec![
        agent(
            Status::Waiting,
            "claude",
            "cornercase",
            Some("issue-98-remove-in-background"),
            details("Opus 5.5", Some(23)),
        ),
        AgentEntry {
            active: true,
            ..agent(Status::Working, "codex", "website", None, details("gpt-5.5", Some(41)))
        },
        agent(Status::Done, "claude", "api", Some("main"), Details::default()),
    ]
}

fn shown(sidebar: Sidebar, entries: Vec<AgentEntry>) -> View<'static> {
    let folded = ProjectShape { collapsed: true, ..ProjectShape::default() };
    let tree = TreeView {
        shape: TreeShape { groups: Vec::new(), projects: vec![folded; 3], tab_bar: false },
        workspaces: Vec::new(),
    };
    View {
        has_project: true,
        issues: true,
        active: 1,
        active_tab: Some(0),
        sidebar,
        workspaces: vec![WorkspaceEntry {
            name: "default".into(),
            tabs: vec!["codex".into()],
            behind: 0,
            removing: false,
        }],
        tree: (sidebar == Sidebar::Tree).then_some(tree),
        agents: Some(AgentsView { entries, scroll: 0 }),
        ..view(&["cornercase", "website", "api"])
    }
}

#[rstest]
#[case::side_by_side(Sidebar::SideBySide)]
#[case::projects_on_top(Sidebar::ProjectsOnTop)]
#[case::workspaces_on_top(Sidebar::WorkspacesOnTop)]
#[case::tree(Sidebar::Tree)]
fn sits_below_the_lists_and_above_the_footer(#[case] sidebar: Sidebar) {
    let a = with_agents(sidebar);
    let clear = |r: Rect| !r.intersects(a.agents) && !r.intersects(a.agents_border);
    assert_eq!(
        (clear(a.list), clear(a.workspaces_list), a.agents_title.y, a.agents.bottom()),
        (true, true, a.agents_border.bottom(), a.separator.y)
    );
}

#[rstest]
#[case::side_by_side(Sidebar::SideBySide)]
#[case::projects_on_top(Sidebar::ProjectsOnTop)]
#[case::workspaces_on_top(Sidebar::WorkspacesOnTop)]
#[case::tree(Sidebar::Tree)]
fn leaves_the_pane_as_it_was(#[case] sidebar: Sidebar) {
    assert_eq!(with_agents(sidebar).pane, layout_with(TALL, Widths::default(), false, sidebar).pane);
}

#[test]
fn takes_a_third_of_the_room_by_default() {
    assert_eq!(with_agents(Sidebar::ProjectsOnTop).agents.height, 7);
}

#[rstest]
#[case::too_tall(Some(100), 10)]
#[case::too_short(Some(1), MIN_STACK_SECTION)]
#[case::saved(Some(8), 8)]
fn keeps_its_height_within_both_lists_minimum(#[case] saved: Option<u16>, #[case] rows: u16) {
    let widths = Widths { agents: saved, ..Widths::default() };
    let a = full_layout(TALL, widths, false, Sidebar::ProjectsOnTop, true);
    assert_eq!((a.agents.height, a.list.height >= 1, a.workspaces_list.height >= 1), (rows, true, true));
}

#[test]
fn the_stack_line_cannot_be_dragged_into_it() {
    let widths = Widths::default().stacked_dragged(Border::Stack, Position::new(5, 28), TALL, true);
    let a = full_layout(TALL, widths, false, Sidebar::ProjectsOnTop, true);
    assert_eq!((widths.stack, a.workspaces_list.bottom() <= a.agents_border.y), (Some(8), true));
}

#[rstest]
#[case::side_by_side(Sidebar::SideBySide)]
#[case::stacked(Sidebar::ProjectsOnTop)]
#[case::tree(Sidebar::Tree)]
fn its_line_is_a_border(#[case] sidebar: Sidebar) {
    let line = with_agents(sidebar).agents_border;
    assert_eq!(with_agents(sidebar).border_hit(Position::new(line.x + 2, line.y)), Some(Border::Agents));
}

#[test]
fn a_double_click_on_its_line_resets_it() {
    assert_eq!(Widths { agents: Some(9), ..Widths::default() }.reset(Border::Agents).agents, None);
}

#[test]
fn a_click_on_the_second_line_of_a_row_hits_that_agent() {
    let list = with_agents(Sidebar::ProjectsOnTop).agents_list;
    let row = agent_row(list, 1, 3, 0, 1);
    assert_eq!(agent_hit(list, 1, 3, 0, Position::new(row.x + 3, row.y + 1)), Some(1));
}

#[test]
fn rows_below_the_section_are_counted() {
    let a = with_agents(Sidebar::ProjectsOnTop);
    let t = render_sized(&shown(Sidebar::ProjectsOnTop, [entries(), entries()].concat()), W, TALL.height);
    assert_eq!(row_text(&t, agent_rows(a.agents_list, 1, 6, 0).more_below()).trim(), "↓ 4 more");
}

#[test]
fn the_agent_on_screen_gets_the_rail_and_the_surface() {
    let a = with_agents(Sidebar::ProjectsOnTop);
    let row = agent_row(a.agents_list, 1, 3, 0, 1);
    let t = render_sized(&shown(Sidebar::ProjectsOnTop, entries()), W, TALL.height);
    let cell = &t.backend().buffer()[(row.x, row.y + 1)];
    assert_eq!((cell.symbol(), cell.fg, cell.bg), ("▌", Color::Cyan, DARK_SURFACE));
}

#[test]
fn a_hovered_row_is_filled() {
    let a = with_agents(Sidebar::ProjectsOnTop);
    let row = agent_row(a.agents_list, 1, 3, 0, 0);
    let v = View { hover: Some(Position::new(row.x + 4, row.y)), ..shown(Sidebar::ProjectsOnTop, entries()) };
    let t = render_sized(&v, W, TALL.height);
    assert_eq!(t.backend().buffer()[(row.right() - 2, row.y + 1)].bg, DARK_HOVER);
}

#[rstest]
#[case::wide(30, "! cornercase › issue-98-re…")]
#[case::without_the_workspace(18, "! cornercase")]
#[case::cut(12, "! corner…")]
fn a_narrow_row_drops_the_workspace_then_cuts_the_project(#[case] width: u16, #[case] expected: &str) {
    let r = Rect::new(0, 0, width, 2);
    let mut t = Terminal::new(TestBackend::new(width, 2)).expect("test backend");
    let v = shown(Sidebar::ProjectsOnTop, Vec::new());
    t.draw(|f| draw_agent(f, &v, r, 1, &entries()[0])).expect("draw");
    assert_eq!(row_text(&t, Rect { height: 1, ..r }).trim(), expected);
}

#[test]
fn compact_mode_shows_it_as_a_third_menu() {
    let small = full_layout(SMALL, Widths::default(), false, Sidebar::ProjectsOnTop, true);
    let agents = small.shown(Some(Nav::Agents));
    let projects = small.shown(Some(Nav::Projects));
    assert_eq!(
        (agents.agents_list.is_empty(), agents.list.is_empty(), agents.workspaces_list.is_empty()),
        (false, true, true)
    );
    assert_eq!((agents.back.is_empty(), agents.agents_button.is_empty()), (false, true));
    assert_eq!((projects.agents_list.is_empty(), projects.agents_button.is_empty()), (true, false));
}

#[test]
fn compact_mode_has_no_agents_button_when_it_is_off() {
    assert!(layout(SMALL, Widths::default()).shown(Some(Nav::Projects)).agents_button.is_empty());
}

#[test]
fn renders_side_by_side() {
    insta::assert_snapshot!(render_sized(&shown(Sidebar::SideBySide, entries()), W, TALL.height).backend());
}

#[test]
fn renders_projects_on_top() {
    insta::assert_snapshot!(render_sized(&shown(Sidebar::ProjectsOnTop, entries()), W, TALL.height).backend());
}

#[test]
fn renders_workspaces_on_top() {
    let v = shown(Sidebar::WorkspacesOnTop, entries());
    insta::assert_snapshot!(render_sized(&v, W, TALL.height).backend());
}

#[test]
fn renders_the_tree() {
    insta::assert_snapshot!(render_sized(&shown(Sidebar::Tree, entries()), W, TALL.height).backend());
}

#[test]
fn renders_in_a_narrow_column() {
    let v = View {
        widths: Widths { projects: MIN_COLUMN_WIDTH + 4, ..Widths::default() },
        ..shown(Sidebar::ProjectsOnTop, entries())
    };
    insta::assert_snapshot!(render_sized(&v, W, TALL.height).backend());
}

#[test]
fn renders_no_agents_running() {
    insta::assert_snapshot!(render_sized(&shown(Sidebar::ProjectsOnTop, Vec::new()), W, TALL.height).backend());
}

#[test]
fn renders_the_compact_agents_menu() {
    let v = View { nav: Some(Nav::Agents), ..shown(Sidebar::ProjectsOnTop, entries()) };
    insta::assert_snapshot!(render_sized(&v, SMALL.width, SMALL.height).backend());
}

#[test]
fn renders_the_compact_projects_menu_with_its_button() {
    let v = View { nav: Some(Nav::Projects), ..shown(Sidebar::ProjectsOnTop, entries()) };
    insta::assert_snapshot!(render_sized(&v, SMALL.width, SMALL.height).backend());
}

#[test]
fn short_terminals_do_not_panic() {
    for sidebar in Sidebar::ALL {
        for height in 1..=TALL.height {
            render_sized(&shown(sidebar, entries()), W, height);
        }
    }
}
