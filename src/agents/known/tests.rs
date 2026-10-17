use super::*;

#[test]
fn names_and_order_keep_the_saved_stats_positions() {
    assert_eq!(Known::ALL.map(Known::name), ["claude", "codex", "gemini", "opencode", "grok", "cursor", "copilot"]);
}

#[test]
fn only_opencode_finds_its_status_by_its_folder() {
    let by_folder: Vec<Known> = Known::ALL.into_iter().filter(|kind| kind.status_by_folder()).collect();
    assert_eq!(by_folder, [Known::Opencode]);
}

#[test]
fn codex_gemini_and_opencode_find_their_records_by_their_folder() {
    let by_folder: Vec<Known> = Known::ALL.into_iter().filter(|kind| kind.record_by_folder()).collect();
    assert_eq!(by_folder, [Known::Codex, Known::Gemini, Known::Opencode]);
}

#[test]
fn claude_codex_cursor_and_copilot_take_one_more_folder_by_flag() {
    let flags: Vec<(Known, Option<&str>)> = Known::ALL.into_iter().map(|kind| (kind, kind.add_dir())).collect();
    assert_eq!(
        flags,
        [
            (Known::Claude, Some("--add-dir")),
            (Known::Codex, Some("--add-dir")),
            (Known::Gemini, None),
            (Known::Opencode, None),
            (Known::Grok, None),
            (Known::Cursor, Some("--add-dir")),
            (Known::Copilot, Some("--add-dir")),
        ]
    );
}
