use super::*;

const WIDE: Widths = Widths { projects: 40, workspaces: 30, changes: None, stack: None, agents: None };

#[test]
fn the_changes_panel_takes_half_of_the_free_space_up_to_its_default() {
    let free = 160 - SIDEBAR_WIDTH - WORKSPACES_WIDTH - PANE_PADDING;
    let widths = Widths::default();
    assert_eq!((widths.changes_width(160), widths.changes_width(300)), (free / 2, changes::DEFAULT_WIDTH));
}

#[test]
fn a_dragged_changes_panel_keeps_its_width() {
    let widths = Widths::default().dragged(Border::Changes, 100, 200);
    assert_eq!((widths.changes, widths.changes_width(200)), (Some(100), 100));
}

#[test]
fn the_changes_panel_leaves_room_for_the_pane_and_columns() {
    let widths = Widths { changes: Some(90), ..Widths::default() };
    assert_eq!(widths.changes_width(100), 100 - PANE_PADDING - MIN_PANE_WIDTH - 2 * MIN_COLUMN_WIDTH);
}

#[test]
fn a_double_click_makes_the_changes_panel_automatic_again() {
    let widths = Widths { changes: Some(90), ..Widths::default() };
    assert_eq!(widths.reset(Border::Changes).changes, None);
}

#[test]
fn fit_keeps_widths_that_leave_room_for_the_pane() {
    assert_eq!(WIDE.fit(W), WIDE);
}

#[test]
fn fit_shrinks_the_workspaces_column_first() {
    let room = 80 - PANE_PADDING - MIN_PANE_WIDTH;
    assert_eq!(WIDE.fit(80), Widths { projects: 40, workspaces: room - 40, ..Widths::default() });
}

#[test]
fn fit_shrinks_the_projects_column_once_workspaces_is_at_its_minimum() {
    let room = 60 - PANE_PADDING - MIN_PANE_WIDTH;
    assert_eq!(
        WIDE.fit(60),
        Widths { projects: room - MIN_COLUMN_WIDTH, workspaces: MIN_COLUMN_WIDTH, ..Widths::default() }
    );
}

#[test]
fn fit_never_goes_below_the_minimum() {
    let min = Widths { projects: MIN_COLUMN_WIDTH, workspaces: MIN_COLUMN_WIDTH, ..Widths::default() };
    assert_eq!(WIDE.fit(20), min);
}

#[test]
fn dragging_the_projects_border_puts_it_under_the_mouse() {
    assert_eq!(Widths::default().dragged(Border::Projects, 39, W).projects, 40);
}

#[test]
fn dragging_the_workspaces_border_puts_it_under_the_mouse() {
    let widths = Widths::default().dragged(Border::Workspaces, SIDEBAR_WIDTH + 29, W);
    assert_eq!(widths, Widths { projects: SIDEBAR_WIDTH, workspaces: 30, ..Widths::default() });
}

#[rstest]
#[case::projects(Border::Projects)]
#[case::workspaces(Border::Workspaces)]
fn dragging_stops_at_the_minimum_width(#[case] border: Border) {
    let widths = Widths::default().dragged(border, 0, W);
    let width = if border == Border::Projects { widths.projects } else { widths.workspaces };
    assert_eq!(width, MIN_COLUMN_WIDTH);
}

#[rstest]
#[case::projects(Border::Projects)]
#[case::workspaces(Border::Workspaces)]
fn dragging_leaves_room_for_the_pane(#[case] border: Border) {
    let widths = Widths::default().dragged(border, W - 1, W);
    assert_eq!(layout(AREA, widths).pane.width, MIN_PANE_WIDTH);
}

#[test]
fn reset_brings_back_the_default_width_of_that_column_only() {
    assert_eq!(WIDE.reset(Border::Projects), Widths { projects: SIDEBAR_WIDTH, workspaces: 30, ..Widths::default() });
}

const STACKED: Rect = Rect { x: 0, y: 0, width: W, height: 30 };

#[test]
fn the_line_starts_halfway() {
    assert_eq!(Widths::default().top_rows(20), 10);
}

#[test]
fn a_dragged_line_keeps_its_rows() {
    assert_eq!(Widths { stack: Some(7), ..Widths::default() }.top_rows(20), 7);
}

#[rstest]
#[case::top(0, MIN_STACK_SECTION)]
#[case::bottom(40, 20 - MIN_STACK_SECTION)]
fn the_line_leaves_room_for_both_lists(#[case] stack: u16, #[case] expected: u16) {
    assert_eq!(Widths { stack: Some(stack), ..Widths::default() }.top_rows(20), expected);
}

#[test]
fn a_saved_line_too_low_for_a_short_screen_is_fitted() {
    assert_eq!(Widths { stack: Some(30), ..Widths::default() }.top_rows(12), 12 - MIN_STACK_SECTION);
}

#[test]
fn a_room_too_short_for_both_lists_is_split_in_half() {
    assert_eq!(Widths { stack: Some(1), ..Widths::default() }.top_rows(7), 3);
}

#[test]
fn dragging_the_line_puts_it_under_the_mouse() {
    let widths = Widths::default().stacked_dragged(
        Border::Stack,
        Position::new(5, HEADER_HEIGHT + 12),
        STACKED,
        Screen::default(),
    );
    assert_eq!(widths.stack, Some(12));
}

#[test]
fn the_stacked_column_can_take_the_room_of_the_workspaces_column() {
    let widths = Widths::default().stacked_dragged(Border::Projects, Position::new(59, 6), STACKED, Screen::default());
    assert_eq!(widths.projects, 60);
}

#[test]
fn the_stacked_column_leaves_room_for_the_pane() {
    let widths =
        Widths::default().stacked_dragged(Border::Projects, Position::new(W - 1, 6), STACKED, Screen::default());
    assert_eq!(widths.stacked_width(W), W - PANE_PADDING - MIN_PANE_WIDTH);
}

#[test]
fn a_double_click_splits_the_lists_in_half_again() {
    assert_eq!(Widths { stack: Some(7), ..Widths::default() }.reset(Border::Stack).stack, None);
}

#[test]
fn two_columns_ignore_the_line() {
    assert_eq!(Widths::default().dragged(Border::Stack, 10, W), Widths::default());
}

mod folded_sections {
    use super::*;
    use crate::state::Section;
    use crate::ui::widths::Above;

    #[rstest]
    #[case::the_top_folded(20, (true, false), 1)]
    #[case::the_bottom_folded(20, (false, true), 19)]
    #[case::both_folded(20, (true, true), 1)]
    #[case::no_room(0, (true, false), 0)]
    fn a_folded_stacked_section_is_one_row(#[case] room: u16, #[case] folded: (bool, bool), #[case] top: u16) {
        assert_eq!(Widths::default().stack_rows(room, folded), top);
    }

    #[test]
    fn open_stacked_sections_split_the_room_as_before() {
        let widths = Widths { stack: Some(8), ..Widths::default() };

        assert_eq!(widths.stack_rows(20, (false, false)), widths.top_rows(20));
    }

    #[rstest]
    #[case::folded_whatever_its_saved_height(Above::Open { keep: 5 }, true, 1)]
    #[case::folded_under_folded_lists(Above::Folded { rows: 3 }, true, 1)]
    #[case::open_under_folded_lists(Above::Folded { rows: 3 }, false, 17)]
    #[case::open_under_open_lists(Above::Open { keep: 5 }, false, 15)]
    fn the_agents_section_takes_its_rows(#[case] above: Above, #[case] folded: bool, #[case] rows: u16) {
        let widths = Widths { agents: Some(15), ..Widths::default() };

        assert_eq!(widths.agents_taken(20, above, folded), rows);
    }

    #[test]
    fn dragging_the_line_under_a_folded_agents_section_follows_the_pointer() {
        let screen = Screen {
            sidebar: Sidebar::ProjectsOnTop,
            agents: true,
            folded: Sections::of(&[Section::Agents]),
            ..Screen::default()
        };
        let widths = Widths { agents: Some(15), ..Widths::default() };
        let line = full_layout(STACKED, widths, screen).stack_border;
        let to = Position::new(line.x + 3, line.y + 4);

        let dragged = widths.stacked_dragged(Border::Stack, to, STACKED, screen);

        assert_eq!(full_layout(STACKED, dragged, screen).stack_border.y, to.y);
    }
}
