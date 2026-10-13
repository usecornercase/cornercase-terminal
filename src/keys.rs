use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use libghostty_vt::key::{self, Action, Encoder, KittyKeyFlags, Mods};

const MODIFIERS: [(&str, KeyModifiers); 3] =
    [("ctrl+", KeyModifiers::CONTROL), ("alt+", KeyModifiers::ALT), ("shift+", KeyModifiers::SHIFT)];
const NAMED: [(&str, KeyCode); 20] = [
    ("enter", KeyCode::Enter),
    ("return", KeyCode::Enter),
    ("esc", KeyCode::Esc),
    ("escape", KeyCode::Esc),
    ("tab", KeyCode::Tab),
    ("backspace", KeyCode::Backspace),
    ("space", KeyCode::Char(' ')),
    ("up", KeyCode::Up),
    ("down", KeyCode::Down),
    ("left", KeyCode::Left),
    ("right", KeyCode::Right),
    ("home", KeyCode::Home),
    ("end", KeyCode::End),
    ("pageup", KeyCode::PageUp),
    ("page-up", KeyCode::PageUp),
    ("pagedown", KeyCode::PageDown),
    ("page-down", KeyCode::PageDown),
    ("delete", KeyCode::Delete),
    ("del", KeyCode::Delete),
    ("insert", KeyCode::Insert),
];

pub fn named(name: &str) -> Option<KeyEvent> {
    let mut mods = KeyModifiers::NONE;
    let mut rest = name;
    while let Some((prefix, modifier)) = MODIFIERS.iter().find(|(prefix, _)| {
        rest.len() > prefix.len() && rest.get(..prefix.len()).is_some_and(|p| p.eq_ignore_ascii_case(prefix))
    }) {
        mods |= *modifier;
        rest = &rest[prefix.len()..];
    }
    let mut chars = rest.chars();
    let code = match (chars.next(), chars.next()) {
        (Some(c), None) => {
            if c.is_ascii_uppercase() {
                mods |= KeyModifiers::SHIFT;
            }
            if mods.contains(KeyModifiers::SHIFT) { KeyCode::Char(c.to_ascii_uppercase()) } else { KeyCode::Char(c) }
        }
        _ => NAMED
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(rest))
            .map(|(_, code)| *code)
            .or_else(|| function(rest))?,
    };
    let code = if code == KeyCode::Tab && mods.contains(KeyModifiers::SHIFT) { KeyCode::BackTab } else { code };
    Some(KeyEvent::new(code, mods))
}

fn function(name: &str) -> Option<KeyCode> {
    let n: u8 = name.strip_prefix(['f', 'F'])?.parse().ok()?;
    (1..=12).contains(&n).then_some(KeyCode::F(n))
}

pub fn encode(event: KeyEvent, encoder: &mut Encoder<'_>, kitty: KittyKeyFlags, app_cursor: bool) -> Vec<u8> {
    if prefers_legacy(&event, kitty) {
        return legacy(event, app_cursor);
    }
    let mut out = Vec::new();
    match ghostty_event(&event) {
        Some(ev) if encoder.encode_to_vec(&ev, &mut out).is_ok() && !out.is_empty() => out,
        _ => legacy(event, app_cursor),
    }
}

fn prefers_legacy(event: &KeyEvent, kitty: KittyKeyFlags) -> bool {
    if !kitty.is_empty() {
        return false;
    }
    match event.code {
        KeyCode::Char(_) => true,
        KeyCode::Enter => !event.modifiers.is_empty(),
        KeyCode::Tab => event.modifiers == KeyModifiers::CONTROL,
        _ => false,
    }
}

fn ghostty_event(event: &KeyEvent) -> Option<key::Event<'static>> {
    let mut ev = key::Event::new().ok()?;
    ev.set_action(match event.kind {
        KeyEventKind::Press => Action::Press,
        KeyEventKind::Repeat => Action::Repeat,
        KeyEventKind::Release => Action::Release,
    });
    let mut mods = ghostty_mods(event.modifiers);
    if event.code == KeyCode::BackTab {
        mods |= Mods::SHIFT;
    }
    ev.set_mods(mods).set_key(ghostty_key(event.code)?);
    if let KeyCode::Char(c) = event.code {
        let base = unshifted(c);
        ev.set_utf8(Some(c.to_string())).set_unshifted_codepoint(base);
        if base != c {
            ev.set_consumed_mods(Mods::SHIFT);
        }
    }
    Some(ev)
}

fn ghostty_mods(mods: KeyModifiers) -> Mods {
    let mut out = Mods::empty();
    out.set(Mods::SHIFT, mods.contains(KeyModifiers::SHIFT));
    out.set(Mods::CTRL, mods.contains(KeyModifiers::CONTROL));
    out.set(Mods::ALT, mods.contains(KeyModifiers::ALT));
    out.set(Mods::SUPER, mods.contains(KeyModifiers::SUPER));
    out
}

fn ghostty_key(code: KeyCode) -> Option<key::Key> {
    use key::Key as G;

    Some(match code {
        KeyCode::Backspace => G::Backspace,
        KeyCode::Enter => G::Enter,
        KeyCode::Left => G::ArrowLeft,
        KeyCode::Right => G::ArrowRight,
        KeyCode::Up => G::ArrowUp,
        KeyCode::Down => G::ArrowDown,
        KeyCode::Home => G::Home,
        KeyCode::End => G::End,
        KeyCode::PageUp => G::PageUp,
        KeyCode::PageDown => G::PageDown,
        KeyCode::Tab | KeyCode::BackTab => G::Tab,
        KeyCode::Delete => G::Delete,
        KeyCode::Insert => G::Insert,
        KeyCode::Esc => G::Escape,
        KeyCode::F(n @ 1..=12) => {
            [G::F1, G::F2, G::F3, G::F4, G::F5, G::F6, G::F7, G::F8, G::F9, G::F10, G::F11, G::F12][usize::from(n - 1)]
        }
        KeyCode::Char(c) => char_key(unshifted(c))?,
        _ => return None,
    })
}

fn char_key(c: char) -> Option<key::Key> {
    use key::Key as G;

    const LETTERS: [key::Key; 26] = [
        G::A,
        G::B,
        G::C,
        G::D,
        G::E,
        G::F,
        G::G,
        G::H,
        G::I,
        G::J,
        G::K,
        G::L,
        G::M,
        G::N,
        G::O,
        G::P,
        G::Q,
        G::R,
        G::S,
        G::T,
        G::U,
        G::V,
        G::W,
        G::X,
        G::Y,
        G::Z,
    ];
    const DIGITS: [key::Key; 10] =
        [G::Digit0, G::Digit1, G::Digit2, G::Digit3, G::Digit4, G::Digit5, G::Digit6, G::Digit7, G::Digit8, G::Digit9];

    Some(match c {
        'a'..='z' => LETTERS[usize::from(c as u8 - b'a')],
        '0'..='9' => DIGITS[usize::from(c as u8 - b'0')],
        '`' => G::Backquote,
        '\\' => G::Backslash,
        '[' => G::BracketLeft,
        ']' => G::BracketRight,
        ',' => G::Comma,
        '=' => G::Equal,
        '-' => G::Minus,
        '.' => G::Period,
        '\'' => G::Quote,
        ';' => G::Semicolon,
        '/' => G::Slash,
        ' ' => G::Space,
        _ => return None,
    })
}

fn unshifted(c: char) -> char {
    match c {
        'A'..='Z' => c.to_ascii_lowercase(),
        '!' => '1',
        '@' => '2',
        '#' => '3',
        '$' => '4',
        '%' => '5',
        '^' => '6',
        '&' => '7',
        '*' => '8',
        '(' => '9',
        ')' => '0',
        '_' => '-',
        '+' => '=',
        '{' => '[',
        '}' => ']',
        '|' => '\\',
        ':' => ';',
        '"' => '\'',
        '<' => ',',
        '>' => '.',
        '?' => '/',
        '~' => '`',
        _ => c,
    }
}

fn legacy(key: KeyEvent, app_cursor: bool) -> Vec<u8> {
    let mods = key.modifiers;
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    let alt = mods.contains(KeyModifiers::ALT);
    let shift = mods.contains(KeyModifiers::SHIFT);
    let modifier_param = 1 + u8::from(shift) + u8::from(alt) * 2 + u8::from(ctrl) * 4;
    let modified = modifier_param > 1;

    let cursor_key = |c: char| -> Vec<u8> {
        if modified {
            format!("\x1b[1;{modifier_param}{c}").into_bytes()
        } else if app_cursor {
            format!("\x1bO{c}").into_bytes()
        } else {
            format!("\x1b[{c}").into_bytes()
        }
    };
    let tilde_key = |n: u8| -> Vec<u8> {
        if modified { format!("\x1b[{n};{modifier_param}~").into_bytes() } else { format!("\x1b[{n}~").into_bytes() }
    };

    let mut out = match key.code {
        KeyCode::Char(c) if ctrl => ctrl_char(c),
        KeyCode::Char(c) => c.to_string().into_bytes(),
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => cursor_key('A'),
        KeyCode::Down => cursor_key('B'),
        KeyCode::Right => cursor_key('C'),
        KeyCode::Left => cursor_key('D'),
        KeyCode::Home => cursor_key('H'),
        KeyCode::End => cursor_key('F'),
        KeyCode::Insert => tilde_key(2),
        KeyCode::Delete => tilde_key(3),
        KeyCode::PageUp => tilde_key(5),
        KeyCode::PageDown => tilde_key(6),
        KeyCode::F(n @ 1..=4) => {
            let c = char::from(b'P' + n - 1);
            if modified { format!("\x1b[1;{modifier_param}{c}").into_bytes() } else { format!("\x1bO{c}").into_bytes() }
        }
        KeyCode::F(5) => tilde_key(15),
        KeyCode::F(n @ 6..=10) => tilde_key(n + 11),
        KeyCode::F(n @ 11..=12) => tilde_key(n + 12),
        _ => vec![],
    };

    if alt && matches!(key.code, KeyCode::Char(_) | KeyCode::Enter | KeyCode::Backspace) {
        out.insert(0, 0x1b);
    }
    out
}

fn ctrl_char(c: char) -> Vec<u8> {
    match c.to_ascii_lowercase() {
        c @ 'a'..='z' => vec![c as u8 - b'a' + 1],
        ' ' | '@' | '2' => vec![0],
        '[' | '3' => vec![0x1b],
        '\\' | '4' => vec![0x1c],
        ']' | '5' => vec![0x1d],
        '^' | '6' => vec![0x1e],
        '_' | '-' | '7' => vec![0x1f],
        '?' | '8' => vec![0x7f],
        other => other.to_string().into_bytes(),
    }
}

#[cfg(test)]
mod tests;
