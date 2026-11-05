use super::*;
use crate::ui::{recap_button, worktrees_button};

const COMPACT_SIZE: Rect = Rect { x: 0, y: 0, width: 70, height: 30 };

fn rows(sidebar: Sidebar) -> Areas {
    layout_with(Rect { x: 0, y: 0, width: W, height: 30 }, Widths::default(), false, sidebar)
}

#[rstest]
#[case::side_by_side(Sidebar::SideBySide)]
#[case::projects_on_top(Sidebar::ProjectsOnTop)]
#[case::workspaces_on_top(Sidebar::WorkspacesOnTop)]
#[case::tree(Sidebar::Tree)]
fn recap_shares_the_worktrees_row_in_every_layout_without_a_row_of_its_own(#[case] sidebar: Sidebar) {
    let a = rows(sidebar);
    let (worktrees, recap) = (worktrees_button(a.worktrees), recap_button(a.worktrees));
    assert!(!recap.is_empty() && !worktrees.is_empty(), "{:?}", a.worktrees);
    assert_eq!((recap.y, recap.height, worktrees.y, worktrees.height), (a.worktrees.y, 1, a.worktrees.y, 1));
    assert!(worktrees.right() <= recap.x, "{worktrees:?} {recap:?}");
    assert!(recap.right() <= a.worktrees.right() && worktrees.x == a.worktrees.x);
}

#[test]
fn the_footer_keeps_its_rows_in_the_stacked_layouts() {
    let a = rows(Sidebar::WorkspacesOnTop);
    assert_eq!(
        (a.separator.y, a.issues.y, a.todo_button.y, a.worktrees.y, a.more_separator.y, a.more.y),
        (24, 25, 26, 27, 28, 29)
    );
}

#[test]
fn recap_sits_in_the_compact_menus_worktrees_band() {
    let a = layout(COMPACT_SIZE, Widths::default());
    assert!(a.compact());
    assert!(!recap_button(a.worktrees).is_empty());
    assert!(worktrees_button(a.worktrees).right() <= recap_button(a.worktrees).x);
}

#[test]
fn a_row_too_narrow_for_both_buttons_keeps_worktrees_whole_and_hides_recap() {
    let row = Rect::new(0, 20, 16, 1);
    assert_eq!((recap_button(row), worktrees_button(row)), (Rect::default(), row));
}
