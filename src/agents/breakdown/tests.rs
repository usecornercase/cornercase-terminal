use rstest::rstest;

use super::*;

#[rstest]
#[case::small(634, "634")]
#[case::thousands(2_700, "2.7k")]
#[case::round_thousand(1_000, "1k")]
#[case::tens(19_651, "19.7k")]
#[case::hundreds(884_900, "884.9k")]
#[case::million(1_000_000, "1M")]
#[case::millions(2_500_000, "2.5M")]
fn tokens_are_abbreviated(#[case] n: u64, #[case] expected: &str) {
    assert_eq!(tokens(n), expected);
}

#[rstest]
#[case::tenth(3_000, 1_000_000, "0.3%")]
#[case::all(10, 10, "100.0%")]
#[case::no_window(5, 0, "500.0%")]
fn shares_have_a_decimal(#[case] n: u64, #[case] max: u64, #[case] expected: &str) {
    assert_eq!(share(n, max), expected);
}

#[rstest]
#[case::now(5, "read just now")]
#[case::minutes(125, "read 2m ago")]
fn reads_are_dated(#[case] secs: u64, #[case] expected: &str) {
    assert_eq!(read_ago(secs), expected);
}

#[rstest]
#[case("used", Slot::Used)]
#[case("free", Slot::Free)]
#[case("buffer", Slot::Buffer)]
#[case("deferred", Slot::Deferred)]
#[case("weird", Slot::Other)]
fn a_category_is_placed_by_its_kind(#[case] kind: &str, #[case] slot: Slot) {
    let category = Category { id: "messages".into(), name: "Free space".into(), tokens: 1, kind: kind.into() };
    assert_eq!(category.slot(), slot);
}
