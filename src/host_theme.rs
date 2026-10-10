use std::fmt::Write as _;

use libghostty_vt::style::RgbColor;
use serde::{Deserialize, Serialize};

use crate::graphics::CellSize;
use crate::graphics::detect::{self, Replies};

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
    pub fn query(kitty: bool) -> String {
        let mut query = String::from(if kitty { detect::KITTY_QUERY } else { "" });
        query.push_str("\x1b[16t\x1b[?1;1;0S\x1b[?2;1;0S\x1b]10;?\x1b\\\x1b]11;?\x1b\\");
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
    replies: Replies,
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
                        self.replies.terminal = Some(name.to_string());
                    }
                    start += len;
                }
                Reply::Apc(body, len) => {
                    self.replies.kitty = kitty_reply(&body).or(self.replies.kitty.take());
                    start += len;
                }
                Reply::Csi(csi, len) => {
                    self.apply_csi(&csi);
                    start += len;
                }
                Reply::Other(len) => start += len,
            }
        }
        self.pending.drain(..start);
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
            (None, false, b't') => {
                if let [Some(6), Some(height), Some(width), ..] = params.as_slice() {
                    let (width, height) = (u16::try_from(*width), u16::try_from(*height));
                    if let (Ok(width), Ok(height)) = (width, height) {
                        self.replies.cell = Some(CellSize { width, height });
                    }
                }
            }
            _ => {}
        }
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    pub fn finish(self) -> (HostTheme, Replies) {
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

        const GHOSTTY: &[u8] = b"\x1b_Gi=31;OK\x1b\\\x1b[6;19;10t\x1b]10;rgb:e0e0/e0e0/e0e0\x1b\\\
            \x1b]11;rgb:2020/2020/2020\x1b\\\x1bP>|ghostty 1.3.1\x1b\\\x1b[?62;22;52c";
        const FOOT: &[u8] = b"\x1b[6;19;10t\x1b[?1;0;1024S\x1b[?2;0;1000;570S\x1bP>|foot(1.25.0)\x1b\\\
            \x1b[?62;4;22;28;52c";

        fn theme(input: &[u8]) -> HostTheme {
            probe(input).finish().0
        }

        fn replies(input: &[u8]) -> Replies {
            probe(input).finish().1
        }

        #[rstest]
        #[case::st_terminated(b"\x1b]11;rgb:0000/0000/0000\x1b\\")]
        #[case::bel_terminated(b"\x1b]11;rgb:0000/0000/0000\x07")]
        fn reads_the_background(#[case] input: &[u8]) {
            assert_eq!(theme(input).background, Some(rgb(0, 0, 0)));
        }

        #[test]
        fn reads_the_foreground() {
            assert_eq!(theme(b"\x1b]10;rgb:ffff/ffff/ffff\x1b\\").foreground, Some(rgb(255, 255, 255)));
        }

        #[test]
        fn reads_palette_entries() {
            assert_eq!(theme(b"\x1b]4;1;rgb:cc/00/00\x1b\\").palette[1], Some(rgb(0xcc, 0, 0)));
        }

        #[test]
        fn ignores_out_of_range_palette_entries() {
            assert_eq!(theme(b"\x1b]4;300;rgb:cc/00/00\x1b\\"), HostTheme::default());
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
            for chunk in [
                &b"\x1b"[..],
                b"]11;rgb:12",
                b"/34/56\x1b",
                b"\\\x1b[6;1",
                b"9;10t\x1b_Gi=3",
                b"1;OK\x1b",
                b"\\\x1b[?6",
                b"2c",
            ] {
                p.feed(chunk);
            }
            let done = p.is_done();
            let (theme, replies) = p.finish();

            assert_eq!(
                (done, theme.background, replies.cell, replies.kitty.as_deref()),
                (true, Some(rgb(0x12, 0x34, 0x56)), Some(CellSize { width: 10, height: 19 }), Some("OK"))
            );
        }

        #[test]
        fn skips_unrelated_bytes() {
            assert_eq!(theme(b"abc\x1b[A\x1b]11;#000000\x07").background, Some(rgb(0, 0, 0)));
        }

        #[test]
        fn reads_the_terminal_name_and_version() {
            let p = probe(b"\x1b]11;rgb:00/00/00\x1b\\\x1bP>|ghostty 1.2.0\x1b\\\x1b[?62;22c");
            assert_eq!((p.is_done(), p.finish().1.terminal.as_deref()), (true, Some("ghostty 1.2.0")));
        }

        #[rstest]
        #[case::decrqss(b"\x1bP1$r0m\x1b\\\x1b[?62;22c")]
        #[case::xtgettcap(b"\x1bP1+r544E=787465726D2D67686F73747479\x1b\\\x1b[?62;22c")]
        fn has_no_terminal_name_when_the_terminal_does_not_say(#[case] input: &[u8]) {
            assert_eq!(replies(input).terminal, None);
        }

        #[test]
        fn reads_everything_ghostty_answers() {
            let replies = replies(GHOSTTY);

            assert_eq!(
                replies,
                Replies {
                    terminal: Some("ghostty 1.3.1".into()),
                    attributes: Some(vec![62, 22, 52]),
                    kitty: Some("OK".into()),
                    cell: Some(CellSize { width: 10, height: 19 }),
                    registers: None,
                    geometry: None,
                }
            );
        }

        #[test]
        fn reads_the_sixel_limits_foot_answers() {
            let replies = replies(FOOT);

            assert_eq!(
                (replies.registers, replies.geometry, replies.attributes),
                (Some(1024), Some((1000, 570)), Some(vec![62, 4, 22, 28, 52]))
            );
        }

        #[rstest]
        #[case::xterm_without_sixel(b"\x1b[?1;3S\x1b[?2;3S\x1b[?62c")]
        #[case::iterm2_with_an_empty_value(b"\x1b[?1;3;S\x1b[?62c")]
        #[case::tmux_without_a_geometry(b"\x1b[?2;3;0S\x1b[?1;2;4c")]
        fn ignores_sixel_limits_the_terminal_failed_to_give(#[case] input: &[u8]) {
            let replies = replies(input);

            assert_eq!((replies.registers, replies.geometry), (None, None));
        }

        #[test]
        fn keeps_empty_device_attributes_out() {
            assert_eq!(replies(b"\x1bP>|kitty(0.45.0)\x1b\\\x1b[?62;52;c").attributes, Some(vec![62, 52]));
        }

        #[rstest]
        #[case::kitty_error(
            b"\x1b_Gi=31;ENOTSUPPORTED:unicode placeholders\x1b\\",
            Some("ENOTSUPPORTED:unicode placeholders")
        )]
        #[case::another_id(b"\x1b_Gi=7;OK\x1b\\", None)]
        #[case::not_kitty(b"\x1b_hello\x1b\\", None)]
        fn reads_the_answer_to_its_own_kitty_query(#[case] input: &[u8], #[case] expected: Option<&str>) {
            assert_eq!(replies(input).kitty.as_deref(), expected);
        }

        #[rstest]
        #[case::window_in_pixels(b"\x1b[4;570;1000t")]
        #[case::window_in_cells(b"\x1b[8;30;100t")]
        fn takes_only_the_cell_size_report_as_a_cell(#[case] input: &[u8]) {
            assert_eq!(replies(input).cell, None);
        }

        #[rstest]
        #[case::secondary_attributes(b"\x1b[>1;10;0c")]
        #[case::mode_report(b"\x1b[?2026;2$y")]
        #[case::konsole_cell_report(b"\x1b]1337;ReportCellSize=19.0;10.0;1.0\x07")]
        fn other_reports_do_not_end_the_probe(#[case] input: &[u8]) {
            let mut p = probe(input);
            p.feed(b"\x1b[?62;22c");

            assert_eq!(p.finish().1.attributes, Some(vec![62, 22]));
        }

        #[rstest]
        #[case::ghostty(GHOSTTY)]
        #[case::foot(FOOT)]
        fn reads_the_same_however_the_replies_are_split(#[case] stream: &[u8]) {
            let whole = replies(stream);
            for at in 1..stream.len() {
                let mut p = ThemeProbe::default();
                p.feed(&stream[..at]);
                p.feed(&stream[at..]);

                assert_eq!(p.finish().1, whole, "split at {at}");
            }
        }

        #[rstest]
        #[case::alt_underscore(b"\x1b_")]
        #[case::alt_p(b"\x1bP")]
        #[case::alt_bracket(b"\x1b]")]
        #[case::alt_underscore_and_a_letter(b"\x1b_x")]
        fn a_key_typed_during_the_probe_does_not_swallow_the_replies(#[case] key: &[u8]) {
            assert_eq!(replies(&[key, GHOSTTY].concat()), replies(GHOSTTY));
        }

        #[test]
        fn reads_what_the_terminal_outside_tmux_answers_through_it() {
            let replies = replies(b"\x1b_Gi=31;OK\x1b\\\x1bP>|kitty(0.45.0)\x1b\\\x1b[?62;52;c");

            assert_eq!(
                (replies.terminal.as_deref(), replies.kitty.as_deref(), replies.attributes),
                (Some("kitty(0.45.0)"), Some("OK"), Some(vec![62, 52]))
            );
        }
    }

    mod query {
        use super::*;

        #[test]
        fn asks_for_colors_and_ends_with_device_attributes() {
            let q = HostTheme::query(false);
            assert!(q.contains("\x1b]10;?\x1b\\\x1b]11;?\x1b\\") && q.ends_with("\x1b]4;255;?\x1b\\\x1b[>q\x1b[c"));
        }

        #[test]
        fn asks_for_the_cell_size_and_the_sixel_limits_first() {
            assert!(HostTheme::query(false).starts_with("\x1b[16t\x1b[?1;1;0S\x1b[?2;1;0S\x1b]10;?"));
        }

        #[rstest]
        #[case::asked(true, true)]
        #[case::not_asked(false, false)]
        fn sends_the_kitty_query_only_when_asked(#[case] kitty: bool, #[case] expected: bool) {
            assert_eq!(HostTheme::query(kitty).starts_with(detect::KITTY_QUERY), expected);
            assert_eq!(HostTheme::query(kitty).contains("\x1b_G"), expected);
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
