use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;

const WIDTH: usize = 40;
const ROWS: usize = 6;

fn press(dialog: &mut Dialog, code: KeyCode) -> Step {
    dialog.key(KeyEvent::new(code, KeyModifiers::NONE), WIDTH, ROWS)
}

fn dialog() -> Dialog {
    Dialog::new(7, "PRs", String::from("- prefers squash merges\n"))
}

mod typing {
    use super::*;

    #[test]
    fn opens_on_the_text_with_the_caret_at_its_start() {
        let dialog = dialog();

        assert_eq!(dialog.focus, Focus::Text);
        assert_eq!(dialog.text.text(), "- prefers squash merges\n");
        assert_eq!(dialog.text.position(WIDTH), (0, 0));
        assert_eq!(dialog.title(), "Memory › PRs");
    }

    #[test]
    fn enter_breaks_the_line_while_the_text_has_the_keys() {
        let mut dialog = dialog();

        assert_eq!(press(&mut dialog, KeyCode::Enter), Step::None);
        assert_eq!(dialog.text.text(), "\n- prefers squash merges\n");
        assert!(dialog.changed());
    }

    #[test]
    fn tab_moves_to_save_where_enter_saves() {
        let mut dialog = dialog();

        press(&mut dialog, KeyCode::Tab);
        assert_eq!(dialog.focus, Focus::Save);
        assert_eq!(press(&mut dialog, KeyCode::Char('x')), Step::None);
        assert_eq!(dialog.text.text(), "- prefers squash merges\n");
        assert_eq!(press(&mut dialog, KeyCode::Enter), Step::Save);
        press(&mut dialog, KeyCode::BackTab);
        assert_eq!(dialog.focus, Focus::Text);
    }

    #[test]
    fn esc_cancels_from_anywhere() {
        let mut dialog = dialog();
        press(&mut dialog, KeyCode::Tab);

        assert_eq!(press(&mut dialog, KeyCode::Esc), Step::Cancel);
    }

    #[test]
    fn a_paste_keeps_its_lines_and_takes_the_text_back() {
        let mut dialog = dialog();
        press(&mut dialog, KeyCode::Tab);

        dialog.paste("a\r\nb", WIDTH, ROWS);

        assert_eq!(dialog.focus, Focus::Text);
        assert_eq!(dialog.text.text(), "a\nb- prefers squash merges\n");
    }

    #[test]
    fn a_key_clears_the_error() {
        let mut dialog = dialog();
        dialog.error = Some(String::from("no"));

        press(&mut dialog, KeyCode::Right);

        assert_eq!(dialog.error, None);
    }
}

mod saving {
    use super::*;

    #[test]
    fn a_memory_the_bot_changed_meanwhile_says_so_and_the_next_save_replaces_it() {
        let mut dialog = dialog();

        dialog.changed_meanwhile(String::from("- the bot's own line\n"));

        assert_eq!(dialog.read, "- the bot's own line\n");
        assert_eq!(dialog.text.text(), "- prefers squash merges\n");
        assert_eq!(
            dialog.error.as_deref(),
            Some("PRs changed its memory meanwhile: save again to replace it with yours")
        );
        assert!(dialog.changed());
    }

    #[test]
    fn an_untouched_text_has_nothing_to_save() {
        assert!(!dialog().changed());
    }
}
