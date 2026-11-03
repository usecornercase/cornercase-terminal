#![cfg(windows)]

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::io;
use std::time::Duration;

use parking_lot::Mutex;
use rstest::rstest;
use windows_sys::Win32::Foundation::{ERROR_INVALID_HANDLE, FALSE, TRUE};
use windows_sys::Win32::System::Console::{
    COORD, DISABLE_NEWLINE_AUTO_RETURN, ENABLE_ECHO_INPUT, ENABLE_EXTENDED_FLAGS, ENABLE_INSERT_MODE,
    ENABLE_LINE_INPUT, ENABLE_MOUSE_INPUT, ENABLE_PROCESSED_INPUT, ENABLE_PROCESSED_OUTPUT, ENABLE_QUICK_EDIT_MODE,
    ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING, ENABLE_WINDOW_INPUT, ENABLE_WRAP_AT_EOL_OUTPUT,
    FOCUS_EVENT, FOCUS_EVENT_RECORD, INPUT_RECORD, INPUT_RECORD_0, KEY_EVENT, KEY_EVENT_RECORD, KEY_EVENT_RECORD_0,
    STD_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, WINDOW_BUFFER_SIZE_EVENT, WINDOW_BUFFER_SIZE_RECORD,
};
use windows_sys::Win32::System::Threading::INFINITE;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_MENU, VK_SHIFT};

use super::imp::{
    Handles, Modes, Pending, Records, characters, hide_echo, millis, raw, read_modes, read_with, wait_with, write_modes,
};

const CONHOST_INPUT: u32 = ENABLE_PROCESSED_INPUT
    | ENABLE_LINE_INPUT
    | ENABLE_ECHO_INPUT
    | ENABLE_WINDOW_INPUT
    | ENABLE_MOUSE_INPUT
    | ENABLE_INSERT_MODE
    | ENABLE_QUICK_EDIT_MODE
    | ENABLE_EXTENDED_FLAGS;
const CONHOST_OUTPUT: u32 = ENABLE_PROCESSED_OUTPUT | ENABLE_WRAP_AT_EOL_OUTPUT;

#[derive(Default)]
struct Console {
    modes: RefCell<HashMap<STD_HANDLE, u32>>,
    refuses: Option<STD_HANDLE>,
}

impl Console {
    fn with(input: u32, output: u32) -> Self {
        let console = Self::default();
        console.modes.borrow_mut().extend([(STD_INPUT_HANDLE, input), (STD_OUTPUT_HANDLE, output)]);
        console
    }

    fn now(&self) -> Modes {
        read_modes(self).expect("both handles are consoles")
    }
}

impl Handles for Console {
    fn mode(&self, which: STD_HANDLE) -> io::Result<u32> {
        self.modes.borrow().get(&which).copied().ok_or_else(not_a_console)
    }

    fn set_mode(&self, which: STD_HANDLE, mode: u32) -> io::Result<()> {
        if self.refuses == Some(which) {
            return Err(not_a_console());
        }
        self.modes.borrow_mut().insert(which, mode);
        Ok(())
    }
}

fn not_a_console() -> io::Error {
    io::Error::from_raw_os_error(ERROR_INVALID_HANDLE.cast_signed())
}

#[derive(Default)]
struct Typed {
    batches: RefCell<VecDeque<Vec<INPUT_RECORD>>>,
    waits: RefCell<Vec<u32>>,
}

impl Typed {
    fn of(batches: impl IntoIterator<Item = Vec<INPUT_RECORD>>) -> Self {
        Self { batches: RefCell::new(batches.into_iter().collect()), waits: RefCell::default() }
    }
}

impl Records for Typed {
    fn wait(&self, millis: u32) -> io::Result<bool> {
        self.waits.borrow_mut().push(millis);
        Ok(!self.batches.borrow().is_empty())
    }

    fn read(&self) -> io::Result<Vec<INPUT_RECORD>> {
        Ok(self.batches.borrow_mut().pop_front().unwrap_or_default())
    }
}

struct Gone;

impl Records for Gone {
    fn wait(&self, _millis: u32) -> io::Result<bool> {
        Err(not_a_console())
    }

    fn read(&self) -> io::Result<Vec<INPUT_RECORD>> {
        Err(not_a_console())
    }
}

fn kind(event: u32) -> u16 {
    u16::try_from(event).expect("a record type fits")
}

fn key(down: bool, code: u16, unit: u16, repeat: u16) -> INPUT_RECORD {
    let key = KEY_EVENT_RECORD {
        bKeyDown: if down { TRUE } else { FALSE },
        wRepeatCount: repeat,
        wVirtualKeyCode: code,
        wVirtualScanCode: 0,
        uChar: KEY_EVENT_RECORD_0 { UnicodeChar: unit },
        dwControlKeyState: 0,
    };
    INPUT_RECORD { EventType: kind(KEY_EVENT), Event: INPUT_RECORD_0 { KeyEvent: key } }
}

fn typed(text: &str) -> Vec<INPUT_RECORD> {
    text.encode_utf16().flat_map(|unit| [key(true, 0, unit, 1), key(false, 0, unit, 1)]).collect()
}

fn focus() -> INPUT_RECORD {
    let focus = FOCUS_EVENT_RECORD { bSetFocus: TRUE };
    INPUT_RECORD { EventType: kind(FOCUS_EVENT), Event: INPUT_RECORD_0 { FocusEvent: focus } }
}

fn resized() -> INPUT_RECORD {
    let size = WINDOW_BUFFER_SIZE_RECORD { dwSize: COORD { X: 100, Y: 30 } };
    INPUT_RECORD { EventType: kind(WINDOW_BUFFER_SIZE_EVENT), Event: INPUT_RECORD_0 { WindowBufferSizeEvent: size } }
}

fn read_all(console: &Typed, pending: &Mutex<Pending>) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut buf = [0; 64];
    while wait_with(console, pending, Duration::ZERO).expect("a fake wait") {
        let read = read_with(console, pending, &mut buf).expect("a fake read");
        bytes.extend_from_slice(&buf[..read]);
    }
    bytes
}

mod modes {
    use super::*;

    #[test]
    fn raw_reads_vt_sequences_without_line_echo_ctrl_c_or_quick_edit() {
        let raw = raw(Modes { input: CONHOST_INPUT, output: CONHOST_OUTPUT });
        let input = ENABLE_WINDOW_INPUT
            | ENABLE_MOUSE_INPUT
            | ENABLE_INSERT_MODE
            | ENABLE_EXTENDED_FLAGS
            | ENABLE_VIRTUAL_TERMINAL_INPUT;
        assert_eq!(raw.input, input);
        let output = CONHOST_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING | DISABLE_NEWLINE_AUTO_RETURN;
        assert_eq!(raw.output, output);
    }

    #[rstest]
    #[case::nothing_set(0, ENABLE_VIRTUAL_TERMINAL_INPUT | ENABLE_EXTENDED_FLAGS)]
    #[case::everything_set(
        u32::MAX,
        !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT | ENABLE_QUICK_EDIT_MODE)
    )]
    fn raw_changes_only_its_own_bits(#[case] kept: u32, #[case] input: u32) {
        let raw = raw(Modes { input: kept, output: kept });
        assert_eq!(raw.input, input);
        assert_eq!(raw.output, kept | ENABLE_VIRTUAL_TERMINAL_PROCESSING | DISABLE_NEWLINE_AUTO_RETURN);
    }

    #[test]
    fn restoring_gives_the_console_back_exactly_as_it_was_kept() {
        let without_ctrl_c = CONHOST_INPUT & !ENABLE_PROCESSED_INPUT;
        let console = Console::with(without_ctrl_c, CONHOST_OUTPUT);
        let kept = read_modes(&console).expect("both handles are consoles");
        write_modes(&console, raw(kept)).expect("raw mode");
        assert_eq!(console.now(), raw(kept));
        write_modes(&console, kept).expect("the kept modes");
        assert_eq!(console.now(), Modes { input: without_ctrl_c, output: CONHOST_OUTPUT });
    }

    #[test]
    fn a_handle_that_refuses_its_mode_still_lets_the_other_one_be_restored() {
        let console = Console { refuses: Some(STD_INPUT_HANDLE), ..Console::with(1, 2) };
        assert!(write_modes(&console, Modes { input: 3, output: 4 }).is_err());
        assert_eq!(console.now(), Modes { input: 1, output: 4 });
    }

    #[test]
    fn an_output_that_is_no_console_keeps_nothing() {
        let console = Console::default();
        console.modes.borrow_mut().insert(STD_INPUT_HANDLE, CONHOST_INPUT);
        assert!(read_modes(&console).is_err());
    }
}

mod hidden_input {
    use super::*;

    #[test]
    fn turns_the_echo_off_and_gives_back_the_mode_before() {
        let console = Console::with(CONHOST_INPUT, CONHOST_OUTPUT);
        assert_eq!(hide_echo(&console), Some(CONHOST_INPUT));
        assert_eq!(console.now().input, CONHOST_INPUT & !ENABLE_ECHO_INPUT);
        assert_ne!(console.now().input & ENABLE_LINE_INPUT, 0, "the token is still read as a line");
    }

    #[test]
    fn an_input_that_is_no_console_keeps_its_echo() {
        assert_eq!(hide_echo(&Console::default()), None);
    }
}

mod records {
    use super::*;

    #[rstest]
    #[case::key_down_and_up(typed("a"), "a")]
    #[case::a_key_without_a_character(vec![key(true, VK_SHIFT, 0, 1)], "")]
    #[case::a_held_key(vec![key(true, 0, u16::from(b'x'), 3)], "xxx")]
    #[case::a_character_composed_with_alt(vec![key(false, VK_MENU, 0xE9, 1)], "\u{e9}")]
    #[case::focus_and_size(vec![focus(), resized()], "")]
    fn only_typed_characters_count(#[case] records: Vec<INPUT_RECORD>, #[case] text: &str) {
        assert_eq!(String::from_utf16_lossy(&characters(&records)), text);
    }

    #[rstest]
    #[case::text("a\u{f1}\u{20ac}\u{1f600}")]
    #[case::a_sequence("\x1b[<0;6;5M")]
    #[case::ctrl_c("\x03")]
    #[case::ctrl_z("\x1a")]
    fn typed_text_comes_back_as_utf8(#[case] text: &str) {
        let console = Typed::of([typed(text)]);
        assert_eq!(read_all(&console, &Mutex::new(Pending::new())), text.as_bytes());
    }

    #[test]
    fn a_character_split_across_reads_waits_for_its_other_half() {
        let units: Vec<u16> = "\u{1f600}".encode_utf16().collect();
        let console = Typed::of(units.iter().map(|&unit| vec![key(true, 0, unit, 1)]));
        let pending = Mutex::new(Pending::new());
        assert!(wait_with(&console, &pending, Duration::from_secs(1)).expect("a fake wait"));
        assert_eq!(console.waits.borrow().len(), 2);
        assert_eq!(read_all(&console, &pending), "\u{1f600}".as_bytes());
    }

    #[test]
    fn a_lone_half_becomes_the_replacement_character() {
        let console = Typed::of([vec![key(true, 0, 0xD83D, 1), key(true, 0, u16::from(b'a'), 1)]]);
        assert_eq!(read_all(&console, &Mutex::new(Pending::new())), "\u{fffd}a".as_bytes());
    }

    #[test]
    fn more_than_the_buffer_comes_in_several_reads() {
        let console = Typed::of([typed("abcdef")]);
        let pending = Mutex::new(Pending::new());
        let mut buf = [0; 4];
        assert_eq!(read_with(&console, &pending, &mut buf).expect("a fake read"), 4);
        assert_eq!(&buf, b"abcd");
        assert_eq!(read_with(&console, &pending, &mut buf).expect("a fake read"), 2);
        assert_eq!(&buf[..2], b"ef");
        assert_eq!(console.waits.borrow().len(), 1, "the second read waits for nothing");
    }
}

mod waiting {
    use super::*;

    #[test]
    fn a_wake_on_focus_or_size_goes_back_to_waiting_until_the_deadline() {
        let console = Typed::of([vec![focus()], vec![resized()]]);
        let typed = wait_with(&console, &Mutex::new(Pending::new()), Duration::from_millis(50)).expect("a fake wait");
        assert!(!typed);
        assert_eq!(console.waits.borrow().len(), 3);
        assert!(console.batches.borrow().is_empty(), "the records that are no characters are read and dropped");
    }

    #[test]
    fn nothing_typed_times_out() {
        let console = Typed::default();
        assert!(!wait_with(&console, &Mutex::new(Pending::new()), Duration::ZERO).expect("a fake wait"));
        assert_eq!(*console.waits.borrow(), [0]);
    }

    #[rstest]
    #[case::none(Duration::ZERO, 0)]
    #[case::a_part_of_one(Duration::from_micros(1), 1)]
    #[case::a_part_past_one(Duration::from_micros(1500), 2)]
    #[case::too_long_to_count(Duration::from_secs(u64::MAX), INFINITE - 1)]
    fn a_wait_rounds_up_to_whole_milliseconds_and_never_waits_forever(#[case] left: Duration, #[case] wait: u32) {
        assert_eq!(millis(left), wait);
    }

    #[test]
    fn a_read_waits_without_a_deadline() {
        let console = Typed::of([vec![focus()], typed("z")]);
        let mut buf = [0; 8];
        assert_eq!(read_with(&console, &Mutex::new(Pending::new()), &mut buf).expect("a fake read"), 1);
        assert_eq!(buf[0], b'z');
        assert_eq!(*console.waits.borrow(), [INFINITE, INFINITE]);
    }

    #[test]
    fn a_console_that_went_away_is_an_error() {
        let error = wait_with(&Gone, &Mutex::new(Pending::new()), Duration::ZERO).expect_err("no console");
        assert_eq!(error.raw_os_error(), Some(ERROR_INVALID_HANDLE.cast_signed()));
        assert!(read_with(&Gone, &Mutex::new(Pending::new()), &mut [0; 8]).is_err());
    }
}
