use std::fmt::Write as _;

use libghostty_vt::style::RgbColor;
use serde::{Deserialize, Serialize};

pub const PALETTE_LEN: usize = 256;
const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
const LIGHT_LUMINANCE: u32 = 128_000;
const BRIGHT_BLACK: usize = 8;
const MIN_MUTED_CONTRAST: f64 = 2.0;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "WireTheme", into = "WireTheme")]
pub struct HostTheme {
    pub foreground: Option<RgbColor>,
    pub background: Option<RgbColor>,
    pub palette: [Option<RgbColor>; PALETTE_LEN],
    pub truecolor: bool,
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
    pub fn query() -> String {
        let mut query = String::from("\x1b]10;?\x1b\\\x1b]11;?\x1b\\");
        for i in 0..PALETTE_LEN {
            let _ = write!(query, "\x1b]4;{i};?\x1b\\");
        }
        query.push_str("\x1b[>q\x1b[c");
        query
    }

    pub fn is_light(&self) -> Option<bool> {
        self.background.map(|c| u32::from(c.r) * 299 + u32::from(c.g) * 587 + u32::from(c.b) * 114 >= LIGHT_LUMINANCE)
    }

    pub fn muted_is_readable(&self) -> bool {
        self.background
            .is_none_or(|bg| self.palette[BRIGHT_BLACK].is_some_and(|c| contrast(c, bg) >= MIN_MUTED_CONTRAST))
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
pub struct ThemeProbe {
    pending: Vec<u8>,
    theme: HostTheme,
    terminal: Option<String>,
    done: bool,
}

impl ThemeProbe {
    pub fn feed(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
        let mut start = 0;
        while !self.done && start < self.pending.len() {
            match parse_reply(&self.pending[start..]) {
                Reply::Incomplete => break,
                Reply::Osc(body, len) => {
                    self.theme.apply_osc(&body);
                    start += len;
                }
                Reply::Dcs(body, len) => {
                    if let Some(name) = body.strip_prefix(">|") {
                        self.terminal = Some(name.to_string());
                    }
                    start += len;
                }
                Reply::DeviceAttributes(len) => {
                    self.done = true;
                    start += len;
                }
                Reply::Other(len) => start += len,
            }
        }
        self.pending.drain(..start);
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    pub fn terminal(&self) -> Option<&str> {
        self.terminal.as_deref()
    }

    pub fn finish(self) -> HostTheme {
        self.theme
    }
}

enum Reply {
    Incomplete,
    Osc(String, usize),
    Dcs(String, usize),
    DeviceAttributes(usize),
    Other(usize),
}

fn parse_reply(buf: &[u8]) -> Reply {
    match buf {
        [ESC] | [ESC, b'['] => Reply::Incomplete,
        [ESC, kind @ (b']' | b'P'), rest @ ..] => match string_end(rest) {
            Some((body_len, term_len)) => {
                let body = String::from_utf8_lossy(&rest[..body_len]).into_owned();
                let len = 2 + body_len + term_len;
                if *kind == b']' { Reply::Osc(body, len) } else { Reply::Dcs(body, len) }
            }
            None => Reply::Incomplete,
        },
        [ESC, b'[', b'?', rest @ ..] => match rest.iter().position(|b| !b.is_ascii_digit() && *b != b';') {
            Some(i) if rest[i] == b'c' => Reply::DeviceAttributes(3 + i + 1),
            Some(_) => Reply::Other(1),
            None => Reply::Incomplete,
        },
        _ => Reply::Other(1),
    }
}

fn string_end(rest: &[u8]) -> Option<(usize, usize)> {
    rest.iter().enumerate().find_map(|(i, &b)| match (b, rest.get(i + 1)) {
        (BEL, _) => Some((i, 1)),
        (ESC, Some(b'\\')) => Some((i, 2)),
        _ => None,
    })
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
mod tests {
    use rstest::rstest;

    use super::*;

    const fn rgb(r: u8, g: u8, b: u8) -> RgbColor {
        RgbColor { r, g, b }
    }

    fn probe(input: &[u8]) -> ThemeProbe {
        let mut p = ThemeProbe::default();
        p.feed(input);
        p
    }

    mod parse_color {
        use super::*;

        #[rstest]
        #[case::sixteen_bit("rgb:cccc/dddd/eeee", Some(rgb(0xcc, 0xdd, 0xee)))]
        #[case::eight_bit("rgb:12/34/56", Some(rgb(0x12, 0x34, 0x56)))]
        #[case::four_bit("rgb:f/0/8", Some(rgb(0xff, 0x00, 0x88)))]
        #[case::hash("#123456", Some(rgb(0x12, 0x34, 0x56)))]
        #[case::missing_component("rgb:12/34", None)]
        #[case::not_hex("rgb:zz/00/00", None)]
        #[case::unknown_format("red", None)]
        fn reads_xterm_color_specs(#[case] value: &str, #[case] expected: Option<RgbColor>) {
            assert_eq!(parse_color(value), expected);
        }
    }

    mod probe {
        use super::*;

        #[rstest]
        #[case::st_terminated(b"\x1b]11;rgb:0000/0000/0000\x1b\\")]
        #[case::bel_terminated(b"\x1b]11;rgb:0000/0000/0000\x07")]
        fn reads_the_background(#[case] input: &[u8]) {
            assert_eq!(probe(input).finish().background, Some(rgb(0, 0, 0)));
        }

        #[test]
        fn reads_the_foreground() {
            assert_eq!(probe(b"\x1b]10;rgb:ffff/ffff/ffff\x1b\\").finish().foreground, Some(rgb(255, 255, 255)));
        }

        #[test]
        fn reads_palette_entries() {
            assert_eq!(probe(b"\x1b]4;1;rgb:cc/00/00\x1b\\").finish().palette[1], Some(rgb(0xcc, 0, 0)));
        }

        #[test]
        fn ignores_out_of_range_palette_entries() {
            assert_eq!(probe(b"\x1b]4;300;rgb:cc/00/00\x1b\\").finish(), HostTheme::default());
        }

        #[test]
        fn is_done_after_device_attributes() {
            assert!(probe(b"\x1b]11;rgb:00/00/00\x1b\\\x1b[?62;22c").is_done());
        }

        #[test]
        fn is_not_done_without_device_attributes() {
            assert!(!probe(b"\x1b]11;rgb:00/00/00\x1b\\").is_done());
        }

        #[test]
        fn joins_replies_split_across_reads() {
            let mut p = ThemeProbe::default();
            for chunk in [&b"\x1b"[..], b"]11;rgb:12", b"/34/56\x1b", b"\\\x1b[?6", b"2c"] {
                p.feed(chunk);
            }
            assert_eq!((p.is_done(), p.finish().background), (true, Some(rgb(0x12, 0x34, 0x56))));
        }

        #[test]
        fn skips_unrelated_bytes() {
            assert_eq!(probe(b"abc\x1b[A\x1b]11;#000000\x07").finish().background, Some(rgb(0, 0, 0)));
        }

        #[test]
        fn reads_the_terminal_name_and_version() {
            let p = probe(b"\x1b]11;rgb:00/00/00\x1b\\\x1bP>|ghostty 1.2.0\x1b\\\x1b[?62;22c");
            assert_eq!((p.terminal(), p.is_done()), (Some("ghostty 1.2.0"), true));
        }

        #[test]
        fn has_no_terminal_name_when_the_terminal_does_not_say() {
            assert_eq!(probe(b"\x1bP1$r0m\x1b\\\x1b[?62;22c").terminal(), None);
        }
    }

    mod query {
        use super::*;

        #[test]
        fn asks_for_colors_and_ends_with_device_attributes() {
            let q = HostTheme::query();
            assert!(q.starts_with("\x1b]10;?\x1b\\\x1b]11;?\x1b\\") && q.ends_with("\x1b]4;255;?\x1b\\\x1b[>q\x1b[c"));
        }
    }

    mod is_light {
        use super::*;

        #[rstest]
        #[case::unknown(None, None)]
        #[case::black(Some(rgb(0, 0, 0)), Some(false))]
        #[case::white(Some(rgb(255, 255, 255)), Some(true))]
        #[case::solarized_light(Some(rgb(0xfd, 0xf6, 0xe3)), Some(true))]
        #[case::solarized_dark(Some(rgb(0x00, 0x2b, 0x36)), Some(false))]
        fn follows_background_luminance(#[case] background: Option<RgbColor>, #[case] expected: Option<bool>) {
            assert_eq!(HostTheme { background, ..HostTheme::default() }.is_light(), expected);
        }
    }

    mod muted_is_readable {
        use super::*;

        #[rstest]
        #[case::nothing_known(None, None, true)]
        #[case::palette_without_background(None, Some(rgb(0x2b, 0x2b, 0x2a)), true)]
        #[case::warp_adeberry_without_palette(Some(rgb(0x1d, 0x20, 0x22)), None, false)]
        #[case::warp_adeberry_as_drawn(Some(rgb(0x1d, 0x20, 0x22)), Some(rgb(0x2b, 0x2b, 0x2a)), false)]
        #[case::solarized_dark(Some(rgb(0x00, 0x2b, 0x36)), Some(rgb(0x00, 0x2b, 0x36)), false)]
        #[case::nord(Some(rgb(0x2e, 0x34, 0x40)), Some(rgb(0x4c, 0x56, 0x6a)), false)]
        #[case::one_dark(Some(rgb(0x28, 0x2c, 0x34)), Some(rgb(0x66, 0x66, 0x66)), true)]
        #[case::dracula(Some(rgb(0x28, 0x2a, 0x36)), Some(rgb(0x62, 0x72, 0xa4)), true)]
        #[case::solarized_light(Some(rgb(0xfd, 0xf6, 0xe3)), Some(rgb(0x00, 0x2b, 0x36)), true)]
        fn needs_the_bright_black_to_stand_out_from_a_known_background(
            #[case] background: Option<RgbColor>,
            #[case] bright_black: Option<RgbColor>,
            #[case] expected: bool,
        ) {
            let mut theme = HostTheme { background, ..HostTheme::default() };
            theme.palette[8] = bright_black;

            assert_eq!(theme.muted_is_readable(), expected);
        }
    }

    mod wire {
        use super::*;

        #[test]
        fn round_trips_through_serde() {
            let mut theme = HostTheme { background: Some(rgb(1, 2, 3)), ..HostTheme::default() };
            theme.palette[200] = Some(rgb(4, 5, 6));

            let bytes = postcard::to_stdvec(&theme).expect("encode theme");

            assert_eq!(postcard::from_bytes::<HostTheme>(&bytes).expect("decode theme"), theme);
        }
    }
}
