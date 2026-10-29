use super::*;

#[test]
fn names_and_order_keep_the_saved_stats_positions() {
    assert_eq!(
        Known::ALL.map(Known::name),
        ["claude", "codex", "gemini", "opencode", "grok", "cursor", "copilot", "vibe"]
    );
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
            (Known::Vibe, Some("--add-dir")),
        ]
    );
}

#[test]
fn each_agent_compacts_with_its_own_command() {
    let commands: Vec<_> = Known::ALL.iter().map(|kind| kind.compact().map(|c| (c.command, c.keeps))).collect();
    assert_eq!(
        commands,
        [
            Some(("/compact", true)),
            Some(("/compact", false)),
            Some(("/compress", false)),
            Some(("/compact", false)),
            Some(("/compact", false)),
            Some(("/summarize", false)),
            Some(("/compact", true)),
            Some(("/compact", true)),
        ]
    );
}

#[rstest::rstest]
#[case::with_instructions(Known::Claude, "/compact keep: x", true)]
#[case::padded(Known::Claude, "  /compact", true)]
#[case::longer_word(Known::Claude, "/compactx", false)]
#[case::not_first(Known::Claude, "please /compact", false)]
#[case::gemini_own(Known::Gemini, "/compress", true)]
#[case::gemini_not_claudes(Known::Gemini, "/compact", false)]
#[case::vibe(Known::Vibe, "/compact keep the login", true)]
#[case::vibe_not_claudes(Known::Vibe, "/compress", false)]
fn knows_a_compact_command(#[case] kind: Known, #[case] text: &str, #[case] compacts: bool) {
    assert_eq!(kind.compacts(text), compacts);
}
