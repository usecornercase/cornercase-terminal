use rstest::rstest;
use serde_json::json;

use super::*;
use crate::text::GB;

#[rstest]
#[case::read(json!({"total": 8 * GB, "available": 3 * GB}), Some(Memory { total: 8 * GB, available: 3 * GB }))]
#[case::more_available_than_total(json!({"total": GB, "available": 2 * GB}), Some(Memory { total: GB, available: GB }))]
#[case::no_total(json!({"available": GB}), None)]
fn a_reading_is_known_only_with_a_total(#[case] sent: serde_json::Value, #[case] memory: Option<Memory>) {
    let info: MemoryInfo = serde_json::from_value(sent).expect("a reading");

    assert_eq!(info.memory(), memory);
}
