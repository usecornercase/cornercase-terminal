use rstest::rstest;

use super::*;

#[rstest]
#[case::before_the_active(3, 1, 2)]
#[case::the_active_itself(2, 2, 1)]
#[case::after_the_active(1, 2, 1)]
#[case::the_first_when_active(0, 0, 0)]
fn removing_keeps_the_active_one_in_place(#[case] active: usize, #[case] removed: usize, #[case] expected: usize) {
    let mut a = active;
    shift_active(&mut a, removed);
    assert_eq!(a, expected);
}

#[rstest]
#[case::down(0, 3, "bcad")]
#[case::up(3, 1, "adbc")]
#[case::to_the_end(1, 4, "acdb")]
#[case::to_the_start(2, 0, "cabd")]
#[case::before_itself(1, 1, "abcd")]
#[case::right_after_itself(1, 2, "abcd")]
#[case::past_the_end(1, 9, "abcd")]
fn moving_puts_the_item_before_another(#[case] from: usize, #[case] before: usize, #[case] expected: &str) {
    let mut items: Vec<char> = "abcd".chars().collect();
    move_before(&mut items, from, before, None);
    assert_eq!(items.into_iter().collect::<String>(), expected);
}

#[rstest]
#[case::the_moved_one(1, 3, 1, 2)]
#[case::one_it_passes_down(1, 3, 2, 1)]
#[case::one_it_passes_up(3, 1, 2, 3)]
#[case::one_it_does_not_pass(0, 2, 3, 3)]
fn moving_keeps_the_active_one_active(
    #[case] from: usize,
    #[case] before: usize,
    #[case] active: usize,
    #[case] expected: usize,
) {
    let (mut items, mut a) = ((0..4).collect::<Vec<usize>>(), active);
    let id = items[active];
    move_before(&mut items, from, before, Some(&mut a));
    assert_eq!((a, items[a]), (expected, id));
}
