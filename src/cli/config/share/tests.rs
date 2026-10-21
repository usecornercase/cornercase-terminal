use super::*;
use crate::control::config::{SectionPreview, SetupChange, SetupNote};

fn change(key: &str, label: &str, from: &str, to: &str) -> SetupChange {
    SetupChange { key: key.into(), label: label.into(), from: from.into(), to: to.into() }
}

fn note(kind: NoteKind, text: &str) -> SetupNote {
    SetupNote { kind, text: text.into() }
}

fn section(id: &str, changes: Vec<SetupChange>, notes: Vec<SetupNote>) -> SectionPreview {
    SectionPreview { id: id.into(), changes, notes }
}

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
