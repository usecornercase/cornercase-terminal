use super::*;

#[derive(Debug, Clone, Copy)]
enum Row {
    Project(usize),
    Group,
    Workspace,
    Tab(usize),
}

impl Row {
    fn nav(self) -> Nav {
        match self {
            Row::Project(_) | Row::Group => Nav::Projects,
            Row::Workspace | Row::Tab(_) => Nav::Workspaces,
        }
    }

    fn active(self) -> bool {
        matches!(self, Row::Project(0) | Row::Tab(0))
    }
}

const COMPACT: Rect = Rect { x: 0, y: 0, width: 80, height: 30 };

fn sample(light: bool) -> View<'static> {
    let mut v = View {
        has_project: true,
        workspaces: vec![WorkspaceEntry {
            collapsed: false,
            name: "login".into(),
            tabs: vec!["claude".into(), "nvim".into()],
            behind: 3,
            pull: None,
            removing: false,
        }],
        active_tab: Some(0),
        light,
        ..view(&["cornercase", "shop"])
    };
    v.groups = vec![GroupEntry { name: "work".into(), icon: '●', colour: 4, collapsed: false }];
    v.projects[1].group = Some(0);
    v
}

fn rect(v: &View, size: Rect, row: Row) -> Rect {
    let a = layout(size, v.widths).shown(v.nav);
    let sidebar = |r| entry_row(a.list, a.pitch, &v.sidebar_rows(), v.projects_scroll, r);
    let workspaces = |r| workspace_row(a.workspaces_list, a.pitch, &v.tab_lines(), v.workspaces_scroll, r);
    match row {
        Row::Project(p) => sidebar(SidebarRow::Project(p)),
        Row::Group => sidebar(SidebarRow::Group(0)),
        Row::Workspace => workspaces(WorkspaceRow::Workspace(0)),
        Row::Tab(t) => workspaces(WorkspaceRow::Tab(0, t)),
    }
}

fn backgrounds(v: &View, size: Rect, r: Rect) -> Vec<Color> {
    let t = render_sized(v, size.width, size.height);
    let mut colours: Vec<Color> = r.positions().map(|p| t.backend().buffer()[p].bg).collect::<Vec<_>>();
    colours.dedup();
    colours
}

fn hovering(mut v: View<'static>, size: Rect, row: Row) -> (View<'static>, Rect) {
    let r = rect(&v, size, row);
    v.hover = Some(Position::new(r.x + 4, middle(r).y));
    (v, r)
}

#[rstest]
fn a_hovered_row_is_filled_edge_to_edge(
    #[values(Row::Project(0), Row::Project(1), Row::Group, Row::Workspace, Row::Tab(0), Row::Tab(1))] row: Row,
    #[values(false, true)] light: bool,
    #[values(false, true)] compact: bool,
) {
    let (size, nav) = if compact { (COMPACT, Some(row.nav())) } else { (AREA, None) };
    let (v, r) = hovering(View { nav, ..sample(light) }, size, row);
    let expected = match (row.active(), light) {
        (true, false) => DARK_SURFACE,
        (true, true) => LIGHT_SURFACE,
        (false, false) => DARK_HOVER,
        (false, true) => LIGHT_HOVER,
    };
    assert_eq!(backgrounds(&v, size, r), vec![expected]);
}

#[test]
fn the_close_button_and_the_behind_tag_sit_on_the_hover_background() {
    let (v, r) = hovering(sample(false), AREA, Row::Workspace);
    let t = render(&v);
    let cells: Vec<(String, Color)> = [r.right() - 2, r.right() - 7]
        .into_iter()
        .map(|x| t.backend().buffer()[(x, r.y)].clone())
        .map(|c| (c.symbol().to_string(), c.bg))
        .collect();
    assert_eq!(cells, vec![("×".into(), DARK_HOVER), ("3".into(), DARK_HOVER)]);
}

#[rstest]
fn a_hovered_group_header_shows_its_close_button(#[values(false, true)] compact: bool) {
    let (size, nav) = if compact { (COMPACT, Some(Nav::Projects)) } else { (AREA, None) };
    let (v, r) = hovering(View { nav, ..sample(false) }, size, Row::Group);
    let close = row_close_button(r, layout(size, v.widths).pitch);
    let t = render_sized(&v, size.width, size.height);
    let cell = t.backend().buffer()[(close.x + close.width / 2, middle(close).y)].clone();
    assert_eq!((cell.symbol(), cell.fg), ("×", Color::DarkGray));
}

#[test]
fn a_long_group_name_is_cut_before_the_close_button() {
    let mut v = sample(false);
    v.groups[0].name = "x".repeat(200);
    let (v, r) = hovering(v, AREA, Row::Group);
    let menu = row_menu_button(r, 1);
    let t = render(&v);
    let cell = |x: u16| t.backend().buffer()[(x, r.y)].symbol().to_string();
    assert_eq!([cell(menu.x - 2), cell(menu.x - 1)], ["…", " "]);
}

#[test]
fn other_rows_stay_plain() {
    let (v, _) = hovering(sample(false), AREA, Row::Tab(1));
    assert_eq!(backgrounds(&v, AREA, rect(&v, AREA, Row::Project(1))), vec![Color::Reset]);
}

#[test]
fn no_row_is_lit_while_an_overlay_is_open() {
    let (mut v, r) = hovering(sample(false), AREA, Row::Project(1));
    v.overlay = Some(Overlay::Menu { at: Position::new(80, 1), items: vec!["rename tab".into()] });
    assert_eq!(backgrounds(&v, AREA, r), vec![Color::Reset]);
}

#[test]
fn no_row_is_lit_while_dragging_a_border() {
    let (v, r) = hovering(sample(false), AREA, Row::Tab(1));
    let v = View { resizing: Some(Border::Workspaces), ..v };
    assert_eq!(backgrounds(&v, AREA, r), vec![Color::Reset]);
}

#[test]
fn no_row_is_lit_while_dragging_a_divider() {
    let (v, r) = hovering(sample(false), AREA, Row::Tab(1));
    let tab = TabView { dragging: Some(Vec::new()), ..single(screen(b"")) };
    let v = View { tab: Some(tab), ..v };
    assert_eq!(backgrounds(&v, AREA, r), vec![Color::Reset]);
}

#[test]
fn no_row_is_lit_and_no_close_button_shows_while_dragging_a_row() {
    let (v, r) = hovering(sample(false), AREA, Row::Tab(1));
    let v = View { drag: Some(Drag::Sidebar(SidebarRow::Project(1), None)), ..v };
    let t = render(&v);
    assert_eq!(
        (backgrounds(&v, AREA, r), t.backend().buffer()[(r.right() - 2, r.y)].symbol()),
        (vec![Color::Reset], " ")
    );
}

#[rstest]
fn the_dragged_row_keeps_the_surface_background(
    #[values(Row::Project(1), Row::Group, Row::Workspace, Row::Tab(1))] row: Row,
    #[values(false, true)] compact: bool,
) {
    let (size, nav) = if compact { (COMPACT, Some(row.nav())) } else { (AREA, None) };
    let v = View { nav, ..sample(false) };
    let drag = match row {
        Row::Project(p) => Drag::Sidebar(SidebarRow::Project(p), None),
        Row::Group => Drag::Sidebar(SidebarRow::Group(0), None),
        Row::Workspace => Drag::Workspaces(WorkspaceRow::Workspace(0), None),
        Row::Tab(t) => Drag::Workspaces(WorkspaceRow::Tab(0, t), None),
    };
    let v = View { drag: Some(drag), ..v };
    assert_eq!(backgrounds(&v, size, rect(&v, size, row)), vec![DARK_SURFACE]);
}
