use super::*;

#[test]
fn sidebar_has_fixed_width_on_the_left_below_the_header() {
    assert_eq!(areas().sidebar, Rect::new(0, HEADER_HEIGHT, SIDEBAR_WIDTH, H - HEADER_HEIGHT));
}

#[test]
fn workspaces_column_sits_right_of_the_sidebar() {
    assert_eq!(areas().workspaces, Rect::new(SIDEBAR_WIDTH, 0, WORKSPACES_WIDTH, H));
}

#[test]
fn workspaces_list_starts_level_with_the_projects_list() {
    assert_eq!((areas().workspaces_list.y, areas().workspaces_title.y), (list().y, areas().title.y));
}

#[test]
fn pane_takes_the_rest_after_a_padding_column() {
    let x = SIDEBAR_WIDTH + WORKSPACES_WIDTH + PANE_PADDING;
    assert_eq!(areas().pane, Rect::new(x, 0, W - x, H));
}

#[test]
fn search_spans_both_columns_on_the_first_row() {
    let search = areas().search;
    assert_eq!(search, Rect::new(1, 0, SIDEBAR_WIDTH + WORKSPACES_WIDTH - 3, 1));
}

#[test]
fn title_leaves_a_blank_row_below_the_search() {
    assert_eq!(areas().title.y, areas().search.bottom() + 1);
}

#[test]
fn list_leaves_a_blank_row_below_the_title() {
    assert_eq!(list().y, areas().title.bottom() + GAP);
}

#[test]
fn list_ends_before_the_border() {
    assert_eq!(list().right(), SIDEBAR_WIDTH - 1);
}

#[test]
fn list_ends_right_above_the_separator() {
    assert_eq!(list().bottom(), areas().separator.y);
}

#[test]
fn settings_and_usage_buttons_sit_right_above_the_quit_button() {
    let row = |y| Rect::new(0, y, SIDEBAR_WIDTH - 1, 1);
    assert_eq!((areas().settings, areas().usage), (row(H - 3), row(H - 2)));
}

#[test]
fn quit_button_takes_the_last_row() {
    assert_eq!(areas().quit, Rect::new(0, H - 1, SIDEBAR_WIDTH - 1, 1));
}

#[test]
fn results_cover_both_columns_below_the_header() {
    let results = areas().results;
    assert_eq!(results, Rect::new(0, HEADER_HEIGHT, SIDEBAR_WIDTH + WORKSPACES_WIDTH - 1, H - HEADER_HEIGHT));
}
