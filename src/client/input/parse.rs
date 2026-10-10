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
        b'\x1B' => {
            if buffer.len() == 1 {
                if input_available { Ok(None) } else { Ok(Some(Decoded::Event(Event::Key(KeyCode::Esc.into())))) }
            } else {
                match buffer[1] {
                    b'O' => {
                        if buffer.len() == 2 {
                            Ok(None)
                        } else {
                            match buffer[2] {
                                b'D' => Ok(Some(Decoded::Event(Event::Key(KeyCode::Left.into())))),
                                b'C' => Ok(Some(Decoded::Event(Event::Key(KeyCode::Right.into())))),
                                b'A' => Ok(Some(Decoded::Event(Event::Key(KeyCode::Up.into())))),
                                b'B' => Ok(Some(Decoded::Event(Event::Key(KeyCode::Down.into())))),
                                b'H' => Ok(Some(Decoded::Event(Event::Key(KeyCode::Home.into())))),
                                b'F' => Ok(Some(Decoded::Event(Event::Key(KeyCode::End.into())))),
                                val @ b'P'..=b'S' => {
                                    Ok(Some(Decoded::Event(Event::Key(KeyCode::F(1 + val - b'P').into()))))
                                }
                                _ => Err(could_not_parse_event_error()),
                            }
                        }
                    }
                    b'[' => parse_csi(buffer),
                    b'\x1B' => Ok(Some(Decoded::Event(Event::Key(KeyCode::Esc.into())))),
                    _ => parse_event(&buffer[1..], input_available).map(|event_option| {
                        event_option.map(|event| {
                            if let Decoded::Event(Event::Key(key_event)) = event {
                                let mut alt_key_event = key_event;
                                alt_key_event.modifiers |= KeyModifiers::ALT;
                                Decoded::Event(Event::Key(alt_key_event))
                            } else {
                                event
                            }
                        })
                    }),
                }
            }
        }
        b'\r' => Ok(Some(Decoded::Event(Event::Key(KeyCode::Enter.into())))),
        b'\n' if !crossterm::terminal::is_raw_mode_enabled().unwrap_or(true) => {
            Ok(Some(Decoded::Event(Event::Key(KeyCode::Enter.into()))))
        }
        b'\t' => Ok(Some(Decoded::Event(Event::Key(KeyCode::Tab.into())))),
        b'\x7F' => Ok(Some(Decoded::Event(Event::Key(KeyCode::Backspace.into())))),
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
        b'[' => {
            if buffer.len() == 3 {
                None
            } else {
                match buffer[3] {
                    val @ b'A'..=b'E' => Some(Event::Key(KeyCode::F(1 + val - b'A').into())),
                    _ => return Err(could_not_parse_event_error()),
                }
            }
        }
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
        b'0'..=b'9' => {
            if buffer.len() == 3 {
                None
            } else {
                let last_byte = buffer[buffer.len() - 1];
                if (64..=126).contains(&last_byte) {
                    if buffer.starts_with(b"\x1B[200~") {
                        return Ok(parse_csi_bracketed_paste(buffer));
                    }
                    match last_byte {
                        b'M' => return parse_csi_rxvt_mouse(buffer),
                        b'~' => return parse_csi_special_key_code(buffer),
                        b'u' => return parse_csi_u_encoded_key_code(buffer),
                        b'R' => return Ok(Some(Decoded::Reply)),
                        _ => return parse_csi_modifier_key_code(buffer),
                    }
                }
                None
            }
        }
        _ => return Err(could_not_parse_event_error()),
    };

    Ok(input_event.map(Decoded::Event))
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

fn parse_csi_modifier_key_code(buffer: &[u8]) -> io::Result<Option<Decoded>> {
    assert!(buffer.starts_with(b"\x1B["));
    let s = std::str::from_utf8(&buffer[2..buffer.len() - 1]).map_err(|_| could_not_parse_event_error())?;
    let mut split = s.split(';');

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

fn translate_functional_key_code(codepoint: u32) -> Option<(KeyCode, KeyEventState)> {
    if let Some(keycode) = match codepoint {
        57399 => Some(KeyCode::Char('0')),
        57400 => Some(KeyCode::Char('1')),
        57401 => Some(KeyCode::Char('2')),
        57402 => Some(KeyCode::Char('3')),
        57403 => Some(KeyCode::Char('4')),
        57404 => Some(KeyCode::Char('5')),
        57405 => Some(KeyCode::Char('6')),
        57406 => Some(KeyCode::Char('7')),
        57407 => Some(KeyCode::Char('8')),
        57408 => Some(KeyCode::Char('9')),
        57409 => Some(KeyCode::Char('.')),
        57410 => Some(KeyCode::Char('/')),
        57411 => Some(KeyCode::Char('*')),
        57412 => Some(KeyCode::Char('-')),
        57413 => Some(KeyCode::Char('+')),
        57414 => Some(KeyCode::Enter),
        57415 => Some(KeyCode::Char('=')),
        57416 => Some(KeyCode::Char(',')),
        57417 => Some(KeyCode::Left),
        57418 => Some(KeyCode::Right),
        57419 => Some(KeyCode::Up),
        57420 => Some(KeyCode::Down),
        57421 => Some(KeyCode::PageUp),
        57422 => Some(KeyCode::PageDown),
        57423 => Some(KeyCode::Home),
        57424 => Some(KeyCode::End),
        57425 => Some(KeyCode::Insert),
        57426 => Some(KeyCode::Delete),
        57427 => Some(KeyCode::KeypadBegin),
        _ => None,
    } {
        return Some((keycode, KeyEventState::KEYPAD));
    }

    if let Some(keycode) = match codepoint {
        57358 => Some(KeyCode::CapsLock),
        57359 => Some(KeyCode::ScrollLock),
        57360 => Some(KeyCode::NumLock),
        57361 => Some(KeyCode::PrintScreen),
        57362 => Some(KeyCode::Pause),
        57363 => Some(KeyCode::Menu),
        57376 => Some(KeyCode::F(13)),
        57377 => Some(KeyCode::F(14)),
        57378 => Some(KeyCode::F(15)),
        57379 => Some(KeyCode::F(16)),
        57380 => Some(KeyCode::F(17)),
        57381 => Some(KeyCode::F(18)),
        57382 => Some(KeyCode::F(19)),
        57383 => Some(KeyCode::F(20)),
        57384 => Some(KeyCode::F(21)),
        57385 => Some(KeyCode::F(22)),
        57386 => Some(KeyCode::F(23)),
        57387 => Some(KeyCode::F(24)),
        57388 => Some(KeyCode::F(25)),
        57389 => Some(KeyCode::F(26)),
        57390 => Some(KeyCode::F(27)),
        57391 => Some(KeyCode::F(28)),
        57392 => Some(KeyCode::F(29)),
        57393 => Some(KeyCode::F(30)),
        57394 => Some(KeyCode::F(31)),
        57395 => Some(KeyCode::F(32)),
        57396 => Some(KeyCode::F(33)),
        57397 => Some(KeyCode::F(34)),
        57398 => Some(KeyCode::F(35)),
        57428 => Some(KeyCode::Media(MediaKeyCode::Play)),
        57429 => Some(KeyCode::Media(MediaKeyCode::Pause)),
        57430 => Some(KeyCode::Media(MediaKeyCode::PlayPause)),
        57431 => Some(KeyCode::Media(MediaKeyCode::Reverse)),
        57432 => Some(KeyCode::Media(MediaKeyCode::Stop)),
        57433 => Some(KeyCode::Media(MediaKeyCode::FastForward)),
        57434 => Some(KeyCode::Media(MediaKeyCode::Rewind)),
        57435 => Some(KeyCode::Media(MediaKeyCode::TrackNext)),
        57436 => Some(KeyCode::Media(MediaKeyCode::TrackPrevious)),
        57437 => Some(KeyCode::Media(MediaKeyCode::Record)),
        57438 => Some(KeyCode::Media(MediaKeyCode::LowerVolume)),
        57439 => Some(KeyCode::Media(MediaKeyCode::RaiseVolume)),
        57440 => Some(KeyCode::Media(MediaKeyCode::MuteVolume)),
        57441 => Some(KeyCode::Modifier(ModifierKeyCode::LeftShift)),
        57442 => Some(KeyCode::Modifier(ModifierKeyCode::LeftControl)),
        57443 => Some(KeyCode::Modifier(ModifierKeyCode::LeftAlt)),
        57444 => Some(KeyCode::Modifier(ModifierKeyCode::LeftSuper)),
        57445 => Some(KeyCode::Modifier(ModifierKeyCode::LeftHyper)),
        57446 => Some(KeyCode::Modifier(ModifierKeyCode::LeftMeta)),
        57447 => Some(KeyCode::Modifier(ModifierKeyCode::RightShift)),
        57448 => Some(KeyCode::Modifier(ModifierKeyCode::RightControl)),
        57449 => Some(KeyCode::Modifier(ModifierKeyCode::RightAlt)),
        57450 => Some(KeyCode::Modifier(ModifierKeyCode::RightSuper)),
        57451 => Some(KeyCode::Modifier(ModifierKeyCode::RightHyper)),
        57452 => Some(KeyCode::Modifier(ModifierKeyCode::RightMeta)),
        57453 => Some(KeyCode::Modifier(ModifierKeyCode::IsoLevel3Shift)),
        57454 => Some(KeyCode::Modifier(ModifierKeyCode::IsoLevel5Shift)),
        _ => None,
    } {
        return Some((keycode, KeyEventState::empty()));
    }

    None
}

fn parse_csi_u_encoded_key_code(buffer: &[u8]) -> io::Result<Option<Decoded>> {
    assert!(buffer.starts_with(b"\x1B["));
    assert!(buffer.ends_with(b"u"));

    let s = std::str::from_utf8(&buffer[2..buffer.len() - 1]).map_err(|_| could_not_parse_event_error())?;
    let mut split = s.split(';');

    let mut codepoints = split.next().ok_or_else(could_not_parse_event_error)?.split(':');

    let codepoint = codepoints
        .next()
        .ok_or_else(could_not_parse_event_error)?
        .parse::<u32>()
        .map_err(|_| could_not_parse_event_error())?;

    let (mut modifiers, kind, state_from_modifiers) =
        if let Ok((modifier_mask, kind_code)) = modifier_and_kind_parsed(&mut split) {
            (parse_modifiers(modifier_mask), parse_key_event_kind(kind_code), parse_modifiers_to_state(modifier_mask))
        } else {
            (KeyModifiers::NONE, KeyEventKind::Press, KeyEventState::NONE)
        };

    let (mut keycode, state_from_keycode) = {
        if let Some((special_key_code, state)) = translate_functional_key_code(codepoint) {
            (special_key_code, state)
        } else if let Some(c) = char::from_u32(codepoint) {
            (
                match c {
                    '\x1B' => KeyCode::Esc,
                    '\r' => KeyCode::Enter,
                    '\n' if !crossterm::terminal::is_raw_mode_enabled().unwrap_or(true) => KeyCode::Enter,
                    '\t' => {
                        if modifiers.contains(KeyModifiers::SHIFT) {
                            KeyCode::BackTab
                        } else {
                            KeyCode::Tab
                        }
                    }
                    '\x7F' => KeyCode::Backspace,
                    _ => KeyCode::Char(c),
                },
                KeyEventState::empty(),
            )
        } else {
            return Err(could_not_parse_event_error());
        }
    };

    if let KeyCode::Modifier(modifier_keycode) = keycode {
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

fn parse_csi_special_key_code(buffer: &[u8]) -> io::Result<Option<Decoded>> {
    assert!(buffer.starts_with(b"\x1B["));
    assert!(buffer.ends_with(b"~"));

    let s = std::str::from_utf8(&buffer[2..buffer.len() - 1]).map_err(|_| could_not_parse_event_error())?;
    let mut split = s.split(';');

    let first = next_parsed::<u8>(&mut split)?;

    let (modifiers, kind, state) = if let Ok((modifier_mask, kind_code)) = modifier_and_kind_parsed(&mut split) {
        (parse_modifiers(modifier_mask), parse_key_event_kind(kind_code), parse_modifiers_to_state(modifier_mask))
    } else {
        (KeyModifiers::NONE, KeyEventKind::Press, KeyEventState::NONE)
    };

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
    assert!(buffer.starts_with(b"\x1B["));
    assert!(buffer.ends_with(b"M"));

    let s = std::str::from_utf8(&buffer[2..buffer.len() - 1]).map_err(|_| could_not_parse_event_error())?;
    let mut split = s.split(';');

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
        let required_bytes = match buffer[0] {
            (0x00..=0x7F) => 1,
            (0xC0..=0xDF) => 2,
            (0xE0..=0xEF) => 3,
            (0xF0..=0xF7) => 4,
            (0x80..=0xBF) | (0xF8..=0xFF) => return Err(could_not_parse_event_error()),
        };

        if required_bytes > 1 && buffer.len() > 1 {
            for byte in &buffer[1..] {
                if byte & !0b0011_1111 != 0b1000_0000 {
                    return Err(could_not_parse_event_error());
                }
            }
        }

        if buffer.len() < required_bytes { Ok(None) } else { Err(could_not_parse_event_error()) }
    }
}
