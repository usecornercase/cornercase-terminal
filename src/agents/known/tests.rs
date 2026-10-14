use super::*;

#[test]
fn names_and_order_keep_the_saved_stats_positions() {
    assert_eq!(Known::ALL.map(Known::name), ["claude", "codex", "gemini", "opencode", "grok"]);
}

#[test]
fn agents_whose_records_are_found_by_folder_take_one_role_per_worktree() {
    let alone: Vec<Known> = Known::ALL.into_iter().filter(|kind| kind.one_per_folder()).collect();
    assert_eq!(alone, [Known::Codex, Known::Gemini, Known::Opencode]);
}
