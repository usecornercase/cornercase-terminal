#![cfg(unix)]

use crossterm::execute;

use super::*;

fn ansi(commands: impl FnOnce(&mut String) -> fmt::Result) -> Vec<u8> {
    let mut ansi = String::new();
    commands(&mut ansi).expect("text takes every command");
    ansi.into_bytes()
}

mod input_modes {
    use super::*;

    #[test]
    fn turning_them_on_writes_the_bytes_execute_wrote() {
        let mut executed = Vec::new();
        execute!(
            executed,
            EnableBracketedPaste,
            EnableMouseCapture,
            EnableFocusChange,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )
        .expect("commands into a buffer");
        assert_ne!(executed, b"");
        assert_eq!(ansi(input_modes_on), executed);
    }

    #[test]
    fn turning_them_off_writes_the_bytes_execute_wrote() {
        let mut executed = Vec::new();
        execute!(executed, PopKeyboardEnhancementFlags, DisableBracketedPaste, DisableMouseCapture, DisableFocusChange)
            .expect("commands into a buffer");
        assert_eq!(ansi(all_input_modes_off), executed);
    }

    #[test]
    fn releasing_the_screen_writes_the_bytes_execute_wrote() {
        let mut executed = Vec::new();
        execute!(executed, DisableBracketedPaste, DisableMouseCapture, DisableFocusChange)
            .expect("commands into a buffer");
        assert_eq!(ansi(input_modes_off), executed);
    }
}
