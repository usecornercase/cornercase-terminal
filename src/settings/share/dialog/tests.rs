use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::control::config::{NoteKind, SetupExported, SetupPreview, change, note, section};

const ROOM: Room = Room { width: 40, height: 4 };

fn key(dialog: &mut Dialog, code: KeyCode) -> Action {
    dialog.key(KeyEvent::new(code, KeyModifiers::NONE), ROOM)
}

fn shared() -> Shared {
    Shared { unknown: vec!["future".into()], ..Shared::default() }
}

fn preview() -> SetupPreview {
    SetupPreview {
        sections: vec![
            section(
                "agents",
                vec![change("prompt", "prompt", "{url}", "Fix {url}"), change("submit", "submit", "", "enter")],
                vec![note(NoteKind::Risk, "starts codex in yolo mode")],
            ),
            section("issues", Vec::new(), Vec::new()),
            section("interface", vec![change("sidebar", "sidebar", "projects on top", "tree")], Vec::new()),
        ],
        notes: vec![note(NoteKind::Unknown, "this cornercase does not know future")],
    }
}

pub(crate) fn previewed() -> Dialog {
    let mut dialog = Dialog::new("version = 1");
    dialog.previewed(Ok((shared(), preview())));
    dialog
}

fn error(dialog: &Dialog) -> Option<&str> {
    match &dialog.page {
        Page::Paste { error, .. } | Page::Preview { error, .. } => error.as_deref(),
    }
}

fn pasted(dialog: &Dialog) -> String {
    match &dialog.page {
        Page::Paste { editor, .. } => editor.text(),
        Page::Preview { .. } => panic!("not on the paste page"),
    }
}

fn ticked(dialog: &Dialog) -> Vec<bool> {
    match &dialog.page {
        Page::Preview { ticked, .. } => ticked.clone(),
        Page::Paste { .. } => panic!("not on the import page"),
    }
}

mod the_paste_page {
    use super::*;

    #[test]
    fn shows_its_buttons_and_title() {
        let dialog = Dialog::new("");
        assert_eq!((dialog.title(), dialog.buttons()), ("Settings › Share", vec![BACK, COPY, PREVIEW]));
    }

    #[test]
    fn refuses_a_paste_past_the_limit() {
        let mut dialog = Dialog::new("");
        dialog.paste(&"x".repeat(MAX_BYTES + 1), ROOM);
        assert_eq!((error(&dialog), pasted(&dialog)), (Some(too_large().as_str()), String::new()));
    }

    #[test]
    fn keeps_a_paste_within_the_limit() {
        let mut dialog = Dialog::new("");
        dialog.paste("version = 1\r\n[interface]", ROOM);
        assert_eq!((error(&dialog), pasted(&dialog)), (None, "version = 1\n[interface]".to_string()));
    }

    #[test]
    fn asks_for_a_setup_before_a_preview() {
        let mut dialog = Dialog::new("  \n");
        assert_eq!(dialog.press(PREVIEW), Action::None);
        assert_eq!(error(&dialog), Some("paste a setup first"));
    }

    #[test]
    fn hands_the_text_to_preview() {
        let mut dialog = Dialog::new("version = 1");
        assert_eq!(dialog.press(PREVIEW), Action::Preview("version = 1".into()));
    }

    #[test]
    fn stays_with_the_reason_of_a_refused_text() {
        let mut dialog = Dialog::new("version = 9");
        dialog.previewed(Err("made by a newer cornercase".into()));
        assert_eq!((dialog.title(), error(&dialog)), ("Settings › Share", Some("made by a newer cornercase")));
    }

    #[test]
    fn goes_back_to_the_settings_on_esc_and_back() {
        let mut dialog = Dialog::new("");
        assert_eq!((key(&mut dialog, KeyCode::Esc), dialog.press(BACK)), (Action::Back, Action::Back));
    }

    #[test]
    fn copies_on_its_button() {
        assert_eq!(Dialog::new("").press(COPY), Action::Copy);
    }

    #[test]
    fn breaks_the_line_on_enter() {
        let mut dialog = Dialog::new("a");
        key(&mut dialog, KeyCode::Enter);
        assert_eq!(pasted(&dialog), "\na");
    }
}

mod the_import_page {
    use super::*;

    #[test]
    fn ticks_the_sections_with_changes() {
        let dialog = previewed();
        assert_eq!(
            (dialog.title(), dialog.buttons(), ticked(&dialog)),
            ("Settings › Import", vec![BACK, IMPORT], vec![true, false, true])
        );
    }

    #[test]
    fn toggles_only_sections_with_changes() {
        let mut dialog = previewed();
        dialog.toggle(0);
        dialog.toggle(1);
        assert_eq!(ticked(&dialog), [false, false, true]);
    }

    #[test]
    fn imports_only_the_ticked_sections() {
        let mut dialog = previewed();
        dialog.toggle(0);
        assert_eq!(dialog.press(IMPORT), Action::Import(Box::new(shared()), vec![Section::Interface]));
    }

    #[test]
    fn imports_on_enter() {
        let mut dialog = previewed();
        assert_eq!(
            key(&mut dialog, KeyCode::Enter),
            Action::Import(Box::new(shared()), vec![Section::Agents, Section::Interface])
        );
    }

    #[test]
    fn dims_import_with_nothing_ticked() {
        let mut dialog = previewed();
        assert_eq!(dialog.dimmed(), None);
        dialog.toggle(0);
        dialog.toggle(2);
        assert_eq!((dialog.press(IMPORT), dialog.dimmed()), (Action::None, Some(IMPORT)));
    }

    #[test]
    fn dims_import_and_says_so_when_nothing_changes() {
        let mut dialog = Dialog::new("version = 1");
        let nothing = SetupPreview { sections: vec![section("agents", Vec::new(), Vec::new())], notes: Vec::new() };
        dialog.previewed(Ok((Shared::default(), nothing)));
        assert_eq!((dialog.dimmed(), dialog.intro()), (Some(IMPORT), NOTHING));
    }

    #[test]
    fn shows_each_section_with_its_changes_and_notes() {
        let mut dialog = previewed();
        dialog.toggle(0);
        let lines: Vec<(String, Tone, Option<usize>)> =
            dialog.lines().into_iter().map(|line| (line.text, line.tone, line.section)).collect();
        assert_eq!(
            lines,
            [
                ("[ ] agents · 2 changes".to_string(), Tone::Section, Some(0)),
                ("      prompt: {url} → Fix {url}".to_string(), Tone::Plain, None),
                ("      submit: - → enter".to_string(), Tone::Plain, None),
                ("      starts codex in yolo mode".to_string(), Tone::Red, None),
                (String::new(), Tone::Plain, None),
                ("    issues · no changes".to_string(), Tone::Muted, Some(1)),
                (String::new(), Tone::Plain, None),
                ("[x] interface · 1 change".to_string(), Tone::Section, Some(2)),
                ("      sidebar: projects on top → tree".to_string(), Tone::Plain, None),
                (String::new(), Tone::Plain, None),
                ("this cornercase does not know future".to_string(), Tone::Muted, None),
            ]
        );
    }

    #[test]
    fn goes_back_to_the_paste_page_with_the_text_on_esc() {
        let mut dialog = previewed();
        assert_eq!(key(&mut dialog, KeyCode::Esc), Action::None);
        assert_eq!((dialog.title(), pasted(&dialog)), ("Settings › Share", "version = 1".to_string()));
    }

    #[test]
    fn goes_back_to_the_paste_page_on_back() {
        let mut dialog = previewed();
        dialog.press(BACK);
        assert_eq!(pasted(&dialog), "version = 1");
    }

    #[test]
    fn shows_why_an_import_was_refused() {
        let mut dialog = previewed();
        dialog.refused("config.json cannot be read".into());
        assert_eq!((dialog.title(), error(&dialog)), ("Settings › Import", Some("config.json cannot be read")));
    }

    #[test]
    fn scrolls_within_its_lines() {
        let mut dialog = previewed();
        dialog.scroll(100, ROOM);
        let Page::Preview { scroll, .. } = &dialog.page else { panic!("not on the import page") };
        assert_eq!(*scroll, dialog.lines().len() - ROOM.height);
    }
}

mod the_copy_toast {
    use super::*;

    fn exported(warnings: &[&str]) -> SetupExported {
        SetupExported {
            text: String::new(),
            settings: 12,
            repositories: 1,
            left_out: vec!["jira_email".into()],
            warnings: warnings.iter().map(ToString::to_string).collect(),
        }
    }

    #[test]
    fn says_what_was_copied_and_left_out() {
        assert_eq!(
            copied(&exported(&[])),
            "setup copied: 12 settings, 1 repositories · left out: tokens, personal settings"
        );
    }

    #[test]
    fn names_the_first_warning() {
        assert_eq!(
            copied(&exported(&["setup of a sets NPM_TOKEN: check no secret goes with it", "second"])),
            "setup copied: 12 settings, 1 repositories · left out: tokens, personal settings · check: setup of a sets \
             NPM_TOKEN: check no secret goes with it"
        );
    }

    #[test]
    fn puts_every_warning_on_the_hint_line() {
        let mut dialog = Dialog::new("");
        dialog.warnings = vec!["first".into(), "second".into()];
        assert_eq!(dialog.note(), Some("first · second".into()));
        key(&mut dialog, KeyCode::Char('x'));
        assert_eq!(dialog.note(), None);
    }
}
