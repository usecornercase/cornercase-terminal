use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseMode {
    None,
    Press,
    PressRelease,
    ButtonMotion,
    AnyMotion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEncoding {
    Default,
    Utf8,
    Sgr,
}

const LEGACY_RELEASE: u32 = 3;
const MOTION: u32 = 32;
const WHEEL: u32 = 64;
const LEGACY_OFFSET: u32 = 32;

pub fn encode(ev: &MouseEvent, col: u16, row: u16, mode: MouseMode, encoding: MouseEncoding) -> Option<Vec<u8>> {
    if !is_wanted(ev.kind, mode) {
        return None;
    }

    let sgr = encoding == MouseEncoding::Sgr;
    let code = button_code(ev.kind, sgr) + modifier_bits(ev.modifiers);
    let (x, y) = (u32::from(col) + 1, u32::from(row) + 1);

    Some(match encoding {
        MouseEncoding::Sgr => {
            let end = if matches!(ev.kind, MouseEventKind::Up(_)) { 'm' } else { 'M' };
            format!("\x1b[<{code};{x};{y}{end}").into_bytes()
        }
        MouseEncoding::Default => {
            let byte = |v: u32| u8::try_from(LEGACY_OFFSET + v).unwrap_or(u8::MAX);
            vec![0x1b, b'[', b'M', byte(code), byte(x), byte(y)]
        }
        MouseEncoding::Utf8 => {
            let mut out = b"\x1b[M".to_vec();
            for v in [code, x, y] {
                let c = char::from_u32(LEGACY_OFFSET + v).unwrap_or(' ');
                out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
            }
            out
        }
    })
}

fn is_wanted(kind: MouseEventKind, mode: MouseMode) -> bool {
    use MouseEventKind as K;
    use MouseMode as M;

    match kind {
        K::Down(_) | K::ScrollUp | K::ScrollDown | K::ScrollLeft | K::ScrollRight => mode != M::None,
        K::Up(_) => matches!(mode, M::PressRelease | M::ButtonMotion | M::AnyMotion),
        K::Drag(_) => matches!(mode, M::ButtonMotion | M::AnyMotion),
        K::Moved => mode == M::AnyMotion,
    }
}

fn button_code(kind: MouseEventKind, sgr: bool) -> u32 {
    use MouseEventKind as K;

    let button = |b: MouseButton| match b {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    };
    match kind {
        K::Down(b) => button(b),
        K::Up(b) if sgr => button(b),
        K::Up(_) => LEGACY_RELEASE,
        K::Drag(b) => button(b) + MOTION,
        K::Moved => LEGACY_RELEASE + MOTION,
        K::ScrollUp => WHEEL,
        K::ScrollDown => WHEEL + 1,
        K::ScrollLeft => WHEEL + 2,
        K::ScrollRight => WHEEL + 3,
    }
}

fn modifier_bits(mods: KeyModifiers) -> u32 {
    let bit = |m: KeyModifiers, value: u32| if mods.contains(m) { value } else { 0 };
    bit(KeyModifiers::SHIFT, 4) + bit(KeyModifiers::ALT, 8) + bit(KeyModifiers::CONTROL, 16)
}

#[cfg(test)]
mod tests;
