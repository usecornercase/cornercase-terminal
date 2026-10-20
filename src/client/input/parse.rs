use std::io;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MediaKeyCode, ModifierKeyCode, MouseButton,
    MouseEvent, MouseEventKind,
};

use super::Decoded;

fn could_not_parse_event_error() -> io::Error {
    io::Error::other("Could not parse an event.")
}

pub(super) fn parse_event(buffer: &[u8], input_available: bool) -> io::Result<Option<Decoded>> {
    if buffer.is_empty() {
        return Ok(None);
    }

    match buffer[0] {
        b'\x1B' => parse_escape(buffer, input_available),
        b'\r' => Ok(Some(key_event(KeyCode::Enter))),
        b'\n' if !crossterm::terminal::is_raw_mode_enabled().unwrap_or(true) => Ok(Some(key_event(KeyCode::Enter))),
        b'\t' => Ok(Some(key_event(KeyCode::Tab))),
        b'\x7F' => Ok(Some(key_event(KeyCode::Backspace))),
        c @ b'\x01'..=b'\x1A' => Ok(Some(Decoded::Event(Event::Key(KeyEvent::new(
            KeyCode::Char((c - 0x1 + b'a') as char),
            KeyModifiers::CONTROL,
        ))))),
        c @ b'\x1C'..=b'\x1F' => Ok(Some(Decoded::Event(Event::Key(KeyEvent::new(
            KeyCode::Char((c - 0x1C + b'4') as char),
            KeyModifiers::CONTROL,
        ))))),
        b'\0' => Ok(Some(Decoded::Event(Event::Key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::CONTROL))))),
        _ => parse_utf8_char(buffer).map(|maybe_char| {
            maybe_char.map(KeyCode::Char).map(char_code_to_event).map(Event::Key).map(Decoded::Event)
        }),
    }
}

fn key_event(code: KeyCode) -> Decoded {
    Decoded::Event(Event::Key(code.into()))
}

fn parse_escape(buffer: &[u8], input_available: bool) -> io::Result<Option<Decoded>> {
    if buffer.len() == 1 {
        return Ok(if input_available { None } else { Some(key_event(KeyCode::Esc)) });
    }
    match buffer[1] {
        b'O' => parse_ss3(buffer),
        b'[' => parse_csi(buffer),
        b'\x1B' => Ok(Some(key_event(KeyCode::Esc))),
        _ => parse_event(&buffer[1..], input_available).map(|event_option| event_option.map(with_alt)),
    }
}

fn with_alt(event: Decoded) -> Decoded {
    if let Decoded::Event(Event::Key(key_event)) = event {
        let mut alt_key_event = key_event;
        alt_key_event.modifiers |= KeyModifiers::ALT;
        Decoded::Event(Event::Key(alt_key_event))
    } else {
        event
    }
}

fn parse_ss3(buffer: &[u8]) -> io::Result<Option<Decoded>> {
    if buffer.len() == 2 {
        return Ok(None);
    }
    let code = match buffer[2] {
        b'D' => KeyCode::Left,
        b'C' => KeyCode::Right,
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'H' => KeyCode::Home,
        b'F' => KeyCode::End,
        val @ b'P'..=b'S' => KeyCode::F(1 + val - b'P'),
        _ => return Err(could_not_parse_event_error()),
    };
    Ok(Some(key_event(code)))
}

fn char_code_to_event(code: KeyCode) -> KeyEvent {
    let modifiers = match code {
        KeyCode::Char(c) if c.is_uppercase() => KeyModifiers::SHIFT,
        _ => KeyModifiers::empty(),
    };
    KeyEvent::new(code, modifiers)
}

fn parse_csi(buffer: &[u8]) -> io::Result<Option<Decoded>> {
    assert!(buffer.starts_with(b"\x1B["));

    if buffer.len() == 2 {
        return Ok(None);
    }

    let input_event = match buffer[2] {
        b'[' => parse_csi_linux_function_key(buffer)?,
        b'D' => Some(Event::Key(KeyCode::Left.into())),
        b'C' => Some(Event::Key(KeyCode::Right.into())),
        b'A' => Some(Event::Key(KeyCode::Up.into())),
        b'B' => Some(Event::Key(KeyCode::Down.into())),
        b'H' => Some(Event::Key(KeyCode::Home.into())),
        b'F' => Some(Event::Key(KeyCode::End.into())),
        b'Z' => Some(Event::Key(KeyEvent::new_with_kind(KeyCode::BackTab, KeyModifiers::SHIFT, KeyEventKind::Press))),
        b'M' => return parse_csi_normal_mouse(buffer),
        b'<' => return parse_csi_sgr_mouse(buffer),
        b'I' => Some(Event::FocusGained),
        b'O' => Some(Event::FocusLost),
        b';' => return parse_csi_modifier_key_code(buffer),
        b'P' => Some(Event::Key(KeyCode::F(1).into())),
        b'Q' => Some(Event::Key(KeyCode::F(2).into())),
        b'S' => Some(Event::Key(KeyCode::F(4).into())),
        b'?' | b'>' => {
            return Ok(buffer.last().filter(|&&last| (b'@'..=b'~').contains(&last)).map(|_| Decoded::Reply));
        }
        b'0'..=b'9' => return parse_csi_numeric(buffer),
        _ => return Err(could_not_parse_event_error()),
    };

    Ok(input_event.map(Decoded::Event))
}

fn parse_csi_linux_function_key(buffer: &[u8]) -> io::Result<Option<Event>> {
    if buffer.len() == 3 {
        return Ok(None);
    }
    match buffer[3] {
        val @ b'A'..=b'E' => Ok(Some(Event::Key(KeyCode::F(1 + val - b'A').into()))),
        _ => Err(could_not_parse_event_error()),
    }
}

fn parse_csi_numeric(buffer: &[u8]) -> io::Result<Option<Decoded>> {
    if buffer.len() == 3 {
        return Ok(None);
    }
    let last_byte = buffer[buffer.len() - 1];
    if !(64..=126).contains(&last_byte) {
        return Ok(None);
    }
    if buffer.starts_with(b"\x1B[200~") {
        return Ok(parse_csi_bracketed_paste(buffer));
    }
    match last_byte {
        b'M' => parse_csi_rxvt_mouse(buffer),
        b'~' => parse_csi_special_key_code(buffer),
        b'u' => parse_csi_u_encoded_key_code(buffer),
        b'R' => Ok(Some(Decoded::Reply)),
        _ => parse_csi_modifier_key_code(buffer),
    }
}

fn csi_fields(buffer: &[u8]) -> io::Result<std::str::Split<'_, char>> {
    assert!(buffer.starts_with(b"\x1B["));
    let s = std::str::from_utf8(&buffer[2..buffer.len() - 1]).map_err(|_| could_not_parse_event_error())?;
    Ok(s.split(';'))
}

fn next_parsed<T>(iter: &mut dyn Iterator<Item = &str>) -> io::Result<T>
where
    T: std::str::FromStr,
{
    iter.next().ok_or_else(could_not_parse_event_error)?.parse::<T>().map_err(|_| could_not_parse_event_error())
}

fn coordinate(iter: &mut dyn Iterator<Item = &str>) -> io::Result<u16> {
    next_parsed::<u16>(iter)?.checked_sub(1).ok_or_else(could_not_parse_event_error)
}

fn modifier_and_kind_parsed(iter: &mut dyn Iterator<Item = &str>) -> io::Result<(u8, u8)> {
    let mut sub_split = iter.next().ok_or_else(could_not_parse_event_error)?.split(':');

    let modifier_mask = next_parsed::<u8>(&mut sub_split)?;

    if let Ok(kind_code) = next_parsed::<u8>(&mut sub_split) {
        Ok((modifier_mask, kind_code))
    } else {
        Ok((modifier_mask, 1))
    }
}

fn parse_modifiers(mask: u8) -> KeyModifiers {
    let modifier_mask = mask.saturating_sub(1);
    let mut modifiers = KeyModifiers::empty();
    if modifier_mask & 1 != 0 {
        modifiers |= KeyModifiers::SHIFT;
    }
    if modifier_mask & 2 != 0 {
        modifiers |= KeyModifiers::ALT;
    }
    if modifier_mask & 4 != 0 {
        modifiers |= KeyModifiers::CONTROL;
    }
    if modifier_mask & 8 != 0 {
        modifiers |= KeyModifiers::SUPER;
    }
    if modifier_mask & 16 != 0 {
        modifiers |= KeyModifiers::HYPER;
    }
    if modifier_mask & 32 != 0 {
        modifiers |= KeyModifiers::META;
    }
    modifiers
}

fn parse_modifiers_to_state(mask: u8) -> KeyEventState {
    let modifier_mask = mask.saturating_sub(1);
    let mut state = KeyEventState::empty();
    if modifier_mask & 64 != 0 {
        state |= KeyEventState::CAPS_LOCK;
    }
    if modifier_mask & 128 != 0 {
        state |= KeyEventState::NUM_LOCK;
    }
    state
}

fn parse_key_event_kind(kind: u8) -> KeyEventKind {
    match kind {
        2 => KeyEventKind::Repeat,
        3 => KeyEventKind::Release,
        _ => KeyEventKind::Press,
    }
}

fn modifiers_kind_state(split: &mut dyn Iterator<Item = &str>) -> (KeyModifiers, KeyEventKind, KeyEventState) {
    if let Ok((modifier_mask, kind_code)) = modifier_and_kind_parsed(split) {
        (parse_modifiers(modifier_mask), parse_key_event_kind(kind_code), parse_modifiers_to_state(modifier_mask))
    } else {
        (KeyModifiers::NONE, KeyEventKind::Press, KeyEventState::NONE)
    }
}

fn parse_csi_modifier_key_code(buffer: &[u8]) -> io::Result<Option<Decoded>> {
    let mut split = csi_fields(buffer)?;

    split.next();

    let (modifiers, kind) = if let Ok((modifier_mask, kind_code)) = modifier_and_kind_parsed(&mut split) {
        (parse_modifiers(modifier_mask), parse_key_event_kind(kind_code))
    } else if buffer.len() > 3 {
        (
            parse_modifiers(
                u8::try_from(
                    char::from(buffer[buffer.len() - 2]).to_digit(10).ok_or_else(could_not_parse_event_error)?,
                )
                .map_err(|_| could_not_parse_event_error())?,
            ),
            KeyEventKind::Press,
        )
    } else {
        (KeyModifiers::NONE, KeyEventKind::Press)
    };
    let key = buffer[buffer.len() - 1];

    let keycode = match key {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        b'F' => KeyCode::End,
        b'H' => KeyCode::Home,
        b'P' => KeyCode::F(1),
        b'Q' => KeyCode::F(2),
        b'R' => KeyCode::F(3),
        b'S' => KeyCode::F(4),
        _ => return Err(could_not_parse_event_error()),
    };

    let input_event = Event::Key(KeyEvent::new_with_kind(keycode, modifiers, kind));

    Ok(Some(Decoded::Event(input_event)))
}

const KEYPAD: [(u32, KeyCode); 29] = [
    (57399, KeyCode::Char('0')),
    (57400, KeyCode::Char('1')),
    (57401, KeyCode::Char('2')),
    (57402, KeyCode::Char('3')),
    (57403, KeyCode::Char('4')),
    (57404, KeyCode::Char('5')),
    (57405, KeyCode::Char('6')),
    (57406, KeyCode::Char('7')),
    (57407, KeyCode::Char('8')),
    (57408, KeyCode::Char('9')),
    (57409, KeyCode::Char('.')),
    (57410, KeyCode::Char('/')),
    (57411, KeyCode::Char('*')),
    (57412, KeyCode::Char('-')),
    (57413, KeyCode::Char('+')),
    (57414, KeyCode::Enter),
    (57415, KeyCode::Char('=')),
    (57416, KeyCode::Char(',')),
    (57417, KeyCode::Left),
    (57418, KeyCode::Right),
    (57419, KeyCode::Up),
    (57420, KeyCode::Down),
    (57421, KeyCode::PageUp),
    (57422, KeyCode::PageDown),
    (57423, KeyCode::Home),
    (57424, KeyCode::End),
    (57425, KeyCode::Insert),
    (57426, KeyCode::Delete),
    (57427, KeyCode::KeypadBegin),
];

const FUNCTIONAL: [(u32, KeyCode); 56] = [
    (57358, KeyCode::CapsLock),
    (57359, KeyCode::ScrollLock),
    (57360, KeyCode::NumLock),
    (57361, KeyCode::PrintScreen),
    (57362, KeyCode::Pause),
    (57363, KeyCode::Menu),
    (57376, KeyCode::F(13)),
    (57377, KeyCode::F(14)),
    (57378, KeyCode::F(15)),
    (57379, KeyCode::F(16)),
    (57380, KeyCode::F(17)),
    (57381, KeyCode::F(18)),
    (57382, KeyCode::F(19)),
    (57383, KeyCode::F(20)),
    (57384, KeyCode::F(21)),
    (57385, KeyCode::F(22)),
    (57386, KeyCode::F(23)),
    (57387, KeyCode::F(24)),
    (57388, KeyCode::F(25)),
    (57389, KeyCode::F(26)),
    (57390, KeyCode::F(27)),
    (57391, KeyCode::F(28)),
    (57392, KeyCode::F(29)),
    (57393, KeyCode::F(30)),
    (57394, KeyCode::F(31)),
    (57395, KeyCode::F(32)),
    (57396, KeyCode::F(33)),
    (57397, KeyCode::F(34)),
    (57398, KeyCode::F(35)),
    (57428, KeyCode::Media(MediaKeyCode::Play)),
    (57429, KeyCode::Media(MediaKeyCode::Pause)),
    (57430, KeyCode::Media(MediaKeyCode::PlayPause)),
    (57431, KeyCode::Media(MediaKeyCode::Reverse)),
    (57432, KeyCode::Media(MediaKeyCode::Stop)),
    (57433, KeyCode::Media(MediaKeyCode::FastForward)),
    (57434, KeyCode::Media(MediaKeyCode::Rewind)),
    (57435, KeyCode::Media(MediaKeyCode::TrackNext)),
    (57436, KeyCode::Media(MediaKeyCode::TrackPrevious)),
    (57437, KeyCode::Media(MediaKeyCode::Record)),
    (57438, KeyCode::Media(MediaKeyCode::LowerVolume)),
    (57439, KeyCode::Media(MediaKeyCode::RaiseVolume)),
    (57440, KeyCode::Media(MediaKeyCode::MuteVolume)),
    (57441, KeyCode::Modifier(ModifierKeyCode::LeftShift)),
    (57442, KeyCode::Modifier(ModifierKeyCode::LeftControl)),
    (57443, KeyCode::Modifier(ModifierKeyCode::LeftAlt)),
    (57444, KeyCode::Modifier(ModifierKeyCode::LeftSuper)),
    (57445, KeyCode::Modifier(ModifierKeyCode::LeftHyper)),
    (57446, KeyCode::Modifier(ModifierKeyCode::LeftMeta)),
    (57447, KeyCode::Modifier(ModifierKeyCode::RightShift)),
    (57448, KeyCode::Modifier(ModifierKeyCode::RightControl)),
    (57449, KeyCode::Modifier(ModifierKeyCode::RightAlt)),
    (57450, KeyCode::Modifier(ModifierKeyCode::RightSuper)),
    (57451, KeyCode::Modifier(ModifierKeyCode::RightHyper)),
    (57452, KeyCode::Modifier(ModifierKeyCode::RightMeta)),
    (57453, KeyCode::Modifier(ModifierKeyCode::IsoLevel3Shift)),
    (57454, KeyCode::Modifier(ModifierKeyCode::IsoLevel5Shift)),
];

fn lookup(table: &[(u32, KeyCode)], codepoint: u32) -> Option<KeyCode> {
    table.iter().find(|(code, _)| *code == codepoint).map(|&(_, keycode)| keycode)
}

fn translate_functional_key_code(codepoint: u32) -> Option<(KeyCode, KeyEventState)> {
    lookup(&KEYPAD, codepoint)
        .map(|keycode| (keycode, KeyEventState::KEYPAD))
        .or_else(|| lookup(&FUNCTIONAL, codepoint).map(|keycode| (keycode, KeyEventState::empty())))
}

fn parse_csi_u_encoded_key_code(buffer: &[u8]) -> io::Result<Option<Decoded>> {
    assert!(buffer.ends_with(b"u"));

    let mut split = csi_fields(buffer)?;

    let mut codepoints = split.next().ok_or_else(could_not_parse_event_error)?.split(':');

    let codepoint = codepoints
        .next()
        .ok_or_else(could_not_parse_event_error)?
        .parse::<u32>()
        .map_err(|_| could_not_parse_event_error())?;

    let (mut modifiers, kind, state_from_modifiers) = modifiers_kind_state(&mut split);

    let (mut keycode, state_from_keycode) = decode_codepoint(codepoint, modifiers)?;

    if let KeyCode::Modifier(modifier_keycode) = keycode {
        apply_modifier_key(&mut modifiers, modifier_keycode);
    }

    if modifiers.contains(KeyModifiers::SHIFT)
        && let Some(shifted_c) =
            codepoints.next().and_then(|codepoint| codepoint.parse::<u32>().ok()).and_then(char::from_u32)
    {
        keycode = KeyCode::Char(shifted_c);
        modifiers.set(KeyModifiers::SHIFT, false);
    }

    let input_event = Event::Key(KeyEvent::new_with_kind_and_state(
        keycode,
        modifiers,
        kind,
        state_from_keycode | state_from_modifiers,
    ));

    Ok(Some(Decoded::Event(input_event)))
}

fn decode_codepoint(codepoint: u32, modifiers: KeyModifiers) -> io::Result<(KeyCode, KeyEventState)> {
    if let Some(special) = translate_functional_key_code(codepoint) {
        return Ok(special);
    }
    let c = char::from_u32(codepoint).ok_or_else(could_not_parse_event_error)?;
    let keycode = match c {
        '\x1B' => KeyCode::Esc,
        '\r' => KeyCode::Enter,
        '\n' if !crossterm::terminal::is_raw_mode_enabled().unwrap_or(true) => KeyCode::Enter,
        '\t' if modifiers.contains(KeyModifiers::SHIFT) => KeyCode::BackTab,
        '\t' => KeyCode::Tab,
        '\x7F' => KeyCode::Backspace,
        _ => KeyCode::Char(c),
    };
    Ok((keycode, KeyEventState::empty()))
}

fn apply_modifier_key(modifiers: &mut KeyModifiers, modifier_keycode: ModifierKeyCode) {
    match modifier_keycode {
        ModifierKeyCode::LeftAlt | ModifierKeyCode::RightAlt => modifiers.set(KeyModifiers::ALT, true),
        ModifierKeyCode::LeftControl | ModifierKeyCode::RightControl => modifiers.set(KeyModifiers::CONTROL, true),
        ModifierKeyCode::LeftShift | ModifierKeyCode::RightShift => modifiers.set(KeyModifiers::SHIFT, true),
        ModifierKeyCode::LeftSuper | ModifierKeyCode::RightSuper => modifiers.set(KeyModifiers::SUPER, true),
        ModifierKeyCode::LeftHyper | ModifierKeyCode::RightHyper => modifiers.set(KeyModifiers::HYPER, true),
        ModifierKeyCode::LeftMeta | ModifierKeyCode::RightMeta => modifiers.set(KeyModifiers::META, true),
        _ => {}
    }
}

fn parse_csi_special_key_code(buffer: &[u8]) -> io::Result<Option<Decoded>> {
    assert!(buffer.ends_with(b"~"));

    let mut split = csi_fields(buffer)?;

    let first = next_parsed::<u8>(&mut split)?;

    let (modifiers, kind, state) = modifiers_kind_state(&mut split);

    let keycode = match first {
        1 | 7 => KeyCode::Home,
        2 => KeyCode::Insert,
        3 => KeyCode::Delete,
        4 | 8 => KeyCode::End,
        5 => KeyCode::PageUp,
        6 => KeyCode::PageDown,
        v @ 11..=15 => KeyCode::F(v - 10),
        v @ 17..=21 => KeyCode::F(v - 11),
        v @ 23..=26 => KeyCode::F(v - 12),
        v @ 28..=29 => KeyCode::F(v - 15),
        v @ 31..=34 => KeyCode::F(v - 17),
        _ => return Err(could_not_parse_event_error()),
    };

    let input_event = Event::Key(KeyEvent::new_with_kind_and_state(keycode, modifiers, kind, state));

    Ok(Some(Decoded::Event(input_event)))
}
fn parse_csi_rxvt_mouse(buffer: &[u8]) -> io::Result<Option<Decoded>> {
    assert!(buffer.ends_with(b"M"));

    let mut split = csi_fields(buffer)?;

    let cb = next_parsed::<u8>(&mut split)?.checked_sub(32).ok_or_else(could_not_parse_event_error)?;
    let (kind, modifiers) = parse_cb(cb)?;

    let cx = coordinate(&mut split)?;
    let cy = coordinate(&mut split)?;

    Ok(Some(Decoded::Event(Event::Mouse(MouseEvent { kind, column: cx, row: cy, modifiers }))))
}

fn parse_csi_normal_mouse(buffer: &[u8]) -> io::Result<Option<Decoded>> {
    assert!(buffer.starts_with(b"\x1B[M"));

    if buffer.len() < 6 {
        return Ok(None);
    }

    let cb = buffer[3].checked_sub(32).ok_or_else(could_not_parse_event_error)?;
    let (kind, modifiers) = parse_cb(cb)?;

    let cx = u16::from(buffer[4]).checked_sub(33).ok_or_else(could_not_parse_event_error)?;
    let cy = u16::from(buffer[5]).checked_sub(33).ok_or_else(could_not_parse_event_error)?;

    Ok(Some(Decoded::Event(Event::Mouse(MouseEvent { kind, column: cx, row: cy, modifiers }))))
}

fn parse_csi_sgr_mouse(buffer: &[u8]) -> io::Result<Option<Decoded>> {
    assert!(buffer.starts_with(b"\x1B[<"));

    if !buffer.ends_with(b"m") && !buffer.ends_with(b"M") {
        return Ok(None);
    }

    let s = std::str::from_utf8(&buffer[3..buffer.len() - 1]).map_err(|_| could_not_parse_event_error())?;
    let mut split = s.split(';');

    let cb = next_parsed::<u8>(&mut split)?;
    let (kind, modifiers) = parse_cb(cb)?;

    let cx = coordinate(&mut split)?;
    let cy = coordinate(&mut split)?;

    let kind = if buffer.last() == Some(&b'm') {
        match kind {
            MouseEventKind::Down(button) => MouseEventKind::Up(button),
            other => other,
        }
    } else {
        kind
    };

    Ok(Some(Decoded::Event(Event::Mouse(MouseEvent { kind, column: cx, row: cy, modifiers }))))
}

fn parse_cb(cb: u8) -> io::Result<(MouseEventKind, KeyModifiers)> {
    let button_number = (cb & 0b0000_0011) | ((cb & 0b1100_0000) >> 4);
    let dragging = cb & 0b0010_0000 == 0b0010_0000;

    let kind = match (button_number, dragging) {
        (0, false) => MouseEventKind::Down(MouseButton::Left),
        (1, false) => MouseEventKind::Down(MouseButton::Middle),
        (2, false) => MouseEventKind::Down(MouseButton::Right),
        (0, true) => MouseEventKind::Drag(MouseButton::Left),
        (1, true) => MouseEventKind::Drag(MouseButton::Middle),
        (2, true) => MouseEventKind::Drag(MouseButton::Right),
        (3, false) => MouseEventKind::Up(MouseButton::Left),
        (3..=5, true) => MouseEventKind::Moved,
        (4, false) => MouseEventKind::ScrollUp,
        (5, false) => MouseEventKind::ScrollDown,
        (6, false) => MouseEventKind::ScrollLeft,
        (7, false) => MouseEventKind::ScrollRight,
        _ => return Err(could_not_parse_event_error()),
    };

    let mut modifiers = KeyModifiers::empty();

    if cb & 0b0000_0100 == 0b0000_0100 {
        modifiers |= KeyModifiers::SHIFT;
    }
    if cb & 0b0000_1000 == 0b0000_1000 {
        modifiers |= KeyModifiers::ALT;
    }
    if cb & 0b0001_0000 == 0b0001_0000 {
        modifiers |= KeyModifiers::CONTROL;
    }

    Ok((kind, modifiers))
}

fn parse_csi_bracketed_paste(buffer: &[u8]) -> Option<Decoded> {
    if buffer.ends_with(b"\x1b[201~") {
        Some(Decoded::Event(Event::Paste(String::from_utf8_lossy(&buffer[6..buffer.len() - 6]).into_owned())))
    } else {
        None
    }
}

fn parse_utf8_char(buffer: &[u8]) -> io::Result<Option<char>> {
    if let Ok(s) = std::str::from_utf8(buffer) {
        let ch = s.chars().next().ok_or_else(could_not_parse_event_error)?;

        Ok(Some(ch))
    } else {
        incomplete_utf8_char(buffer)
    }
}

fn incomplete_utf8_char(buffer: &[u8]) -> io::Result<Option<char>> {
    let required_bytes = match buffer[0] {
        (0x00..=0x7F) => 1,
        (0xC0..=0xDF) => 2,
        (0xE0..=0xEF) => 3,
        (0xF0..=0xF7) => 4,
        (0x80..=0xBF) | (0xF8..=0xFF) => return Err(could_not_parse_event_error()),
    };

    if required_bytes > 1 && buffer.len() > 1 && buffer[1..].iter().any(|byte| byte & !0b0011_1111 != 0b1000_0000) {
        return Err(could_not_parse_event_error());
    }

    if buffer.len() < required_bytes { Ok(None) } else { Err(could_not_parse_event_error()) }
}
