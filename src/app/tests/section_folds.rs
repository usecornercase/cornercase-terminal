#![cfg(unix)]

use rstest::rstest;

use super::*;
use crate::agents::activity::Status;
use crate::config;
use crate::state::Section;
use crate::tour::Target as TourTarget;

const TALL: Rect = Rect { x: 0, y: 0, width: 100, height: 30 };

const NARROW: Rect = Rect { x: 0, y: 0, width: 80, height: 30 };

fn laid_out(sidebar: config::Sidebar, projects: usize) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, dirs) = app_with(projects);
    app.config.sidebar = sidebar.id().into();
    app.config.agents_section = true;
    app.follow(TALL);
    (app, rx, dirs)
}

fn title(app: &App, area: Rect, section: Section) -> Position {
    let target = ui::section_title(&app.layout(area).shown(app.screen.nav), section);
    assert!(!target.is_empty(), "{section:?} has no title");
    Position::new(target.x + 3, target.y)
}

fn click_title(app: &mut App, area: Rect, section: Section) {
    let at = title(app, area, section);
    click_in(app, at, area);
}

fn folded(app: &App) -> Vec<Section> {
    app.screen.sections.listed()
}

fn drawn_title(app: &mut App, area: Rect, section: Section) -> String {
    let target = ui::section_title(&app.layout(area).shown(app.screen.nav), section);
    let terminal = render(app, area);
    let buffer = terminal.backend().buffer();
    let middle = target.y + target.height.saturating_sub(1) / 2;
    (target.x..target.right()).map(|x| buffer[(x, middle)].symbol()).collect()
}

#[rstest]
#[case::side_by_side_projects(config::Sidebar::SideBySide, Section::Projects)]
#[case::side_by_side_workspaces(config::Sidebar::SideBySide, Section::Workspaces)]
#[case::side_by_side_agents(config::Sidebar::SideBySide, Section::Agents)]
#[case::projects_on_top_projects(config::Sidebar::ProjectsOnTop, Section::Projects)]
#[case::projects_on_top_workspaces(config::Sidebar::ProjectsOnTop, Section::Workspaces)]
#[case::workspaces_on_top_workspaces(config::Sidebar::WorkspacesOnTop, Section::Workspaces)]
#[case::workspaces_on_top_agents(config::Sidebar::WorkspacesOnTop, Section::Agents)]
#[case::tree_projects(config::Sidebar::Tree, Section::Projects)]
#[case::tree_agents(config::Sidebar::Tree, Section::Agents)]
fn a_click_on_a_title_collapses_its_section_and_another_opens_it(
    #[case] sidebar: config::Sidebar,
    #[case] section: Section,
) {
    let (mut app, _rx, _dirs) = laid_out(sidebar, 2);

    click_title(&mut app, TALL, section);
    assert_eq!(folded(&app), [section]);
    assert!(drawn_title(&mut app, TALL, section).starts_with(" ▸ "), "{}", drawn_title(&mut app, TALL, section));

    click_title(&mut app, TALL, section);
    assert_eq!(folded(&app), []);
    assert!(drawn_title(&mut app, TALL, section).starts_with(" ▾ "), "{}", drawn_title(&mut app, TALL, section));
}

#[rstest]
#[case::projects(ui::Nav::Projects, Section::Projects)]
#[case::workspaces(ui::Nav::Workspaces, Section::Workspaces)]
#[case::agents(ui::Nav::Agents, Section::Agents)]
fn a_tap_on_a_menu_title_collapses_its_section_in_compact_mode(#[case] nav: ui::Nav, #[case] section: Section) {
    let (mut app, _rx, _dirs) = laid_out(config::Sidebar::SideBySide, 2);
    app.screen.nav = Some(nav);
    app.follow(NARROW);

    click_title(&mut app, NARROW, section);

    assert_eq!((folded(&app), app.screen.nav), (vec![section], Some(nav)));
    assert!(drawn_title(&mut app, NARROW, section).contains("▸ "), "{}", drawn_title(&mut app, NARROW, section));
}

#[test]
fn the_back_button_of_a_menu_does_not_fold_its_section() {
    let (mut app, _rx, _dirs) = laid_out(config::Sidebar::SideBySide, 1);
    app.screen.nav = Some(ui::Nav::Agents);
    let back = app.layout(NARROW).shown(app.screen.nav).back;

    click_in(&mut app, back.as_position(), NARROW);

    assert_eq!((folded(&app), app.screen.nav), (vec![], Some(ui::Nav::Projects)));
}

#[rstest]
#[case::released_elsewhere(false)]
#[case::moved_off_and_back(true)]
fn a_press_that_leaves_the_title_changes_nothing(#[case] back: bool) {
    let (mut app, _rx, _dirs) = laid_out(config::Sidebar::SideBySide, 2);
    let from = title(&app, TALL, Section::Projects);
    let away = Position::new(from.x, from.y + 4);
    let to = if back { from } else { away };

    mouse_in(&mut app, MouseEventKind::Down(MouseButton::Left), from, TALL);
    mouse_in(&mut app, MouseEventKind::Drag(MouseButton::Left), away, TALL);
    mouse_in(&mut app, MouseEventKind::Drag(MouseButton::Left), to, TALL);
    mouse_in(&mut app, MouseEventKind::Up(MouseButton::Left), to, TALL);

    assert_eq!((folded(&app), app.screen.drag.is_some()), (vec![], false));
}

#[test]
fn a_press_alone_does_not_fold_and_escape_ends_it() {
    let (mut app, _rx, _dirs) = laid_out(config::Sidebar::SideBySide, 1);
    let at = title(&app, TALL, Section::Workspaces);

    mouse_in(&mut app, MouseEventKind::Down(MouseButton::Left), at, TALL);
    assert_eq!(folded(&app), []);
    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    mouse_in(&mut app, MouseEventKind::Up(MouseButton::Left), at, TALL);

    assert_eq!(folded(&app), []);
}

#[test]
fn a_right_click_on_a_title_opens_no_menu() {
    let (mut app, _rx, _dirs) = laid_out(config::Sidebar::SideBySide, 1);
    let at = title(&app, TALL, Section::Projects);

    mouse_in(&mut app, MouseEventKind::Down(MouseButton::Right), at, TALL);
    mouse_in(&mut app, MouseEventKind::Up(MouseButton::Right), at, TALL);

    assert!(app.overlay.is_none() && folded(&app).is_empty());
}

#[test]
fn folding_moves_neither_the_focus_nor_the_active_row() {
    let (mut app, _rx, _dirs) = laid_out(config::Sidebar::SideBySide, 2);
    let (focus, active) = (app.focus(), app.active);

    for section in Section::ALL {
        click_title(&mut app, TALL, section);
    }
    app.follow(TALL);

    assert_eq!((app.focus(), app.active, folded(&app)), (focus, active, Section::ALL.to_vec()));
}

#[derive(Clone, Copy)]
enum Away {
    Search,
    Pane,
    Row,
}

#[rstest]
#[case::a_search_jump(Away::Search)]
#[case::a_jump_to_a_pane_there(Away::Pane)]
#[case::a_click_on_a_workspace_row(Away::Row)]
fn going_elsewhere_leaves_a_folded_section_folded(#[case] away: Away) {
    let (mut app, _rx, _dirs) = laid_out(config::Sidebar::ProjectsOnTop, 2);
    app.active = 0;
    app.follow(TALL);
    click_title(&mut app, TALL, Section::Projects);
    let (project, pane) = (app.projects[1].id, app.projects[1].workspaces[0].tabs[0].panes[0].id);

    match away {
        Away::Search => app.goto(Goto::Place { project, workspace: None, tab: None }),
        Away::Pane => app.jump_to_pane(pane),
        Away::Row => {
            let list = app.layout(TALL).workspaces_list;
            let row = ui::workspace_row(list, 1, &app.tab_lines(), 0, WorkspaceRow::Workspace(0));
            click_in(&mut app, Position::new(row.x + 6, row.y), TALL);
        }
    }
    app.follow(TALL);

    assert_eq!(folded(&app), [Section::Projects]);
}

#[test]
fn expanding_a_section_reveals_the_active_row_it_moved_out_of_view() {
    let (mut app, _rx, _dirs) = laid_out(config::Sidebar::ProjectsOnTop, 8);
    app.active = 0;
    app.follow(TALL);
    click_title(&mut app, TALL, Section::Projects);
    let (folded_scroll, last) = (app.scroll.of(List::Projects), app.projects[7].id);

    app.goto(Goto::Place { project: last, workspace: None, tab: None });
    app.follow(TALL);
    assert_eq!(app.scroll.of(List::Projects), folded_scroll);
    click_title(&mut app, TALL, Section::Projects);
    app.follow(TALL);

    let (areas, rows) = (app.layout(TALL), app.sidebar_rows());
    let shown = ui::entry_row(areas.list, areas.pitch, &rows, app.scroll.of(List::Projects), SidebarRow::Project(7));
    assert!(!shown.is_empty(), "{:?} at {}", areas.list, app.scroll.of(List::Projects));
}

#[test]
fn the_wheel_over_a_folded_section_keeps_its_scroll() {
    let (mut app, _rx, _dirs) = laid_out(config::Sidebar::SideBySide, 8);
    app.scroll.set(List::Projects, 3);
    click_title(&mut app, TALL, Section::Projects);
    let at = title(&app, TALL, Section::Projects);

    mouse_in(&mut app, MouseEventKind::ScrollDown, at, TALL);
    mouse_in(&mut app, MouseEventKind::ScrollUp, at, TALL);

    assert_eq!(app.scroll.of(List::Projects), 3);
}

#[test]
fn the_folds_are_saved_and_come_back() {
    let (mut app, _rx, _dirs) = laid_out(config::Sidebar::SideBySide, 1);
    app.screen.sections = state::Sections::of(&[Section::Agents, Section::Projects]);
    let saved = app.state();

    let (mut restored, _rx) = empty_app();
    restored.restore(&saved, TALL);

    assert_eq!(saved.collapsed_sections, [Section::Projects, Section::Agents]);
    assert_eq!(folded(&restored), [Section::Projects, Section::Agents]);
}

#[test]
fn a_restore_without_folds_opens_every_section() {
    let (app, _rx, _dirs) = laid_out(config::Sidebar::SideBySide, 1);
    let saved = app.state();
    let (mut restored, _rx) = empty_app();
    restored.screen.sections = state::Sections::of(&Section::ALL);

    restored.restore(&saved, TALL);

    assert_eq!(folded(&restored), []);
}

#[rstest]
#[case::the_stack_line_over_folded_projects(&[Section::Projects], ui::Border::Stack)]
#[case::the_stack_line_over_folded_workspaces(&[Section::Workspaces], ui::Border::Stack)]
#[case::the_agents_line_of_a_folded_section(&[Section::Agents], ui::Border::Agents)]
#[case::the_agents_line_under_folded_lists(&[Section::Projects, Section::Workspaces], ui::Border::Agents)]
fn a_press_on_a_fixed_line_drags_nothing(#[case] sections: &[Section], #[case] border: ui::Border) {
    let (mut app, _rx, _dirs) = laid_out(config::Sidebar::ProjectsOnTop, 1);
    app.screen.sections = state::Sections::of(sections);
    app.follow(TALL);
    let areas = app.layout(TALL);
    let line = if border == ui::Border::Stack { areas.stack_line } else { areas.agents_line };
    let widths = app.screen.widths;

    drag_between_in(&mut app, Position::new(line.x + 3, line.y), Position::new(line.x + 3, line.y + 3));

    assert_eq!((app.screen.widths, app.screen.drag.is_some()), (widths, false));
}

fn drag_between_in(app: &mut App, from: Position, to: Position) {
    mouse_in(app, MouseEventKind::Down(MouseButton::Left), from, TALL);
    mouse_in(app, MouseEventKind::Drag(MouseButton::Left), to, TALL);
    mouse_in(app, MouseEventKind::Up(MouseButton::Left), to, TALL);
}

#[rstest]
#[case::its_own_section(Section::Projects)]
#[case::another_one_that_moves_the_rows(Section::Workspaces)]
fn a_row_grabbed_before_a_fold_does_nothing_on_release(#[case] section: Section) {
    let (mut app, _rx, _dirs) = laid_out(config::Sidebar::WorkspacesOnTop, 2);
    app.active = 0;
    app.follow(TALL);
    let areas = app.layout(TALL);
    let rows = app.sidebar_rows();
    let row = ui::entry_row(areas.list, areas.pitch, &rows, 0, SidebarRow::Project(1));
    let at = Position::new(row.x + 3, row.y);
    let order: Vec<u64> = app.projects.iter().map(|p| p.id).collect();

    mouse_in(&mut app, MouseEventKind::Down(MouseButton::Left), at, TALL);
    app.set_section_fold(section, true);
    app.follow(TALL);
    mouse_in(&mut app, MouseEventKind::Up(MouseButton::Left), at, TALL);

    assert_eq!((app.active, app.projects.iter().map(|p| p.id).collect::<Vec<_>>()), (0, order));
}

#[rstest]
#[case::projects(TourTarget::Projects, Section::Projects, config::Sidebar::SideBySide)]
#[case::new_project(TourTarget::NewProject, Section::Projects, config::Sidebar::SideBySide)]
#[case::workspaces(TourTarget::Workspaces, Section::Workspaces, config::Sidebar::SideBySide)]
#[case::tabs(TourTarget::Tabs, Section::Workspaces, config::Sidebar::ProjectsOnTop)]
#[case::agents(TourTarget::Agents, Section::Agents, config::Sidebar::SideBySide)]
#[case::the_tree(TourTarget::Workspaces, Section::Projects, config::Sidebar::Tree)]
fn the_tour_points_at_the_title_of_a_folded_section(
    #[case] target: TourTarget,
    #[case] section: Section,
    #[case] sidebar: config::Sidebar,
) {
    let (mut app, _rx, _dirs) = laid_out(sidebar, 1);
    app.screen.sections = state::Sections::of(&[section]);
    let areas = app.layout(TALL);

    assert_eq!(app.tour_target(&areas, target), (ui::section_title(&areas, section), false));
}

#[test]
fn a_folded_title_counts_what_it_hides() {
    let (mut app, _rx, _dirs) = laid_out(config::Sidebar::SideBySide, 3);
    app.screen.sections = state::Sections::of(&[Section::Projects, Section::Workspaces]);

    let drawn = drawn_title(&mut app, TALL, Section::Projects);

    assert!(drawn.starts_with(" ▸ Projects (3)"), "{drawn}");
    assert!(drawn_title(&mut app, TALL, Section::Workspaces).starts_with(" ▸ Workspaces (1)"));
}

#[test]
fn a_folded_agents_title_counts_the_agents_of_folded_teams_too() {
    let entry = ui::AgentEntry {
        status: Some(Status::Working),
        agent: "claude".into(),
        project: "shop".into(),
        workspace: None,
        role: None,
        place: true,
        details: ui::Details::default(),
        active: false,
        depth: 0,
        last: true,
        rails: Vec::new(),
        fold: Some(true),
        hidden: 2,
        attention: Some(Status::Waiting),
        machine: None,
    };
    let view = ui::View {
        folded: state::Sections::of(&[Section::Agents]),
        agents: Some(ui::AgentsView { entries: vec![entry], scroll: 0 }),
        ..ui::View::default()
    };

    let tags = sidebar::section_tags(&view);

    assert_eq!(tags, [(Section::Agents, ui::SectionTags { count: 3, attention: Some(Status::Waiting) })]);
}
