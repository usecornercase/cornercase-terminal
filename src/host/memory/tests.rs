use rstest::rstest;

use super::*;
use crate::text::GB;

#[rstest]
#[case::total_and_available("32GB,9GB", Some(Memory { total: 32 * GB, available: 9 * GB }))]
#[case::with_spaces(" 16 GB , 2 GB ", Some(Memory { total: 16 * GB, available: 2 * GB }))]
#[case::available_above_total("4GB,8GB", Some(Memory { total: 4 * GB, available: 4 * GB }))]
#[case::one_amount("32GB", None)]
#[case::not_an_amount("lots,some", None)]
fn given_reads_a_fixed_reading(#[case] value: &str, #[case] read: Option<Memory>) {
    assert_eq!(given(value), read);
}

#[test]
fn free_of_says_both_amounts() {
    assert_eq!(free_of(GB * 92 / 10, 32 * GB), "9.2 GB free of 32 GB");
}

#[rstest]
#[case::whole_gigabytes(4 * GB, "4 GB")]
#[case::tenths(GB * 41 / 10, "4.1 GB")]
#[case::megabytes(512 << 20, "512 MB")]
fn shown_drops_a_zero_tenth(#[case] bytes: u64, #[case] text: &str) {
    assert_eq!(shown(bytes), text);
}
