use std::fmt::Write as _;

use libghostty_vt::style::RgbColor;
use serde::{Deserialize, Serialize};

use crate::graphics::CellSize;
use crate::graphics::detect::{self, Replies};

pub(crate) const PALETTE_LEN: usize = 256;
const THEMED_COLORS: usize = 16;
const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
const LIGHT_LUMINANCE: u32 = 128_000;
const WHITE: usize = 7;
const BRIGHT_BLACK: usize = 8;
const MIN_MUTED_CONTRAST: f64 = 2.0;
const MIN_SOFT_CONTRAST: f64 = 3.0;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "WireTheme", into = "WireTheme")]
pub(crate) struct HostTheme {
    pub(crate) foreground: Option<RgbColor>,
    pub(crate) background: Option<RgbColor>,
    pub(crate) palette: [Option<RgbColor>; PALETTE_LEN],
    pub(crate) truecolor: bool,
}

impl Default for HostTheme {
    fn default() -> Self {
        Self { foreground: None, background: None, palette: [None; PALETTE_LEN], truecolor: false }
    }
}

type WireColor = Option<(u8, u8, u8)>;

#[derive(Serialize, Deserialize)]
struct WireTheme {
    foreground: WireColor,
    background: WireColor,
    palette: Vec<WireColor>,
    #[serde(default)]
    truecolor: bool,
}

impl From<HostTheme> for WireTheme {
    fn from(theme: HostTheme) -> Self {
        let wire = |c: Option<RgbColor>| c.map(|c| (c.r, c.g, c.b));
        Self {
            foreground: wire(theme.foreground),
            background: wire(theme.background),
            palette: theme.palette.iter().copied().map(wire).collect(),
            truecolor: theme.truecolor,
        }
    }
}

impl From<WireTheme> for HostTheme {
    fn from(wire: WireTheme) -> Self {
        let rgb = |c: WireColor| c.map(|(r, g, b)| RgbColor { r, g, b });
        let mut theme = Self {
            foreground: rgb(wire.foreground),
            background: rgb(wire.background),
            truecolor: wire.truecolor,
            ..Self::default()
        };
        for (slot, color) in theme.palette.iter_mut().zip(wire.palette) {
            *slot = rgb(color);
        }
        theme
    }
}

impl HostTheme {
    pub(crate) fn query(kitty: bool) -> String {
        let mut query = String::from(if kitty { detect::KITTY_QUERY } else { "" });
        query.push_str("\x1b[16t\x1b[?1;1;0S\x1b[?2;1;0S\x1b]10;?\x1b\\\x1b]11;?\x1b\\");
        for i in 0..THEMED_COLORS {
            let _ = write!(query, "\x1b]4;{i};?\x1b\\");
        }
        query.push_str("\x1b[>q\x1b[c");
        query
    }

    pub(crate) fn is_light(&self) -> Option<bool> {
        self.background.map(|c| u32::from(c.r) * 299 + u32::from(c.g) * 587 + u32::from(c.b) * 114 >= LIGHT_LUMINANCE)
    }

    pub(crate) fn muted_is_readable(&self) -> bool {
        self.stands_out(BRIGHT_BLACK, MIN_MUTED_CONTRAST)
    }

    pub(crate) fn soft_is_readable(&self, surface: RgbColor) -> bool {
        self.stands_out(WHITE, MIN_SOFT_CONTRAST)
            && self.palette[WHITE].is_none_or(|c| contrast(c, surface) >= MIN_SOFT_CONTRAST)
    }

    fn stands_out(&self, index: usize, min: f64) -> bool {
        self.background.is_none_or(|bg| self.palette[index].is_some_and(|c| contrast(c, bg) >= min))
    }

    fn apply_osc(&mut self, body: &str) {
        let mut parts = body.splitn(3, ';');
        match (parts.next(), parts.next(), parts.next()) {
            (Some("10"), Some(value), None) => self.foreground = parse_color(value).or(self.foreground),
            (Some("11"), Some(value), None) => self.background = parse_color(value).or(self.background),
            (Some("4"), Some(index), Some(value)) => {
                if let (Ok(i), Some(color)) = (index.parse::<usize>(), parse_color(value))
                    && let Some(slot) = self.palette.get_mut(i)
                {
                    *slot = Some(color);
                }
            }
            _ => {}
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct ThemeProbe {
    pending: Vec<u8>,
    theme: HostTheme,
    replies: Replies,
    done: bool,
}

impl ThemeProbe {
    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
        let mut start = 0;
        while !self.done && start < self.pending.len() {
            let reply = parse_reply(&self.pending[start..]);
            if matches!(reply, Reply::Incomplete) {
                break;
            }
            start += self.take(reply);
        }
        self.pending.drain(..start);
    }

    fn take(&mut self, reply: Reply) -> usize {
        match reply {
            Reply::Incomplete => 0,
            Reply::Osc(body, len) => {
                self.theme.apply_osc(&body);
                len
            }
            Reply::Dcs(body, len) => {
                if let Some(name) = body.strip_prefix(">|") {
                    self.replies.terminal = Some(name.to_string());
                }
                len
            }
            Reply::Apc(body, len) => {
                self.replies.kitty = kitty_reply(&body).or(self.replies.kitty.take());
                len
            }
            Reply::Csi(csi, len) => {
                self.apply_csi(&csi);
                len
            }
            Reply::Other(len) => len,
        }
    }

    fn apply_csi(&mut self, csi: &Csi) {
        let params = csi.numbers();
        match (csi.private, csi.intermediate, csi.last) {
            (Some(b'?'), false, b'c') => {
                self.replies.attributes = Some(params.into_iter().flatten().collect());
                self.done = true;
            }
            (Some(b'?'), false, b'S') => match params.as_slice() {
                [Some(1), Some(0), Some(registers), ..] => self.replies.registers = Some(*registers),
                [Some(2), Some(0), Some(width), Some(height), ..] => self.replies.geometry = Some((*width, *height)),
                _ => {}
            },
            (None, false, b't') => self.apply_cell(&params),
            _ => {}
        }
    }

    fn apply_cell(&mut self, params: &[Option<u32>]) {
        if let [Some(6), Some(height), Some(width), ..] = params
            && let (Ok(width), Ok(height)) = (u16::try_from(*width), u16::try_from(*height))
        {
            self.replies.cell = Some(CellSize { width, height });
        }
    }

    pub(crate) fn is_done(&self) -> bool {
        self.done
    }

    pub(crate) fn finish(self) -> (HostTheme, Replies) {
        (self.theme, self.replies)
    }
}

fn kitty_reply(body: &str) -> Option<String> {
    let (keys, message) = body.strip_prefix('G')?.split_once(';')?;
    keys.split(',').any(|key| key.strip_prefix("i=") == Some(detect::KITTY_QUERY_ID)).then(|| message.to_string())
}

enum Reply {
    Incomplete,
    Osc(String, usize),
    Dcs(String, usize),
    Apc(String, usize),
    Csi(Csi, usize),
    Other(usize),
}

struct Csi {
    private: Option<u8>,
    params: String,
    intermediate: bool,
    last: u8,
}

impl Csi {
    fn numbers(&self) -> Vec<Option<u32>> {
        self.params.split(';').map(|p| p.parse().ok()).collect()
    }
}

fn parse_reply(buf: &[u8]) -> Reply {
    match buf {
        [ESC] => Reply::Incomplete,
        [ESC, kind @ (b']' | b'P' | b'_'), rest @ ..] => match string_end(rest) {
            End::Found(body_len, term_len) => {
                let body = String::from_utf8_lossy(&rest[..body_len]).into_owned();
                let len = 2 + body_len + term_len;
                match kind {
                    b']' => Reply::Osc(body, len),
                    b'P' => Reply::Dcs(body, len),
                    _ => Reply::Apc(body, len),
                }
            }
            End::Cut(at) => Reply::Other(2 + at),
            End::Incomplete => Reply::Incomplete,
        },
        [ESC, b'[', rest @ ..] => parse_csi(rest),
        _ => Reply::Other(1),
    }
}

fn parse_csi(rest: &[u8]) -> Reply {
    let private = rest.first().copied().filter(|b| matches!(b, b'<'..=b'?'));
    let params_start = usize::from(private.is_some());
    let params_len = rest[params_start..].iter().take_while(|b| matches!(b, b'0'..=b';')).count();
    let after_params = params_start + params_len;
    let intermediates = rest[after_params..].iter().take_while(|b| matches!(b, b' '..=b'/')).count();
    match rest.get(after_params + intermediates) {
        None => Reply::Incomplete,
        Some(&last @ b'@'..=b'~') => Reply::Csi(
            Csi {
                private,
                params: String::from_utf8_lossy(&rest[params_start..after_params]).into_owned(),
                intermediate: intermediates > 0,
                last,
            },
            2 + after_params + intermediates + 1,
        ),
        Some(_) => Reply::Other(1),
    }
}

enum End {
    Found(usize, usize),
    Cut(usize),
    Incomplete,
}

fn string_end(rest: &[u8]) -> End {
    rest.iter()
        .enumerate()
        .find_map(|(i, &b)| match (b, rest.get(i + 1)) {
            (BEL, _) => Some(End::Found(i, 1)),
            (ESC, Some(b'\\')) => Some(End::Found(i, 2)),
            (ESC, Some(_)) => Some(End::Cut(i)),
            _ => None,
        })
        .unwrap_or(End::Incomplete)
}

fn contrast(a: RgbColor, b: RgbColor) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn luminance(c: RgbColor) -> f64 {
    let linear = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
}

fn parse_color(value: &str) -> Option<RgbColor> {
    let [r, g, b] = if let Some(rgb) = value.strip_prefix("rgb:") {
        let parts: Vec<&str> = rgb.split('/').collect();
        <[&str; 3]>::try_from(parts).ok()?
    } else {
        let hex = value.strip_prefix('#')?;
        let n = hex.len() / 3;
        if n == 0 || hex.len() != n * 3 || !hex.is_ascii() {
            return None;
        }
        [&hex[..n], &hex[n..2 * n], &hex[2 * n..]]
    };
    Some(RgbColor { r: scale(r)?, g: scale(g)?, b: scale(b)? })
}

fn scale(component: &str) -> Option<u8> {
    if !(1..=4).contains(&component.len()) {
        return None;
    }
    let value = u32::from_str_radix(component, 16).ok()?;
    let max = (1u32 << (component.len() * 4)) - 1;
    u8::try_from((value * 255 + max / 2) / max).ok()
}

#[cfg(test)]
mod tests;
