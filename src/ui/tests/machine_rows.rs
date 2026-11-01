use super::*;
use crate::agents::activity::Status;
use crate::ui::rows::ROW_MENU_ICON;

const TALL: u16 = 50;

fn tag(name: &str, dim: bool) -> MachineTag {
    MachineTag { icon: "☁".into(), name: name.into(), dim, seen: Some("2 min".into()) }
}

fn there(name: &str, dim: bool) -> WorkspaceEntry {
    let details = Details { model: Some("opus".into()), percent: Some(40), ..Details::default() };
    let agent = TabEntry { status: Some(Status::Working), details, ..TabEntry::from("claude") };
    WorkspaceEntry {
        name: format!("api-{name}"),
        tabs: vec![agent, "zsh".into()],
        machine: Some(tag(name, dim)),
        ..WorkspaceEntry::default()
    }
}

fn only_there(name: &str, machine: &str, dim: bool) -> ProjectEntry {
    ProjectEntry {
        name: name.into(),
        workspaces: 1,
        status: Some(Status::Working),
        machine: Some(tag(machine, dim)),
        ..ProjectEntry::default()
    }
}

fn projects() -> Vec<ProjectEntry> {
    let ours = ProjectEntry { name: "web-shop".into(), workspaces: 3, ..ProjectEntry::default() };
    vec![ours, only_there("blog", "vps", false), only_there("notes", "nas", true)]
}

fn with_machines(sidebar: Sidebar, nav: Option<Nav>) -> View<'static> {
    View {
        has_project: true,
        workspaces: vec![
            WorkspaceEntry { name: "login".into(), tabs: vec!["nvim".into()], ..WorkspaceEntry::default() },
            there("vps", false),
            there("pi", true),
        ],
        active_tab: Some(0),
        spin: Some(3),
        sidebar,
        nav,
        projects: projects(),
        ..view(&[])
    }
}

fn rendered(sidebar: Sidebar, nav: Option<Nav>) -> Terminal<TestBackend> {
    let width = if nav.is_some() { SMALL.width } else { W };
    render_sized(&with_machines(sidebar, nav), width, TALL)
}

fn spot(t: &Terminal<TestBackend>, text: &str) -> Option<Position> {
    let buffer = t.backend().buffer();
    let line = |y| (0..buffer.area.width).map(|x| buffer[(x, y)].symbol().to_string()).collect::<Vec<_>>();
    (0..buffer.area.height).find_map(|y| {
        let cells = line(y);
        let x = (0..cells.len()).find(|&x| cells[x..].concat().starts_with(text))?;
        Some(Position::new(u16::try_from(x).ok()?, y))
    })
}

fn find(t: &Terminal<TestBackend>, text: &str) -> Position {
    spot(t, text).unwrap_or_else(|| panic!("`{text}` is drawn"))
}

fn shown_there(nav: Option<Nav>) -> View<'static> {
    View {
        has_project: false,
        remote_project: true,
        active: 1,
        workspaces: vec![there("vps", false)],
        ..with_machines(Sidebar::SideBySide, nav)
    }
}

fn row_of(t: &Terminal<TestBackend>, text: &str) -> String {
    let at = find(t, text);
    let buffer = t.backend().buffer();
    (0..buffer.area.width).map(|x| buffer[(x, at.y)].symbol().to_string()).collect()
}

fn hovered(text: &str) -> String {
    let t = rendered(Sidebar::SideBySide, None);
    let v = View { hover: Some(find(&t, text)), ..with_machines(Sidebar::SideBySide, None) };
    row_of(&render_sized(&v, W, TALL), text)
}

#[test]
fn renders_a_machine_s_rows_side_by_side() {
    insta::assert_snapshot!(rendered(Sidebar::SideBySide, None).backend());
}

#[test]
fn renders_a_machine_s_rows_with_the_projects_on_top() {
    insta::assert_snapshot!(rendered(Sidebar::ProjectsOnTop, None).backend());
}

#[test]
fn renders_a_machine_s_rows_with_the_workspaces_on_top() {
    insta::assert_snapshot!(rendered(Sidebar::WorkspacesOnTop, None).backend());
}

#[test]
fn renders_a_machine_s_rows_in_the_compact_menu() {
    insta::assert_snapshot!(rendered(Sidebar::SideBySide, Some(Nav::Workspaces)).backend());
}

#[rstest]
#[case::side_by_side(Sidebar::SideBySide, None)]
#[case::projects_on_top(Sidebar::ProjectsOnTop, None)]
#[case::workspaces_on_top(Sidebar::WorkspacesOnTop, None)]
#[case::compact(Sidebar::SideBySide, Some(Nav::Workspaces))]
fn a_fallen_machine_s_rows_are_dim_and_a_linked_one_s_are_not(#[case] sidebar: Sidebar, #[case] nav: Option<Nav>) {
    let t = rendered(sidebar, nav);
    let dim = |text| t.backend().buffer()[find(&t, text)].modifier.contains(Modifier::DIM);

    let fallen = (dim("api-pi"), dim("☁ pi"), dim("working · seen 2 min ago"));
    let linked = (dim("api-vps"), dim("☁ vps"), dim("opus · 40%"));

    assert_eq!((fallen, linked), ((true, true, true), (false, false, false)));
}

#[test]
fn a_fallen_machine_s_spinner_stands_still() {
    let t = rendered(Sidebar::SideBySide, None);

    let spinners: Vec<String> = ["api-vps", "api-pi"]
        .iter()
        .map(|workspace| {
            let below = find(&t, workspace).y + 1;
            let buffer = t.backend().buffer();
            (0..buffer.area.width).map(|x| buffer[(x, below)].symbol().to_string()).collect()
        })
        .collect();

    assert!(spinners[0].contains("⠸ claude") && spinners[1].contains("⠋ claude"), "{spinners:?}");
}

#[test]
fn a_machine_s_rows_have_no_new_tab_nor_buttons_on_hover() {
    let t = rendered(Sidebar::SideBySide, None);
    let tabs = (0..TALL).filter(|&y| row_text(&t, Rect::new(0, y, W, 1)).contains("└ + Tab")).count();

    assert_eq!(tabs, 1);
    assert!(hovered("login").contains(ROW_MENU_ICON));
    for text in ["api-vps", "zsh"] {
        let row = hovered(text);
        assert!(!row.contains(ROW_MENU_ICON) && !row.contains('×'), "{row}");
    }
}

#[test]
fn a_long_machine_name_is_cut_at_its_end() {
    let mut v = with_machines(Sidebar::SideBySide, None);
    v.workspaces[1].machine.as_mut().expect("a machine").name = "build-server-in-the-basement".into();

    let row = row_of(&render_sized(&v, W, TALL), "api-vps");

    assert!(row.contains("☁ bu") && row.contains('…') && !row.contains("basement"), "{row}");
}

#[test]
fn renders_a_machine_s_projects_in_the_compact_menu() {
    insta::assert_snapshot!(rendered(Sidebar::SideBySide, Some(Nav::Projects)).backend());
}

#[rstest]
#[case::side_by_side(Sidebar::SideBySide, None)]
#[case::projects_on_top(Sidebar::ProjectsOnTop, None)]
#[case::workspaces_on_top(Sidebar::WorkspacesOnTop, None)]
#[case::compact(Sidebar::SideBySide, Some(Nav::Projects))]
fn a_fallen_machine_s_project_is_dim_and_a_linked_one_s_is_not(#[case] sidebar: Sidebar, #[case] nav: Option<Nav>) {
    let t = rendered(sidebar, nav);
    let dim = |text| t.backend().buffer()[find(&t, text)].modifier.contains(Modifier::DIM);

    assert_eq!((dim("notes"), dim("☁ nas"), dim("blog"), dim("web-shop")), (true, true, false, false));
}

#[test]
fn a_machine_s_project_has_no_buttons_on_hover() {
    assert!(hovered("web-shop").contains(ROW_MENU_ICON));
    let row = hovered("blog");
    assert!(!row.contains(ROW_MENU_ICON) && !row.contains('×'), "{row}");
}

#[test]
fn a_machine_s_project_shown_is_the_marked_row_and_its_column_adds_nothing() {
    let t = render_sized(&shown_there(None), W, TALL);
    let bold = |text| t.backend().buffer()[find(&t, text)].modifier.contains(Modifier::BOLD);

    let news = (0..TALL).map(|y| row_text(&t, Rect::new(0, y, W, 1)).matches(NEW_BUTTON).count()).sum::<usize>();

    assert_eq!((bold("blog"), news), (true, 1));
    assert_eq!(spot(&t, TAGLINE), None);
}

#[test]
fn a_machine_s_project_shown_names_the_compact_menu() {
    let t = render_sized(&shown_there(Some(Nav::Workspaces)), SMALL.width, TALL);

    assert!(spot(&t, "api-vps").is_some() && spot(&t, "blog").is_some());
}

#[test]
fn only_a_machine_s_projects_still_welcome() {
    let v = View {
        has_project: false,
        workspaces: Vec::new(),
        projects: vec![only_there("blog", "vps", false)],
        ..view(&[])
    };

    let t = render_sized(&v, W, TALL);

    assert!(spot(&t, TAGLINE).is_some());
    assert!(!t.backend().buffer()[find(&t, "blog")].modifier.contains(Modifier::BOLD));
}

fn machine_tree() -> TreeView {
    let open = |tabs: Vec<u16>| WorkspaceShape { collapsed: false, machine: true, tabs };
    let ours = WorkspaceShape { collapsed: false, machine: false, tabs: vec![1] };
    let paired = ProjectShape { workspaces: vec![ours, open(vec![2, 1]), open(vec![2, 1])], ..ProjectShape::default() };
    let theirs = |tabs| ProjectShape { machine: true, workspaces: vec![open(tabs)], ..ProjectShape::default() };
    let shape = TreeShape {
        groups: Vec::new(),
        projects: vec![paired, theirs(vec![2, 1]), theirs(vec![2, 1])],
        tab_bar: false,
    };
    let local = with_machines(Sidebar::Tree, None).workspaces;
    TreeView { shape, workspaces: vec![local, vec![there("vps", false)], vec![there("nas", true)]] }
}

fn in_tree() -> View<'static> {
    View { tree: Some(machine_tree()), ..with_machines(Sidebar::Tree, None) }
}

#[test]
fn renders_a_machine_s_rows_in_the_tree() {
    insta::assert_snapshot!(render_sized(&in_tree(), W, TALL).backend());
}

#[test]
fn a_machine_s_rows_in_the_tree_have_no_new_rows_nor_arrow_on_its_project() {
    let t = render_sized(&in_tree(), W, TALL);
    let count = |text: &str| (0..TALL).filter(|&y| row_text(&t, Rect::new(0, y, W, 1)).contains(text)).count();

    assert_eq!((count("└ + Tab"), count(NEW_BUTTON)), (1, 2));
    assert!(!row_of(&t, "blog").contains('▾'), "{}", row_of(&t, "blog"));
    assert!(t.backend().buffer()[find(&t, "api-nas")].modifier.contains(Modifier::DIM));
}
