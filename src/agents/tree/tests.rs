use rstest::rstest;

use super::*;

fn shape(rows: &[(u64, Option<u64>)], narrow: bool) -> Vec<(u64, usize, bool, Vec<bool>)> {
    order(rows, narrow).into_iter().map(|node| (rows[node.index].0, node.depth, node.last, node.rails)).collect()
}

mod order {
    use super::*;

    #[rstest]
    #[case::no_parents_keep_the_sidebar_order(
        &[(1, None), (2, None), (3, None)],
        &[(1, 0, false), (2, 0, false), (3, 0, true)],
    )]
    #[case::children_go_under_their_parent(
        &[(1, None), (2, None), (3, Some(1)), (4, Some(1))],
        &[(1, 0, false), (3, 1, false), (4, 1, true), (2, 0, true)],
    )]
    #[case::a_grandchild_goes_under_its_parent(
        &[(1, None), (2, Some(1)), (3, Some(2))],
        &[(1, 0, true), (2, 1, true), (3, 2, true)],
    )]
    #[case::a_parent_not_listed_makes_a_root(
        &[(1, None), (2, Some(9)), (3, None)],
        &[(1, 0, false), (2, 0, false), (3, 0, true)],
    )]
    #[case::a_child_listed_before_its_parent_moves_under_it(
        &[(5, Some(7)), (6, None), (7, None)],
        &[(6, 0, false), (7, 0, true), (5, 1, true)],
    )]
    #[case::siblings_keep_the_sidebar_order(
        &[(4, Some(1)), (1, None), (3, Some(1)), (2, Some(1))],
        &[(1, 0, true), (4, 1, false), (3, 1, false), (2, 1, true)],
    )]
    #[case::a_pane_that_is_its_own_parent_is_a_root(
        &[(1, Some(1)), (2, None)],
        &[(1, 0, false), (2, 0, true)],
    )]
    #[case::a_cycle_ends_and_lists_every_pane(
        &[(1, Some(2)), (2, Some(1)), (3, None)],
        &[(3, 0, false), (1, 0, true), (2, 1, true)],
    )]
    fn follows_the_tree(#[case] rows: &[(u64, Option<u64>)], #[case] expected: &[(u64, usize, bool)]) {
        let got: Vec<_> = shape(rows, false).into_iter().map(|(id, depth, last, _)| (id, depth, last)).collect();
        assert_eq!(got, expected);
    }
}

mod depth {
    use super::*;

    #[rstest]
    #[case::wide(false, &[0, 1, 2, 3, 3, 3])]
    #[case::narrow(true, &[0, 1, 1, 1, 1, 1])]
    fn is_capped(#[case] narrow: bool, #[case] expected: &[usize]) {
        let rows = [(1, None), (2, Some(1)), (3, Some(2)), (4, Some(3)), (5, Some(4)), (6, Some(5))];
        let depths: Vec<_> = order(&rows, narrow).into_iter().map(|node| node.depth).collect();
        assert_eq!(depths, expected);
    }
}

mod capped {
    use super::*;

    #[rstest]
    #[case::a_grandchild_squashed_in_a_narrow_column(
        true,
        &[(1, None), (2, Some(1)), (3, Some(2))],
        &[(1, 0, true, vec![]), (2, 1, false, vec![]), (3, 1, true, vec![])],
    )]
    #[case::a_fourth_level_squashed_to_the_third(
        false,
        &[(1, None), (2, Some(1)), (3, Some(2)), (4, Some(3)), (5, Some(4))],
        &[
            (1, 0, true, vec![]),
            (2, 1, true, vec![]),
            (3, 2, true, vec![false]),
            (4, 3, false, vec![false, false]),
            (5, 3, true, vec![false, false]),
        ],
    )]
    fn draws_the_depth_it_shows(
        #[case] narrow: bool,
        #[case] rows: &[(u64, Option<u64>)],
        #[case] expected: &[(u64, usize, bool, Vec<bool>)],
    ) {
        assert_eq!(shape(rows, narrow), expected);
    }
}

mod rails {
    use super::*;

    #[rstest]
    #[case::an_ancestor_with_a_later_sibling_draws_its_line(
        &[(1, None), (2, Some(1)), (3, Some(2)), (4, Some(1))],
        &[vec![], vec![], vec![true], vec![]],
    )]
    #[case::the_last_ancestor_draws_none(
        &[(1, None), (2, Some(1)), (3, Some(2)), (4, Some(3))],
        &[vec![], vec![], vec![false], vec![false, false]],
    )]
    #[case::a_capped_row_keeps_the_rails_it_shows(
        &[(1, None), (2, Some(1)), (3, Some(2)), (4, Some(3)), (5, Some(4)), (6, Some(2)), (7, Some(1))],
        &[vec![], vec![], vec![true], vec![true, true], vec![true, true], vec![true], vec![]],
    )]
    fn tell_which_ancestors_go_on(#[case] rows: &[(u64, Option<u64>)], #[case] expected: &[Vec<bool>]) {
        let got: Vec<_> = shape(rows, false).into_iter().map(|(.., rails)| rails).collect();
        assert_eq!(got, expected);
    }

    #[test]
    fn narrow_rows_have_none() {
        let rows = [(1, None), (2, Some(1)), (3, Some(2)), (4, Some(1))];
        assert!(shape(&rows, true).into_iter().all(|(.., rails)| rails.is_empty()));
    }
}
