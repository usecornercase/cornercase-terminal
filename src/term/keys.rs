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

pub(crate) fn named(name: &str) -> Option<KeyEvent> {
    let mut mods = KeyModifiers::NONE;
    let mut rest = name;
    while let Some((prefix, modifier)) = MODIFIERS.iter().find(|(prefix, _)| {
        rest.len() > prefix.len() && rest.get(..prefix.len()).is_some_and(|p| p.eq_ignore_ascii_case(prefix))
    }) {
        mods |= *modifier;
        rest = &rest[prefix.len()..];
    }
    event(rest, mods, KeyEventKind::Press)
}

pub(crate) fn event(code: &str, mut mods: KeyModifiers, kind: KeyEventKind) -> Option<KeyEvent> {
    let mut chars = code.chars();
    let code = match (chars.next(), chars.next()) {
        (Some(c), None) => {
            if c.is_ascii_uppercase() {
                mods |= KeyModifiers::SHIFT;
            }
            if mods.contains(KeyModifiers::SHIFT) { KeyCode::Char(c.to_ascii_uppercase()) } else { KeyCode::Char(c) }
        }
        _ => NAMED
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(code))
            .map(|(_, code)| *code)
            .or_else(|| function(code))?,
    };
    let code = if code == KeyCode::Tab && mods.contains(KeyModifiers::SHIFT) { KeyCode::BackTab } else { code };
    Some(KeyEvent::new_with_kind(code, mods, kind))
}

pub(crate) fn typed(c: char) -> KeyEvent {
    let mut text = [0; 4];
    event(c.encode_utf8(&mut text), KeyModifiers::NONE, KeyEventKind::Press)
        .unwrap_or_else(|| KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

fn function(name: &str) -> Option<KeyCode> {
    let n: u8 = name.strip_prefix(['f', 'F'])?.parse().ok()?;
    (1..=12).contains(&n).then_some(KeyCode::F(n))
}

pub(crate) fn encode(event: KeyEvent, encoder: &mut Encoder<'_>, kitty: KittyKeyFlags, app_cursor: bool) -> Vec<u8> {
    if event.kind == KeyEventKind::Release {
        return release(&event, encoder, kitty);
    }
    if prefers_legacy(&event, kitty) {
        return legacy(event, app_cursor);
    }
    let mut out = Vec::new();
    match ghostty_event(&event) {
        Some(ev) if encoder.encode_to_vec(&ev, &mut out).is_ok() && !out.is_empty() => out,
        _ => legacy(event, app_cursor),
    }
}

fn release(event: &KeyEvent, encoder: &mut Encoder<'_>, kitty: KittyKeyFlags) -> Vec<u8> {
    let mut out = Vec::new();
    if !kitty.contains(KittyKeyFlags::REPORT_EVENTS) {
        return out;
    }
    match ghostty_event(event) {
        Some(ev) if encoder.encode_to_vec(&ev, &mut out).is_ok() => out,
        _ => Vec::new(),
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

const LETTERS: [key::Key; 26] = [
    key::Key::A,
    key::Key::B,
    key::Key::C,
    key::Key::D,
    key::Key::E,
    key::Key::F,
    key::Key::G,
    key::Key::H,
    key::Key::I,
    key::Key::J,
    key::Key::K,
    key::Key::L,
    key::Key::M,
    key::Key::N,
    key::Key::O,
    key::Key::P,
    key::Key::Q,
    key::Key::R,
    key::Key::S,
    key::Key::T,
    key::Key::U,
    key::Key::V,
    key::Key::W,
    key::Key::X,
    key::Key::Y,
    key::Key::Z,
];

const DIGITS: [key::Key; 10] = [
    key::Key::Digit0,
    key::Key::Digit1,
    key::Key::Digit2,
    key::Key::Digit3,
    key::Key::Digit4,
    key::Key::Digit5,
    key::Key::Digit6,
    key::Key::Digit7,
    key::Key::Digit8,
    key::Key::Digit9,
];

fn char_key(c: char) -> Option<key::Key> {
    use key::Key as G;

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

struct Legacy {
    param: u8,
    app_cursor: bool,
}

impl Legacy {
    fn modified(&self) -> bool {
        self.param > 1
    }

    fn cursor_key(&self, c: char) -> Vec<u8> {
        let param = self.param;
        if self.modified() {
            format!("\x1b[1;{param}{c}").into_bytes()
        } else if self.app_cursor {
            format!("\x1bO{c}").into_bytes()
        } else {
            format!("\x1b[{c}").into_bytes()
        }
    }

    fn tilde_key(&self, n: u8) -> Vec<u8> {
        let param = self.param;
        if self.modified() { format!("\x1b[{n};{param}~").into_bytes() } else { format!("\x1b[{n}~").into_bytes() }
    }

    fn function_key(&self, n: u8) -> Vec<u8> {
        let (param, c) = (self.param, char::from(b'P' + n - 1));
        if self.modified() { format!("\x1b[1;{param}{c}").into_bytes() } else { format!("\x1bO{c}").into_bytes() }
    }
}

fn legacy(key: KeyEvent, app_cursor: bool) -> Vec<u8> {
    let mods = key.modifiers;
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    let alt = mods.contains(KeyModifiers::ALT);
    let shift = mods.contains(KeyModifiers::SHIFT);
    let keys = Legacy { param: 1 + u8::from(shift) + u8::from(alt) * 2 + u8::from(ctrl) * 4, app_cursor };

    let mut out = match key.code {
        KeyCode::Char(c) if ctrl => ctrl_char(c),
        KeyCode::Char(c) => c.to_string().into_bytes(),
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => keys.cursor_key('A'),
        KeyCode::Down => keys.cursor_key('B'),
        KeyCode::Right => keys.cursor_key('C'),
        KeyCode::Left => keys.cursor_key('D'),
        KeyCode::Home => keys.cursor_key('H'),
        KeyCode::End => keys.cursor_key('F'),
        KeyCode::Insert => keys.tilde_key(2),
        KeyCode::Delete => keys.tilde_key(3),
        KeyCode::PageUp => keys.tilde_key(5),
        KeyCode::PageDown => keys.tilde_key(6),
        KeyCode::F(n @ 1..=4) => keys.function_key(n),
        KeyCode::F(5) => keys.tilde_key(15),
        KeyCode::F(n @ 6..=10) => keys.tilde_key(n + 11),
        KeyCode::F(n @ 11..=12) => keys.tilde_key(n + 12),
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
