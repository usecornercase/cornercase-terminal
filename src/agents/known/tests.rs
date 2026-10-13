use super::*;

#[test]
fn names_and_order_keep_the_saved_stats_positions() {
    assert_eq!(Known::ALL.map(Known::name), ["claude", "codex", "gemini", "opencode", "grok"]);
}
