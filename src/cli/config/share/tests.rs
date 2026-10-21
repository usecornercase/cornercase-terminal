use super::*;
use crate::control::config::{change, note, section};

fn preview() -> SetupPreview {
    SetupPreview {
        sections: vec![
            section(
                "agents",
                vec![
                    change("default_agent", "Default agent", "claude", "codex"),
                    change("agent_args", "Codex arguments", "", "--yolo"),
                ],
                vec![note(NoteKind::Risk, "Codex runs without asking for permission")],
            ),
            section(
                "worktrees",
                vec![change("worktrees_dir", "Worktrees folder", "~/.cornercase/worktrees", "~/wt")],
                vec![note(NoteKind::Missing, "~/wt does not exist here")],
            ),
            section("interface", Vec::new(), Vec::new()),
        ],
        notes: vec![
            note(NoteKind::Connect, "connect linear in Settings › Issues to use it"),
            note(NoteKind::Secret, "the setup sets GITHUB_TOKEN: check no secret goes with it"),
        ],
    }
}

#[test]
fn the_preview_shows_each_change_and_note_by_section() {
    insta::assert_snapshot!(preview_text(&preview()));
}

#[test]
fn a_setup_is_printed_a_line_at_a_time_under_its_project() {
    let changes = vec![change("repositories.setup", "~/code/web-shop", "nothing", "npm ci\ncp .env.example .env")];
    let preview = SetupPreview { sections: vec![section("repositories", changes, Vec::new())], notes: Vec::new() };
    assert_eq!(
        preview_text(&preview),
        "repositories\n  ~/code/web-shop setup: nothing →\n      npm ci\n      cp .env.example .env\n"
    );
}

#[test]
fn a_preview_without_changes_imports_nothing() {
    let mut nothing = preview();
    assert!(changes_anything(&nothing));
    for section in &mut nothing.sections {
        section.changes.clear();
    }
    assert!(!changes_anything(&nothing));
}

#[test]
fn a_folder_is_refused_as_the_file_to_write() {
    let dir = crate::test_util::TempDir::new();
    let refused = written(dir.path(), "x").expect_err("a folder");
    assert!(refused.to_string().contains("is a folder"), "{refused}");
}
