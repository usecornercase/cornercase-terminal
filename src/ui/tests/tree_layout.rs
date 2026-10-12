use super::*;

const TALL: Rect = Rect { x: 0, y: 0, width: W, height: 30 };

fn tree() -> Areas {
    layout_with(TALL, Widths::default(), false, Sidebar::Tree)
}

#[test]
fn one_list_runs_from_its_title_to_the_footer() {
    let a = tree();
    assert_eq!(
        (a.title.y, a.list.y, a.list.bottom(), a.separator.y, a.issues.y, a.todo_button.y, a.quit.y),
        (HEADER_HEIGHT, HEADER_HEIGHT + 2, 24, 24, 25, 26, 29)
    );
}

#[test]
fn has_no_workspaces_list_and_no_line() {
    let a = tree();
    assert_eq!([a.workspaces, a.workspaces_list, a.stack_border], [Rect::default(); 3]);
}

#[test]
fn the_pane_gets_the_width_of_the_workspaces_column() {
    let side = layout(TALL, Widths::default());
    assert_eq!(tree().pane.width, side.pane.width + WORKSPACES_WIDTH);
}

#[test]
fn the_wheel_scrolls_it_anywhere_in_the_column() {
    let a = tree();
    assert_eq!((a.sidebar.x, a.sidebar.width, a.sidebar.contains(a.list.as_position())), (0, SIDEBAR_WIDTH, true));
}

#[test]
fn only_the_tree_says_it_is_one() {
    let trees: Vec<bool> =
        Sidebar::ALL.into_iter().map(|s| layout_with(TALL, Widths::default(), false, s).tree).collect();
    assert_eq!(trees, [false, false, false, true]);
}

#[test]
fn narrow_terminals_get_the_compact_menu() {
    let a = layout_with(Rect { width: 80, ..TALL }, Widths::default(), false, Sidebar::Tree);
    assert_eq!((a.compact(), a.tree), (true, false));
}
