use rstest::rstest;

use super::*;

const AREA: Rect = Rect { x: 10, y: 2, width: 42, height: 21 };

fn pair(dir: Dir) -> Node<u64> {
    let mut node = Node::Leaf(1);
    node.split(1, dir, 2);
    node
}

fn three() -> Node<u64> {
    let mut node = pair(Dir::Right);
    node.split(2, Dir::Down, 3);
    node
}

mod neighbours {
    use super::*;

    #[rstest]
    #[case::left_of_the_top_right(2, Place::Left, Some(1))]
    #[case::below_the_top_right(2, Place::Below, Some(3))]
    #[case::above_the_bottom_right(3, Place::Above, Some(2))]
    #[case::nothing_at_the_edge(1, Place::Left, None)]
    #[case::nothing_above_the_top(2, Place::Above, None)]
    fn finds_the_pane_on_that_side(#[case] from: u64, #[case] side: Place, #[case] expected: Option<u64>) {
        assert_eq!(beside(&three().panes(AREA), from, side), expected);
    }

    #[test]
    fn takes_the_pane_sharing_the_most_of_its_side() {
        let mut node = pair(Dir::Down);
        node.split(1, Dir::Right, 3);
        node.set_ratio(&[true], 0.8);
        assert_eq!(beside(&node.panes(AREA), 2, Place::Above), Some(1));
    }
}

mod geometry {
    use super::*;

    #[test]
    fn a_leaf_takes_the_whole_area() {
        assert_eq!(Node::Leaf(7).panes(AREA), vec![(7, AREA)]);
    }

    #[test]
    fn split_right_leaves_the_divider_and_a_blank_column_between_the_halves() {
        let panes = pair(Dir::Right).panes(AREA);
        assert_eq!(panes, vec![(1, Rect::new(10, 2, 20, 21)), (2, Rect::new(32, 2, 20, 21))]);
    }

    #[test]
    fn split_down_leaves_one_row_between_the_halves() {
        let panes = pair(Dir::Down).panes(AREA);
        assert_eq!(panes, vec![(1, Rect::new(10, 2, 42, 10)), (2, Rect::new(10, 13, 42, 10))]);
    }

    #[test]
    fn the_divider_sits_between_the_halves() {
        let dividers = pair(Dir::Right).dividers(AREA);
        assert_eq!(dividers.iter().map(|d| d.line).collect::<Vec<_>>(), vec![Rect::new(30, 2, 1, 21)]);
    }

    #[test]
    fn nested_dividers_carry_their_path() {
        let paths: Vec<Vec<bool>> = three().dividers(AREA).into_iter().map(|d| d.path).collect();
        assert_eq!(paths, vec![vec![], vec![true]]);
    }

    #[test]
    fn halves_never_shrink_below_their_minimum() {
        let mut node = pair(Dir::Right);
        node.set_ratio(&[], 0.0);
        assert_eq!(node.panes(AREA)[0].1.width, MIN_COLS);
    }

    #[test]
    fn a_pane_is_found_under_the_mouse() {
        assert_eq!(three().pane_at(AREA, 1, Position::new(40, 20)), Some(3));
    }

    #[test]
    fn the_divider_is_not_a_pane() {
        assert_eq!(pair(Dir::Right).pane_at(AREA, 1, Position::new(30, 5)), None);
    }

    #[test]
    fn the_blank_column_after_the_divider_also_grabs_it() {
        assert_eq!(pair(Dir::Right).divider_at(AREA, Position::new(31, 5)).map(|d| d.path), Some(vec![]));
    }
}

mod room {
    use super::*;

    const NARROW: Rect = Rect { width: 15, ..AREA };

    fn stacked() -> Node<u64> {
        let mut node = pair(Dir::Down);
        node.split(2, Dir::Down, 3);
        node
    }

    #[rstest]
    #[case::a_pane(Node::Leaf(1), (MIN_COLS, MIN_ROWS))]
    #[case::side_by_side(pair(Dir::Right), (2 * MIN_COLS + 2, MIN_ROWS))]
    #[case::one_above_the_other(pair(Dir::Down), (MIN_COLS, 2 * MIN_ROWS + 1))]
    #[case::nested(three(), (2 * MIN_COLS + 2, 2 * MIN_ROWS + 1))]
    fn a_tree_needs_room_for_every_pane(#[case] node: Node<u64>, #[case] expected: (u16, u16)) {
        assert_eq!((node.min_size(Dir::Right), node.min_size(Dir::Down)), expected);
    }

    #[test]
    fn dragging_the_outer_divider_to_the_edge_keeps_the_nested_panes() {
        let mut node = stacked();
        let outer = node.dividers(AREA).remove(0);
        node.set_ratio(&outer.path, ratio_at(&outer, Position::new(AREA.x, AREA.bottom() - 1)));
        assert!(node.panes(AREA).iter().all(|(_, r)| r.height >= MIN_ROWS), "{:?}", node.panes(AREA));
    }

    #[test]
    fn dragging_to_the_other_edge_keeps_the_first_pane() {
        let mut node = pair(Dir::Right);
        let divider = node.dividers(AREA).remove(0);
        node.set_ratio(&divider.path, ratio_at(&divider, Position::new(AREA.x, 5)));
        assert_eq!(node.panes(AREA)[0].1.width, MIN_COLS);
    }

    #[test]
    fn a_smaller_area_keeps_every_pane_while_the_tree_fits() {
        let mut node = stacked();
        node.set_ratio(&[], 0.9);
        let low = Rect { height: stacked().min_size(Dir::Down), ..AREA };
        assert!(node.panes(low).iter().all(|(_, r)| r.height >= MIN_ROWS), "{:?}", node.panes(low));
    }

    #[test]
    fn with_room_every_pane_shows() {
        assert_eq!(three().visible(AREA, 3), three().panes(AREA));
    }

    #[test]
    fn without_room_for_every_pane_only_the_active_one_shows() {
        assert_eq!(three().visible(NARROW, 3), vec![(3, NARROW)]);
    }

    #[test]
    fn a_single_pane_always_shows() {
        let tiny = Rect { width: 3, height: 1, ..AREA };
        assert_eq!(Node::Leaf(1).visible(tiny, 1), vec![(1, tiny)]);
    }

    #[test]
    fn without_room_there_are_no_dividers() {
        assert_eq!(three().dividers(NARROW), Vec::new());
    }

    #[test]
    fn without_room_the_hidden_panes_have_no_place() {
        let tree = three();
        let under = tree.pane_at(NARROW, 3, Position::new(NARROW.x, NARROW.y));
        assert_eq!((tree.pane(NARROW, 3, 1), tree.pane(NARROW, 3, 3), under), (None, Some(NARROW), Some(3)));
    }
}

mod editing {
    use super::*;

    #[test]
    fn removing_a_pane_gives_its_space_to_the_sibling() {
        let mut node = three();
        node.remove(3);
        assert_eq!(node, pair(Dir::Right));
    }

    #[test]
    fn removing_the_first_half_keeps_the_second() {
        let mut node = pair(Dir::Right);
        node.remove(1);
        assert_eq!(node, Node::Leaf(2));
    }

    #[rstest]
    #[case::the_first_goes_to_the_nearest_of_the_rest(1, Some(2))]
    #[case::the_last_goes_to_its_sibling(3, Some(2))]
    #[case::a_missing_one_changes_nothing(9, None)]
    fn removing_names_the_pane_that_takes_the_space(#[case] removed: u64, #[case] near: Option<u64>) {
        assert_eq!(three().remove(removed), near);
    }

    #[test]
    fn the_ratio_of_a_nested_split_changes_by_path() {
        let mut node = three();
        node.set_ratio(&[true], 0.25);
        let Node::Split { second, .. } = node else { panic!("expected a split") };
        assert!(matches!(*second, Node::Split { ratio, .. } if (ratio - 0.25).abs() < f32::EPSILON));
    }

    #[test]
    fn dragging_puts_the_divider_under_the_mouse() {
        let mut node = pair(Dir::Right);
        let divider = node.dividers(AREA).remove(0);
        node.set_ratio(&divider.path, ratio_at(&divider, Position::new(20, 5)));
        assert_eq!(node.dividers(AREA)[0].line.x, 20);
    }

    #[test]
    fn a_row_shares_the_width() {
        let widths: Vec<u16> = row(&[1, 2, 3]).expect("a row").panes(AREA).iter().map(|(_, r)| r.width).collect();
        assert_eq!(widths, vec![13, 13, 12]);
    }

    #[test]
    fn map_drops_the_tree_when_an_id_is_unknown() {
        assert_eq!(three().map(&|id| (id != 3).then_some(id)), None);
    }

    #[test]
    fn two_panes_side_by_side_can_be_stacked() {
        assert_eq!(pair(Dir::Right).moved(2, 1, Place::Below), Some(pair(Dir::Down)));
    }

    #[test]
    fn a_pane_dropped_above_its_sibling_goes_first() {
        let expected = Node::Split {
            dir: Dir::Down,
            ratio: HALF,
            first: Box::new(Node::Leaf(2)),
            second: Box::new(Node::Leaf(1)),
        };
        assert_eq!(pair(Dir::Right).moved(2, 1, Place::Above), Some(expected));
    }

    #[rstest]
    #[case::to_the_left(Place::Left, Dir::Right, [3, 1])]
    #[case::to_the_right(Place::Right, Dir::Right, [1, 3])]
    #[case::above(Place::Above, Dir::Down, [3, 1])]
    #[case::below(Place::Below, Dir::Down, [1, 3])]
    fn a_moved_pane_leaves_its_place_and_splits_the_target(
        #[case] place: Place,
        #[case] dir: Dir,
        #[case] order: [u64; 2],
    ) {
        let [first, second] = order;
        let mut target = Node::Leaf(first);
        target.split(first, dir, second);
        let expected =
            Node::Split { dir: Dir::Right, ratio: HALF, first: Box::new(target), second: Box::new(Node::Leaf(2)) };
        assert_eq!(three().moved(3, 1, place), Some(expected));
    }

    #[test]
    fn swapping_keeps_the_layout() {
        let swapped = three().moved(1, 3, Place::Swap).expect("a new layout");
        let mut expected = Node::Leaf(3);
        expected.split(3, Dir::Right, 2);
        expected.split(2, Dir::Down, 1);
        assert_eq!(swapped, expected);
    }

    #[rstest]
    #[case::onto_itself(1, 1)]
    #[case::an_unknown_pane(9, 1)]
    #[case::onto_an_unknown_target(1, 9)]
    fn a_move_that_cannot_happen_gives_nothing(#[case] pane: u64, #[case] target: u64) {
        assert_eq!(three().moved(pane, target, Place::Left), None);
    }

    #[rstest]
    #[case::the_middle(Position::new(20, 10), Place::Swap)]
    #[case::near_the_left_edge(Position::new(1, 10), Place::Left)]
    #[case::near_the_right_edge(Position::new(39, 10), Place::Right)]
    #[case::near_the_top(Position::new(20, 1), Place::Above)]
    #[case::near_the_bottom(Position::new(20, 19), Place::Below)]
    #[case::a_corner_goes_to_the_nearer_edge(Position::new(2, 0), Place::Above)]
    fn where_a_pane_lands_follows_the_pointer(#[case] pos: Position, #[case] expected: Place) {
        assert_eq!(Place::at(Rect::new(0, 0, 40, 20), pos), expected);
    }

    #[rstest]
    #[case::swap(Place::Swap, Rect::new(10, 2, 41, 21))]
    #[case::left(Place::Left, Rect::new(10, 2, 20, 21))]
    #[case::right(Place::Right, Rect::new(31, 2, 20, 21))]
    #[case::above(Place::Above, Rect::new(10, 2, 41, 10))]
    #[case::below(Place::Below, Rect::new(10, 13, 41, 10))]
    fn the_landing_covers_the_half_the_pane_takes(#[case] place: Place, #[case] expected: Rect) {
        assert_eq!(place.area(Rect::new(10, 2, 41, 21)), expected);
    }

    #[rstest]
    #[case::wide_enough_for_right(Rect::new(0, 0, 22, 5), Dir::Right, true)]
    #[case::too_narrow_for_right(Rect::new(0, 0, 21, 50), Dir::Right, false)]
    #[case::tall_enough_for_down(Rect::new(0, 0, 5, 7), Dir::Down, true)]
    #[case::too_short_for_down(Rect::new(0, 0, 80, 6), Dir::Down, false)]
    fn a_split_needs_room_for_both_halves(#[case] area: Rect, #[case] dir: Dir, #[case] expected: bool) {
        assert_eq!(fits(area, dir), expected);
    }
}
