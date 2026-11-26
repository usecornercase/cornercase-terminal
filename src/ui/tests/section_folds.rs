use super::*;
use crate::state::Section;

const TALL: Rect = Rect { x: 0, y: 0, width: W, height: 30 };

const NARROW_COLUMN: Widths = Widths { projects: 16, workspaces: 16, changes: None, stack: None, agents: None };

fn workspace(name: &str, tabs: &[&str]) -> WorkspaceEntry {
    WorkspaceEntry {
        shared: 0,
        collapsed: false,
        name: name.into(),
        tabs: tabs.iter().map(|&tab| tab.into()).collect(),
        behind: 0,
        pull: None,
        removing: false,
        setting_up: false,
        closed: 0,
        changed: None,
        machine: None,
    }
}

fn agent(project: &str) -> AgentEntry {
    AgentEntry {
        status: Some(Status::Waiting),
        agent: "claude".into(),
        project: project.into(),
        workspace: None,
        role: None,
        place: true,
        details: Details::default(),
        active: false,
        depth: 0,
        last: true,
        rails: Vec::new(),
        fold: None,
        hidden: 0,
        attention: None,
        machine: None,
    }
}

fn shown(sidebar: Sidebar, folded: &[Section]) -> View<'static> {
    let open = ProjectShape { workspaces: vec![WorkspaceShape::default()], ..ProjectShape::default() };
    let tree = TreeView {
        shape: TreeShape { groups: Vec::new(), projects: vec![open; 3], tab_bar: false },
        workspaces: (0..3).map(|_| vec![workspace("login", &["claude"])]).collect(),
    };
    let tags = SectionTags { count: 3, attention: Some(Status::Waiting) };
    View {
        has_project: true,
        issues: true,
        active: 1,
        active_tab: Some(0),
        sidebar,
        folded: Sections::of(folded),
        section_tags: folded.iter().map(|&section| (section, tags)).collect(),
        workspaces: vec![workspace("login", &["claude", "nvim"]), workspace("main", &["zsh"])],
        tree: (sidebar == Sidebar::Tree).then_some(tree),
        agents: Some(AgentsView { entries: vec![agent("website"), agent("api")], scroll: 0 }),
        ..view(&["cornercase", "website", "api"])
    }
}

fn screen(sidebar: Sidebar, folded: &[Section]) -> Screen {
    Screen { sidebar, agents: true, folded: Sections::of(folded), ..Screen::default() }
}

fn areas_of(area: Rect, sidebar: Sidebar, folded: &[Section]) -> Areas {
    full_layout(area, Widths::default(), screen(sidebar, folded))
}

fn with_bots(view: View<'static>) -> View<'static> {
    let bots = BotsView { entries: Vec::new(), scroll: 0, page: false };
    View { bots: Some(bots), ..view }
}

#[rstest]
#[case::every_title_open("every_title_open", Sidebar::SideBySide, &[])]
#[case::folded_projects("folded_projects", Sidebar::SideBySide, &[Section::Projects])]
#[case::folded_workspaces("folded_workspaces", Sidebar::SideBySide, &[Section::Workspaces])]
#[case::folded_agents("folded_agents", Sidebar::SideBySide, &[Section::Agents])]
#[case::the_top_folded("stacked_top_folded", Sidebar::ProjectsOnTop, &[Section::Projects])]
#[case::the_bottom_folded("stacked_bottom_folded", Sidebar::ProjectsOnTop, &[Section::Workspaces])]
#[case::both_folded("stacked_both_folded", Sidebar::ProjectsOnTop, &[Section::Projects, Section::Workspaces])]
#[case::workspaces_on_top("workspaces_on_top_top_folded", Sidebar::WorkspacesOnTop, &[Section::Workspaces])]
#[case::the_tree("tree_projects_folded", Sidebar::Tree, &[Section::Projects])]
fn renders_the_folded_sections(#[case] name: &str, #[case] sidebar: Sidebar, #[case] folded: &[Section]) {
    insta::assert_snapshot!(name, render_sized(&shown(sidebar, folded), W, TALL.height).backend());
}

#[rstest]
#[case::projects("compact_projects_folded", Nav::Projects, Section::Projects)]
#[case::workspaces("compact_workspaces_folded", Nav::Workspaces, Section::Workspaces)]
#[case::agents("compact_agents_folded", Nav::Agents, Section::Agents)]
fn renders_a_folded_menu_in_compact_mode(#[case] name: &str, #[case] nav: Nav, #[case] section: Section) {
    let view = View { nav: Some(nav), ..shown(Sidebar::SideBySide, &[section]) };
    insta::assert_snapshot!(name, render_sized(&view, SMALL.width, SMALL.height).backend());
}

#[test]
fn the_tree_ignores_a_folded_workspaces_section() {
    let plain = render_sized(&shown(Sidebar::Tree, &[]), W, TALL.height);
    let folded = render_sized(&shown(Sidebar::Tree, &[Section::Workspaces]), W, TALL.height);

    assert_eq!(folded.backend().buffer(), plain.backend().buffer());
}

#[rstest]
#[case::nothing_folded(Sidebar::ProjectsOnTop, &[], true, true)]
#[case::the_top_folded(Sidebar::ProjectsOnTop, &[Section::Projects], false, true)]
#[case::the_bottom_folded(Sidebar::ProjectsOnTop, &[Section::Workspaces], false, true)]
#[case::both_folded(Sidebar::ProjectsOnTop, &[Section::Projects, Section::Workspaces], false, false)]
#[case::agents_folded(Sidebar::ProjectsOnTop, &[Section::Agents], true, false)]
#[case::workspaces_on_top(Sidebar::WorkspacesOnTop, &[Section::Workspaces], false, true)]
#[case::side_by_side_projects(Sidebar::SideBySide, &[Section::Projects], false, false)]
#[case::side_by_side_workspaces(Sidebar::SideBySide, &[Section::Workspaces], false, true)]
#[case::tree_projects(Sidebar::Tree, &[Section::Projects], false, false)]
#[case::tree_workspaces(Sidebar::Tree, &[Section::Workspaces], false, true)]
fn a_line_drags_only_while_both_sides_of_it_are_open(
    #[case] sidebar: Sidebar,
    #[case] folded: &[Section],
    #[case] stack: bool,
    #[case] agents: bool,
) {
    let a = areas_of(TALL, sidebar, folded);
    let line = |dragged, line: Rect| if dragged { line } else { Rect::default() };

    assert_eq!(a.stack_line.is_empty(), !sidebar.stacked() || sidebar == Sidebar::Tree, "{a:?}");
    assert_eq!(a.stack_border, line(stack, a.stack_line));
    assert!(!a.agents_line.is_empty());
    assert_eq!(a.agents_border, line(agents, a.agents_line));
}

#[rstest]
#[case::side_by_side(Sidebar::SideBySide)]
#[case::tree(Sidebar::Tree)]
fn folded_projects_leave_the_room_to_the_agents(#[case] sidebar: Sidebar) {
    let a = areas_of(TALL, sidebar, &[Section::Projects]);

    assert_eq!((a.title.height, a.list, a.agents_line.y), (1, Rect::default(), a.title.bottom()));
    assert_eq!(a.agents.bottom(), a.separator.y);
}

#[test]
fn a_folded_agents_section_sits_at_the_bottom_with_its_line_above_it() {
    let a = areas_of(TALL, Sidebar::SideBySide, &[Section::Agents]);

    assert_eq!((a.agents.height, a.agents.bottom(), a.agents_line.bottom()), (1, a.separator.y, a.agents.y));
    assert_eq!(a.list.bottom(), a.agents_line.y);
}

#[rstest]
#[case::projects_on_top(Sidebar::ProjectsOnTop, Section::Projects)]
#[case::workspaces_on_top(Sidebar::WorkspacesOnTop, Section::Workspaces)]
fn a_folded_top_section_is_one_row_and_the_other_takes_the_rest(#[case] sidebar: Sidebar, #[case] folded: Section) {
    let a =
        full_layout(TALL, Widths::default(), Screen { sidebar, folded: Sections::of(&[folded]), ..Screen::default() });
    let (top, below) = match folded {
        Section::Projects => (a.title, a.workspaces_list),
        _ => (a.workspaces_title, a.list),
    };

    assert_eq!((a.stack_line.y, below.bottom()), (top.bottom(), a.separator.y));
}

#[test]
fn a_folded_bottom_section_is_one_row_above_the_footer() {
    let screen =
        Screen { sidebar: Sidebar::ProjectsOnTop, folded: Sections::of(&[Section::Workspaces]), ..Screen::default() };
    let a = full_layout(TALL, Widths::default(), screen);

    assert_eq!((a.workspaces_title.bottom(), a.list.bottom()), (a.separator.y, a.stack_line.y));
}

#[test]
fn both_folded_sections_leave_their_titles_at_the_top() {
    let a = areas_of(TALL, Sidebar::ProjectsOnTop, &[Section::Projects, Section::Workspaces]);

    assert_eq!((a.stack_line.y, a.workspaces_title.y), (a.title.bottom(), a.stack_line.bottom()));
    assert_eq!((a.agents_line.y, a.agents.bottom()), (a.workspaces_title.bottom(), a.separator.y));
}

#[rstest]
#[case::projects(Section::Projects)]
#[case::workspaces(Section::Workspaces)]
#[case::agents(Section::Agents)]
fn a_title_target_never_covers_a_button_in_compact_mode(#[case] section: Section) {
    let screen =
        Screen { agents: true, bots: BotsShape { offered: true, ..BotsShape::default() }, ..Screen::default() };
    let nav = match section {
        Section::Projects => Nav::Projects,
        Section::Workspaces => Nav::Workspaces,
        Section::Agents => Nav::Agents,
    };
    let a = full_layout(SMALL, Widths::default(), screen).shown(Some(nav));
    let target = section_title(&a, section);

    assert!(!target.is_empty() && target.height == COMPACT_PITCH, "{target:?}");
    for button in [a.back, a.bots_button, a.agents_button] {
        assert!(!target.intersects(button), "{target:?} covers {button:?}");
    }
}

#[rstest]
#[case::side_by_side(Sidebar::SideBySide)]
#[case::projects_on_top(Sidebar::ProjectsOnTop)]
#[case::tree(Sidebar::Tree)]
fn the_projects_target_ends_where_the_bots_button_starts(#[case] sidebar: Sidebar) {
    let bots = BotsShape { offered: true, ..BotsShape::default() };
    let a = full_layout(TALL, Widths::default(), Screen { sidebar, bots, ..Screen::default() });
    let target = section_title(&a, Section::Projects);

    assert_eq!((target.x, target.right()), (a.title.x, a.bots_button.x));
}

#[test]
fn a_hidden_title_has_no_target() {
    let a = full_layout(SMALL, Widths::default(), screen(Sidebar::SideBySide, &[])).shown(None);

    assert_eq!(Section::ALL.map(|section| section_title(&a, section).is_empty()), [true; 3]);
}

#[test]
fn the_tree_has_no_workspaces_target() {
    let a = areas_of(TALL, Sidebar::Tree, &[]);

    assert!(section_title(&a, Section::Workspaces).is_empty() && !section_title(&a, Section::Projects).is_empty());
}

#[rstest]
#[case::side_by_side(Sidebar::SideBySide)]
#[case::projects_on_top(Sidebar::ProjectsOnTop)]
#[case::tree(Sidebar::Tree)]
fn the_bots_page_ignores_the_folds_of_the_lists(#[case] sidebar: Sidebar) {
    let page = BotsShape { offered: true, page: true, ..BotsShape::default() };
    let plain = Screen { sidebar, agents: true, bots: page, ..Screen::default() };
    let folded = Screen { folded: Sections::of(&Section::ALL), ..plain };
    let a = full_layout(TALL, Widths::default(), folded);

    assert_eq!(a, full_layout(TALL, Widths::default(), plain));
    assert!(Section::ALL.into_iter().all(|section| section_title(&a, section).is_empty()), "{a:?}");
}

fn cell_bg(t: &Terminal<TestBackend>, x: u16, y: u16) -> Color {
    t.backend().buffer()[(x, y)].bg
}

#[test]
fn a_hovered_title_fills_its_target_and_stops_before_the_bots_button() {
    let view = with_bots(shown(Sidebar::SideBySide, &[]));
    let a = full_layout(TALL, Widths::default(), screen_of(&view));
    let target = section_title(&a, Section::Projects);
    let at = Position::new(target.x + 3, target.y);
    let t = render_sized(&View { hover: Some(at), ..view }, W, TALL.height);

    let filled: Vec<bool> = (target.x..target.right()).map(|x| cell_bg(&t, x, target.y) == DARK_HOVER).collect();
    assert!(!filled.is_empty() && filled.iter().all(|&fill| fill), "{filled:?}");
    assert_ne!(cell_bg(&t, a.bots_button.x, target.y), DARK_HOVER);
}

#[test]
fn a_title_is_not_filled_while_a_dialog_is_open() {
    let view = shown(Sidebar::SideBySide, &[]);
    let a = full_layout(TALL, Widths::default(), screen_of(&view));
    let target = section_title(&a, Section::Workspaces);
    let hover = Some(Position::new(target.x + 3, target.y));
    let overlay = Some(Overlay::Star);
    let t = render_sized(&View { hover, overlay, ..view }, W, TALL.height);

    assert_ne!(cell_bg(&t, target.x + 3, target.y), DARK_HOVER);
}

fn title_text(view: &View, section: Section) -> String {
    let a = full_layout(TALL, view.widths, screen_of(view));
    let t = render_sized(view, W, TALL.height);
    row_text(&t, section_title(&a, section))
}

#[rstest]
#[case::counts_on(true, " ▸ Projects (3)")]
#[case::counts_off(false, " ▸ Projects    ")]
fn a_folded_title_says_how_much_it_hides_while_counts_are_on(#[case] counts: bool, #[case] start: &str) {
    let view = View { counts, ..shown(Sidebar::SideBySide, &[Section::Projects]) };

    assert!(title_text(&view, Section::Projects).starts_with(start), "{}", title_text(&view, Section::Projects));
}

#[test]
fn an_open_title_says_nothing_of_what_is_under_it() {
    let view = shown(Sidebar::SideBySide, &[]);

    assert_eq!(title_text(&view, Section::Projects).trim_end(), " ▾ Projects");
}

#[rstest]
#[case::projects_in_the_narrowest_column(Section::Projects, 16, " ▸ Pr… ! ")]
#[case::projects_just_short_of_the_count(Section::Projects, 26, " ▸ Projects    ! ")]
#[case::projects_with_room_for_both(Section::Projects, 27, " ▸ Projects (3) ! ")]
#[case::workspaces_in_the_narrowest_column(Section::Workspaces, 16, " ▸ Workspac… ! ")]
#[case::workspaces_just_short_of_the_count(Section::Workspaces, 20, " ▸ Workspaces    ! ")]
#[case::workspaces_with_room_for_both(Section::Workspaces, 21, " ▸ Workspaces (3) ! ")]
#[case::agents_just_short_of_the_count(Section::Agents, 16, " ▸ Agents    ! ")]
#[case::agents_with_room_for_both(Section::Agents, 17, " ▸ Agents (3) ! ")]
fn a_folded_title_adds_its_count_only_beside_its_whole_name_and_keeps_what_needs_you(
    #[case] section: Section,
    #[case] width: u16,
    #[case] expected: &str,
) {
    let widths = Widths { projects: width, workspaces: width, ..NARROW_COLUMN };
    let view = with_bots(View { widths, ..shown(Sidebar::SideBySide, &[section]) });

    assert_eq!(title_text(&view, section), expected);
}

fn compact_title(view: &View, section: Section, cols: u16) -> String {
    let title = section_title(&full_layout(Rect::new(0, 0, cols, 30), view.widths, screen_of(view)), section);
    let t = render_sized(view, cols, 30);
    row_text(&t, Rect { y: title.y + 1, height: 1, ..title })
}

#[rstest]
#[case::no_room_beside_ten_cells(33, " ▸ documentatio… ! ")]
#[case::room_beside_ten_cells(34, " ▸ documenta… (3) ! ")]
fn a_long_project_name_gives_up_cells_to_the_count_only_down_to_ten(#[case] cols: u16, #[case] expected: &str) {
    let folded = shown(Sidebar::SideBySide, &[Section::Workspaces]);
    let projects = view(&["cornercase", "documentation-site"]).projects;
    let view = View { nav: Some(Nav::Workspaces), projects, ..folded };

    assert_eq!(compact_title(&view, Section::Workspaces, cols), expected);
}

fn compact_menu(section: Section) -> View<'static> {
    let nav = match section {
        Section::Projects => Nav::Projects,
        Section::Workspaces => Nav::Workspaces,
        Section::Agents => Nav::Agents,
    };
    with_bots(View { nav: Some(nav), ..shown(Sidebar::SideBySide, &[section]) })
}

#[rstest]
#[case::open(&[], 23, " ▾ 界 界 … ")]
#[case::the_name_cut_beside_the_icon(&[Section::Workspaces], 29, " ▸ 界 界 界 界 … ! ")]
#[case::the_whole_name_short_of_the_count(&[Section::Workspaces], 33, " ▸ 界 界 界 界 界     ! ")]
#[case::room_for_both(&[Section::Workspaces], 34, " ▸ 界 界 界 界 界  (3) ! ")]
fn a_wide_project_name_is_measured_in_cells(#[case] folded: &[Section], #[case] cols: u16, #[case] expected: &str) {
    let projects = view(&["cornercase", "界界界界界"]).projects;
    let view = View { projects, folded: Sections::of(folded), ..compact_menu(Section::Workspaces) };

    assert_eq!(compact_title(&view, Section::Workspaces, cols), expected);
}

#[rstest]
#[case::only_the_icon_fits(26, " ▸ ! ")]
#[case::one_cell_short_of_a_name(28, " ▸   ! ")]
#[case::the_name_cut_beside_the_icon(29, " ▸ P… ! ")]
fn a_narrow_compact_menu_cuts_the_name_before_what_needs_you(#[case] cols: u16, #[case] expected: &str) {
    assert_eq!(compact_title(&compact_menu(Section::Projects), Section::Projects, cols), expected);
}

#[rstest]
#[case::projects(Section::Projects)]
#[case::workspaces(Section::Workspaces)]
#[case::agents(Section::Agents)]
fn what_needs_you_shows_at_every_compact_width_with_a_cell_to_spare_after_it(#[case] section: Section) {
    let view = compact_menu(section);

    for cols in 18..=60 {
        let title = compact_title(&view, section, cols);
        assert_eq!(title.contains('!'), title.chars().count() >= 5, "{cols}: {title:?}");
    }
}

#[rstest]
#[case::projects(Section::Projects)]
#[case::workspaces(Section::Workspaces)]
#[case::agents(Section::Agents)]
fn a_folded_title_with_nothing_waiting_is_never_cut_shorter_than_the_open_one(#[case] section: Section) {
    for width in 16..=32 {
        let widths = Widths { projects: width, workspaces: width, ..NARROW_COLUMN };
        let open = with_bots(View { widths, ..shown(Sidebar::SideBySide, &[]) });
        let quiet = vec![(section, SectionTags { count: 3, attention: None })];
        let folded = with_bots(View { widths, section_tags: quiet, ..shown(Sidebar::SideBySide, &[section]) });
        let open = title_text(&open, section).trim_end().replace('▾', "▸");

        assert!(title_text(&folded, section).starts_with(&open), "{width}: {open:?}");
    }
}

#[rstest]
#[case::projects(Section::Projects)]
#[case::workspaces(Section::Workspaces)]
#[case::agents(Section::Agents)]
fn a_folded_compact_title_with_nothing_waiting_is_never_cut_shorter_than_the_open_one(#[case] section: Section) {
    let open = View { folded: Sections::default(), section_tags: Vec::new(), ..compact_menu(section) };
    let quiet = vec![(section, SectionTags { count: 3, attention: None })];
    let folded = View { section_tags: quiet, ..compact_menu(section) };

    for cols in 18..=60 {
        let open = compact_title(&open, section, cols).trim_end().replace('▾', "▸");
        assert!(compact_title(&folded, section, cols).starts_with(&open), "{cols}: {open:?}");
    }
}
