use super::*;

const TALL: u16 = 22;

fn group(name: &str, icon: char, colour: u8) -> GroupEntry {
    GroupEntry { name: name.into(), icon, colour, collapsed: false }
}

fn grouped(active: usize, collapsed: bool) -> View<'static> {
    let mut v = View { active, has_project: true, ..view(&["tmp", "api", "web", "cornercase"]) };
    v.groups = vec![group("work", '●', 4), group("oss", '★', 99)];
    v.groups[0].collapsed = collapsed;
    for (p, g) in [(1, 0), (2, 0), (3, 1)] {
        v.projects[p].group = Some(g);
    }
    v
}

fn tall_list() -> Rect {
    layout(Rect::new(0, 0, W, TALL), Widths::default()).list
}

#[rstest]
#[case::no_groups(&[None, None], &[], &[SidebarRow::Project(0), SidebarRow::Project(1)])]
#[case::loose_projects_first(&[Some(0), None], &[false], &[
    SidebarRow::Project(1),
    SidebarRow::Gap,
    SidebarRow::Group(0),
    SidebarRow::Project(0),
])]
#[case::no_gap_without_loose_projects(&[Some(0)], &[false], &[SidebarRow::Group(0), SidebarRow::Project(0)])]
#[case::collapsed_hides_its_projects(&[Some(0), Some(1)], &[true, false], &[
    SidebarRow::Group(0),
    SidebarRow::Gap,
    SidebarRow::Group(1),
    SidebarRow::Project(1),
])]
#[case::an_empty_group_keeps_its_header(&[None], &[false], &[
    SidebarRow::Project(0),
    SidebarRow::Gap,
    SidebarRow::Group(0),
])]
fn rows(#[case] groups: &[Option<usize>], #[case] collapsed: &[bool], #[case] expected: &[SidebarRow]) {
    assert_eq!(sidebar_rows(groups, collapsed), expected);
}

#[rstest]
#[case::loose_project(0, Some(SidebarHit::Select(0)))]
#[case::gap(1, None)]
#[case::header(2, Some(SidebarHit::Group(0)))]
#[case::grouped_project(3, Some(SidebarHit::Select(1)))]
fn hit_testing_follows_the_rows(#[case] row: u16, #[case] expected: Option<SidebarHit>) {
    let rows = grouped(0, false).sidebar_rows();
    assert_eq!(sidebar_hit(list(), 1, &rows, 0, Position::new(3, list().y + row)), expected);
}

#[rstest]
#[case::grouped_project(SidebarRow::Project(2), SidebarHit::Close(2))]
#[case::header(SidebarRow::Group(0), SidebarHit::CloseGroup(0))]
fn the_close_button_closes_its_row(#[case] row: SidebarRow, #[case] expected: SidebarHit) {
    let rows = grouped(0, false).sidebar_rows();
    let pos = close_button(list(), 1, &rows, 0, row).as_position();
    assert_eq!(sidebar_hit(list(), 1, &rows, 0, pos), Some(expected));
}

#[rstest]
#[case::grouped_project(SidebarRow::Project(2), SidebarHit::Menu(2))]
#[case::header(SidebarRow::Group(0), SidebarHit::GroupMenu(0))]
fn the_menu_button_opens_its_row_menu(#[case] row: SidebarRow, #[case] expected: SidebarHit) {
    let rows = grouped(0, false).sidebar_rows();
    let pos = row_menu_button(entry_row(list(), 1, &rows, 0, row), 1).as_position();
    assert_eq!(sidebar_hit(list(), 1, &rows, 0, pos), Some(expected));
}

#[test]
fn renders_expanded_groups_with_the_active_project_inside() {
    insta::assert_snapshot!(render_sized(&grouped(1, false), W, TALL).backend());
}

#[test]
fn a_collapsed_group_holding_the_active_project_marks_its_header() {
    insta::assert_snapshot!(render_sized(&grouped(1, true), W, TALL).backend());
}

#[test]
fn a_collapsed_group_without_the_active_project_is_not_marked() {
    insta::assert_snapshot!(render_sized(&grouped(0, true), W, TALL).backend());
}

#[test]
fn counts_can_be_turned_off() {
    let row_of = |v: &View, row| {
        let r = entry_row(tall_list(), 1, &v.sidebar_rows(), 0, row);
        row_text(&render_sized(v, W, TALL), r)
    };
    let on = grouped(0, true);
    let off = View { counts: false, ..grouped(0, true) };
    let rows = [SidebarRow::Group(0), SidebarRow::Project(0)];
    assert_eq!(rows.map(|row| row_of(&on, row).contains('(')), [true, true]);
    assert_eq!(rows.map(|row| row_of(&off, row).contains('(')), [false, false]);
}

#[test]
fn the_header_takes_the_group_colour() {
    let rows = grouped(0, false).sidebar_rows();
    let header = entry_row(tall_list(), 1, &rows, 0, SidebarRow::Group(1));
    let t = render_sized(&grouped(0, false), W, TALL);
    assert_eq!(t.backend().buffer()[(header.x + 4, header.y)].fg, Color::Indexed(99));
}

fn styling(icon: char, colour: u8) -> View<'static> {
    let group = GroupEntry { name: "work".into(), icon, colour, collapsed: false };
    View { overlay: Some(Overlay::GroupStyle(group)), ..grouped(1, false) }
}

#[test]
fn every_preset_is_offered_in_the_modal() {
    assert!(
        GROUP_STYLES.iter().all(|(icon, colour)| GROUP_ICONS.contains(icon) && GROUP_COLOURS.contains(colour))
    );
}

#[test]
fn renders_the_icon_and_colour_modal() {
    insta::assert_snapshot!(render(&styling('★', 99)).backend());
}

#[test]
fn the_chosen_icon_is_highlighted() {
    let t = render(&styling('★', 99));
    let at = |i: usize| t.backend().buffer()[style_icon(AREA, i).as_position()].bg;
    assert_eq!((at(7), at(0)), (Color::Cyan, Color::Reset));
}

#[test]
fn each_swatch_shows_its_colour() {
    let t = render(&styling('★', 99));
    let fg = |i: usize| {
        let cell = style_colour(AREA, i);
        t.backend().buffer()[(cell.x + 1, cell.y)].fg
    };
    assert_eq!((0..GROUP_COLOURS.len()).map(fg).collect::<Vec<_>>(), GROUP_COLOURS.map(Color::Indexed));
}

#[rstest]
#[case::first_icon(style_icon(AREA, 0), Some(StyleHit::Icon(0)))]
#[case::icon_on_the_second_row(style_icon(AREA, 11), Some(StyleHit::Icon(11)))]
#[case::colour(style_colour(AREA, 5), Some(StyleHit::Colour(5)))]
#[case::colour_on_the_second_row(style_colour(AREA, 15), Some(StyleHit::Colour(15)))]
#[case::done(style_done(AREA), Some(StyleHit::Done))]
#[case::title(Rect { y: form_area(AREA).y, ..style_icon(AREA, 0) }, None)]
fn style_hits(#[case] cell: Rect, #[case] expected: Option<StyleHit>) {
    assert_eq!(style_hit(AREA, Position::new(cell.right() - 1, cell.y)), expected);
}

#[test]
fn the_cells_fit_a_narrow_terminal() {
    let narrow = Rect::new(0, 0, 40, 20);
    let all =
        (0..GROUP_ICONS.len()).map(|i| style_icon(narrow, i)).chain((0..16).map(|i| style_colour(narrow, i)));
    assert!(all.into_iter().all(|r| r.width >= ICON_CELL));
}

#[test]
fn the_more_counts_skip_the_gaps() {
    let short = H - 3;
    let t = render_sized(&View { projects_scroll: 2, ..grouped(0, false) }, W, short);
    let above = row_text(&t, more_above(layout(Rect::new(0, 0, W, short), Widths::default()).list));
    assert!(above.contains("↑ 1 more"), "{above:?}");
}
