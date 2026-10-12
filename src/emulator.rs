use std::cell::RefCell;
use std::rc::Rc;

use crossterm::event::KeyEvent;
use libghostty_vt::Terminal;
use libghostty_vt::fmt::{Format, Formatter, FormatterOptions};
use libghostty_vt::key::{Encoder, KittyKeyFlags};
use libghostty_vt::render::{CellIterator, RenderState, RowIterator};
use libghostty_vt::screen::{Screen, TrackedGridRef};
use libghostty_vt::selection::{FormatOptions, Selection};
use libghostty_vt::style::{Palette, StyleColor, Underline};
use libghostty_vt::terminal::{
    ClipboardLocation, ClipboardWrite, ColorScheme, Mode, Options, Point, PointCoordinate, ScrollViewport,
};
use ratatui::layout::Position;
use ratatui::style::{Color, Modifier, Style};

use crate::host_theme::HostTheme;
use crate::keys;
use crate::mouse::{MouseEncoding, MouseMode};

pub type Result<T> = libghostty_vt::error::Result<T>;
pub type ReplyFn = Box<dyn FnMut(&[u8])>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub symbol: String,
    pub style: Style,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub rows: Vec<Vec<Cell>>,
    pub cursor: Option<Position>,
    pub below: usize,
}

impl Snapshot {
    pub fn contents(&self) -> String {
        self.rows
            .iter()
            .map(|row| row.iter().map(|c| c.symbol.as_str()).collect::<String>().trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub struct Emulator {
    vt: Terminal<'static, 'static>,
    render: RenderState<'static>,
    rows: RowIterator<'static>,
    cells: CellIterator<'static>,
    keys: Encoder<'static>,
    anchor: Option<TrackedGridRef>,
    copied: Rc<RefCell<Vec<String>>>,
}

impl Emulator {
    pub fn new(rows: u16, cols: u16, scrollback: usize, theme: &HostTheme, mut reply: ReplyFn) -> Result<Self> {
        let mut vt = Terminal::new(Options { cols, rows, max_scrollback: scrollback })?;
        vt.on_pty_write(move |_, bytes| reply(bytes))?;
        let copied = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&copied);
        vt.on_clipboard_write(move |_, write| {
            if let Some(text) = clipboard_text(&write) {
                sink.borrow_mut().push(text);
            }
            Ok(())
        })?;
        apply_theme(&mut vt, theme)?;
        Ok(Self {
            vt,
            render: RenderState::new()?,
            rows: RowIterator::new()?,
            cells: CellIterator::new()?,
            keys: Encoder::new()?,
            anchor: None,
            copied,
        })
    }

    pub fn take_copied(&mut self) -> Vec<String> {
        std::mem::take(&mut self.copied.borrow_mut())
    }

    pub fn encode_key(&mut self, key: KeyEvent) -> Vec<u8> {
        self.keys.set_options_from_terminal(&self.vt);
        let kitty = self.vt.kitty_keyboard_flags().unwrap_or(KittyKeyFlags::DISABLED);
        let app_cursor = self.application_cursor();
        keys::encode(key, &mut self.keys, kitty, app_cursor)
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.vt.vt_write(bytes);
    }

    pub fn resize(&mut self, rows: u16, cols: u16) -> Result<()> {
        self.vt.resize(cols, rows, 0, 0)
    }

    pub fn size(&self) -> Result<(u16, u16)> {
        Ok((self.vt.rows()?, self.vt.cols()?))
    }

    fn mode(&self, mode: Mode) -> bool {
        self.vt.mode(mode).unwrap_or(false)
    }

    pub fn application_cursor(&self) -> bool {
        self.mode(Mode::DECCKM)
    }

    pub fn title(&self) -> String {
        self.vt.title().map(str::to_string).unwrap_or_default()
    }

    pub fn bracketed_paste(&self) -> bool {
        self.mode(Mode::BRACKETED_PASTE)
    }

    pub fn alternate_screen(&self) -> bool {
        matches!(self.vt.active_screen(), Ok(Screen::Alternate))
    }

    pub fn alt_scroll(&self) -> bool {
        self.mode(Mode::ALT_SCROLL)
    }

    pub fn scroll(&mut self, rows: isize) {
        self.vt.scroll_viewport(ScrollViewport::Delta(rows));
    }

    pub fn scroll_to_bottom(&mut self) {
        self.vt.scroll_viewport(ScrollViewport::Bottom);
    }

    pub fn below(&self) -> usize {
        self.vt.scrollbar().map_or(0, |bar| {
            usize::try_from(bar.total.saturating_sub(bar.offset.saturating_add(bar.len))).unwrap_or(usize::MAX)
        })
    }

    pub fn active_contents(&mut self) -> Result<String> {
        let scrolled = self.vt.scrollbar().ok().filter(|_| self.below() > 0);
        if scrolled.is_some() {
            self.scroll_to_bottom();
        }
        let text = self.snapshot().map(|s| s.contents());
        if let Some(bar) = scrolled {
            self.vt.scroll_viewport(ScrollViewport::Row(usize::try_from(bar.offset).unwrap_or(usize::MAX)));
        }
        text
    }

    pub fn mouse_mode(&self) -> MouseMode {
        [
            (Mode::ANY_MOUSE, MouseMode::AnyMotion),
            (Mode::BUTTON_MOUSE, MouseMode::ButtonMotion),
            (Mode::NORMAL_MOUSE, MouseMode::PressRelease),
            (Mode::X10_MOUSE, MouseMode::Press),
        ]
        .into_iter()
        .find_map(|(mode, ours)| self.mode(mode).then_some(ours))
        .unwrap_or(MouseMode::None)
    }

    pub fn mouse_encoding(&self) -> MouseEncoding {
        if self.mode(Mode::SGR_MOUSE) {
            MouseEncoding::Sgr
        } else if self.mode(Mode::UTF8_MOUSE) {
            MouseEncoding::Utf8
        } else {
            MouseEncoding::Default
        }
    }

    pub fn start_selection(&mut self, at: Position) -> Result<()> {
        self.vt.set_selection(None)?;
        self.anchor = Some(self.vt.track_grid_ref(viewport(at))?);
        Ok(())
    }

    pub fn extend_selection(&mut self, to: Position) -> Result<()> {
        let Some(anchor) = &self.anchor else { return Ok(()) };
        let Some(start) = anchor.snapshot(&self.vt)? else { return Ok(()) };
        let end = self.vt.grid_ref(viewport(to))?;
        self.vt.set_selection(Some(&Selection::new(start, end, false)))?;
        Ok(())
    }

    pub fn finish_selection(&mut self) -> Result<Option<String>> {
        self.anchor = None;
        let text = self.vt.format_selection_alloc(None, plain())?.map(|b| String::from_utf8_lossy(&b).into_owned());
        self.vt.set_selection(None)?;
        Ok(text.filter(|t| !t.is_empty()))
    }

    pub fn screen_text(&self) -> Result<String> {
        let (rows, cols) = self.size()?;
        let at = |x, y| self.vt.grid_ref(Point::Active(PointCoordinate { x, y }));
        let selection =
            Selection::new(at(0, 0)?, at(cols.saturating_sub(1), u32::from(rows.saturating_sub(1)))?, false);
        let text = self.vt.format_selection_alloc(None, plain().with_selection(&selection))?;
        Ok(tail(&text.map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default(), None))
    }

    pub fn last_lines(&self, lines: usize) -> Result<String> {
        let options = FormatterOptions::new().with_format(Format::Plain).with_unwrap(true).with_trim(true);
        let text = Formatter::new(&self.vt, options)?.format_alloc(None)?;
        Ok(tail(&String::from_utf8_lossy(&text), Some(lines)))
    }

    pub fn snapshot(&mut self) -> Result<Snapshot> {
        if self.below() > 0 && self.mouse_mode() != MouseMode::None {
            self.scroll_to_bottom();
        }
        let snap = self.render.update(&self.vt)?;
        let cursor =
            if snap.cursor_visible()? { snap.cursor_viewport()?.map(|vp| Position::new(vp.x, vp.y)) } else { None };
        let mut rows = Vec::new();
        let mut text = String::new();
        let mut row_it = self.rows.update(&snap)?;
        while let Some(row) = row_it.next() {
            let mut cells = Vec::new();
            let mut cell_it = self.cells.update(row)?;
            while let Some(cell) = cell_it.next() {
                text.clear();
                if cell.graphemes_len()? > 0 {
                    cell.graphemes_utf8(&mut text)?;
                }
                text.retain(|c| !c.is_control());
                let symbol = if text.is_empty() { " ".to_string() } else { text.clone() };
                let style = if cell.has_styling()? { to_style(cell.style()?) } else { Style::default() };
                let style = if cell.is_selected()? { selected(style) } else { style };
                cells.push(Cell { symbol, style });
            }
            rows.push(cells);
        }
        Ok(Snapshot { rows, cursor, below: self.below() })
    }
}

fn plain<'t, 's>() -> FormatOptions<'t, 's> {
    FormatOptions::new().with_emit_format(Format::Plain).with_unwrap(true).with_trim(true)
}

fn tail(text: &str, lines: Option<usize>) -> String {
    let all: Vec<&str> = text.lines().collect();
    let end = all.iter().rposition(|line| !line.trim().is_empty()).map_or(0, |i| i + 1);
    let start = lines.map_or(0, |n| end.saturating_sub(n));
    all[start..end].join("\n")
}

fn clipboard_text(write: &ClipboardWrite<'_>) -> Option<String> {
    if write.location() != ClipboardLocation::Standard {
        return None;
    }
    let mut contents = write.contents();
    let text = contents.clone().find(|c| c.mime.starts_with("text/plain")).or_else(|| contents.next())?;
    (!text.data.is_empty()).then(|| text.data.to_string())
}

fn viewport(at: Position) -> Point {
    Point::Viewport(PointCoordinate { x: at.x, y: u32::from(at.y) })
}

fn selected(style: Style) -> Style {
    if style.add_modifier.contains(Modifier::REVERSED) {
        style.remove_modifier(Modifier::REVERSED)
    } else {
        style.add_modifier(Modifier::REVERSED)
    }
}

fn apply_theme(vt: &mut Terminal<'static, 'static>, theme: &HostTheme) -> Result<()> {
    if theme.foreground.is_some() {
        vt.set_default_fg_color(theme.foreground)?;
    }
    if theme.background.is_some() {
        vt.set_default_bg_color(theme.background)?;
    }
    if theme.palette.iter().any(Option::is_some) {
        let Palette(mut colors) = vt.default_color_palette()?;
        for (slot, host) in colors.iter_mut().zip(&theme.palette) {
            if let Some(host) = host {
                *slot = *host;
            }
        }
        vt.set_default_color_palette(Some(Palette(colors)))?;
    }
    if let Some(light) = theme.is_light() {
        let scheme = if light { ColorScheme::Light } else { ColorScheme::Dark };
        vt.on_color_scheme(move |_| Some(scheme))?;
    }
    Ok(())
}

fn to_color(color: StyleColor) -> Color {
    match color {
        StyleColor::None => Color::Reset,
        StyleColor::Palette(i) => Color::Indexed(i.0),
        StyleColor::Rgb(rgb) => Color::Rgb(rgb.r, rgb.g, rgb.b),
    }
}

fn to_style(s: libghostty_vt::style::Style) -> Style {
    let mut m = Modifier::empty();
    m.set(Modifier::BOLD, s.bold);
    m.set(Modifier::ITALIC, s.italic);
    m.set(Modifier::DIM, s.faint);
    m.set(Modifier::SLOW_BLINK, s.blink);
    m.set(Modifier::REVERSED, s.inverse);
    m.set(Modifier::HIDDEN, s.invisible);
    m.set(Modifier::CROSSED_OUT, s.strikethrough);
    m.set(Modifier::UNDERLINED, s.underline != Underline::None);
    Style::default().fg(to_color(s.fg_color)).bg(to_color(s.bg_color)).add_modifier(m)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use rstest::rstest;

    use super::*;

    fn emulator() -> (Emulator, Rc<RefCell<Vec<u8>>>) {
        themed_emulator(&HostTheme::default())
    }

    fn themed_emulator(theme: &HostTheme) -> (Emulator, Rc<RefCell<Vec<u8>>>) {
        let replies = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&replies);
        let reply = Box::new(move |bytes: &[u8]| sink.borrow_mut().extend_from_slice(bytes));
        (Emulator::new(24, 80, 0, theme, reply).expect("emulator"), replies)
    }

    fn snapshot_of(output: &[u8]) -> Snapshot {
        let (mut emu, _) = emulator();
        emu.feed(output);
        emu.snapshot().expect("snapshot")
    }

    fn replies_to(output: &[u8]) -> String {
        let (mut emu, replies) = emulator();
        emu.feed(output);
        String::from_utf8_lossy(&replies.borrow()).into_owned()
    }

    fn style_of(output: &[u8]) -> Style {
        snapshot_of(output).rows[0][0].style
    }

    mod snapshot {
        use super::*;

        #[test]
        fn has_the_text() {
            assert!(snapshot_of(b"hello\r\nworld").contents().starts_with("hello\nworld"));
        }

        #[test]
        fn has_the_cursor() {
            assert_eq!(snapshot_of(b"hello\r\nworld").cursor, Some(Position::new(5, 1)));
        }

        #[test]
        fn never_holds_control_characters() {
            let snap = snapshot_of(b"x\x7fy");
            let controls: Vec<&str> = snap
                .rows
                .iter()
                .flatten()
                .map(|c| c.symbol.as_str())
                .filter(|s| s.contains(char::is_control))
                .collect();
            assert_eq!(controls, Vec::<&str>::new());
        }

        #[test]
        fn hides_the_cursor_when_asked() {
            assert_eq!(snapshot_of(b"\x1b[?25l").cursor, None);
        }

        #[test]
        fn has_one_row_per_line_of_the_terminal() {
            assert_eq!(snapshot_of(b"").rows.len(), 24);
        }

        #[test]
        fn rewraps_lines_on_resize() {
            let (mut emu, _) = emulator();
            emu.feed(&[b'x'; 30]);
            emu.resize(24, 20).expect("resize");
            assert!(emu.snapshot().expect("snapshot").contents().starts_with(&format!(
                "{}\n{}",
                "x".repeat(20),
                "x".repeat(10)
            )));
        }
    }

    mod clipboard {
        use super::*;

        fn copied_by(output: &[u8]) -> Vec<String> {
            let (mut emu, _) = emulator();
            emu.feed(output);
            emu.take_copied()
        }

        #[test]
        fn an_osc52_write_is_handed_over() {
            assert_eq!(copied_by(b"\x1b]52;c;aGVsbG8=\x07"), ["hello"]);
        }

        #[rstest]
        #[case::a_read_request(b"\x1b]52;c;?\x07")]
        #[case::the_primary_selection(b"\x1b]52;p;aGVsbG8=\x07")]
        #[case::an_empty_write(b"\x1b]52;c;\x07")]
        #[case::plain_output(b"hello")]
        fn hands_over_nothing_for(#[case] output: &[u8]) {
            assert_eq!(copied_by(output), Vec::<String>::new());
        }

        #[test]
        fn taking_empties_the_queue() {
            let (mut emu, _) = emulator();
            emu.feed(b"\x1b]52;c;aGVsbG8=\x07");
            emu.take_copied();
            assert_eq!(emu.take_copied(), Vec::<String>::new());
        }
    }

    mod title {
        use super::*;

        fn title_after(output: &[u8]) -> String {
            let (mut emu, _) = emulator();
            emu.feed(output);
            emu.title()
        }

        #[rstest]
        #[case::osc_0("\x1b]0;✳ Claude Code\x07", "✳ Claude Code")]
        #[case::osc_2("\x1b]2;◐ fix the login\x1b\\", "◐ fix the login")]
        #[case::the_latest("\x1b]0;one\x07\x1b]0;two\x07", "two")]
        #[case::none_set("hello", "")]
        fn is_what_the_program_set(#[case] output: &str, #[case] expected: &str) {
            assert_eq!(title_after(output.as_bytes()), expected);
        }
    }

    mod selection {
        use super::*;

        fn small(rows: u16, cols: u16) -> Emulator {
            Emulator::new(rows, cols, 100, &HostTheme::default(), Box::new(|_| {})).expect("emulator")
        }

        fn select(emu: &mut Emulator, from: (u16, u16), to: (u16, u16)) {
            emu.start_selection(Position::new(from.0, from.1)).expect("start");
            emu.extend_selection(Position::new(to.0, to.1)).expect("extend");
        }

        fn copied(output: &[u8], from: (u16, u16), to: (u16, u16)) -> Option<String> {
            let (mut emu, _) = emulator();
            emu.feed(output);
            select(&mut emu, from, to);
            emu.finish_selection().expect("finish")
        }

        #[rstest]
        #[case::forwards(b"hello world", (0, 0), (4, 0), "hello")]
        #[case::backwards(b"hello world", (10, 0), (6, 0), "world")]
        #[case::across_lines(b"ab\r\ncd", (0, 0), (1, 1), "ab\ncd")]
        #[case::without_trailing_spaces(b"ab   \r\ncd", (0, 0), (9, 0), "ab")]
        fn copies_the_text_between_both_points(
            #[case] output: &[u8],
            #[case] from: (u16, u16),
            #[case] to: (u16, u16),
            #[case] expected: &str,
        ) {
            assert_eq!(copied(output, from, to).as_deref(), Some(expected));
        }

        #[test]
        fn joins_a_line_the_terminal_wrapped() {
            let mut emu = small(4, 10);
            emu.feed(b"0123456789abcde");
            select(&mut emu, (0, 0), (4, 1));
            assert_eq!(emu.finish_selection().expect("finish").as_deref(), Some("0123456789abcde"));
        }

        #[test]
        fn keeps_its_start_on_the_text_when_the_screen_scrolls() {
            let mut emu = small(3, 10);
            emu.feed(b"a\r\nb\r\nc");
            emu.start_selection(Position::new(0, 2)).expect("start");
            emu.feed(b"\r\nd");
            emu.extend_selection(Position::new(0, 2)).expect("extend");
            assert_eq!(emu.finish_selection().expect("finish").as_deref(), Some("c\nd"));
        }

        #[test]
        fn a_click_without_dragging_copies_nothing() {
            let (mut emu, _) = emulator();
            emu.feed(b"hello");
            emu.start_selection(Position::new(1, 0)).expect("start");
            assert_eq!(emu.finish_selection().expect("finish"), None);
        }

        #[test]
        fn marks_the_selected_cells_reversed() {
            let (mut emu, _) = emulator();
            emu.feed(b"hello world");
            select(&mut emu, (0, 0), (4, 0));
            let row = &emu.snapshot().expect("snapshot").rows[0];
            assert!(row[4].style.add_modifier.contains(Modifier::REVERSED));
            assert!(!row[5].style.add_modifier.contains(Modifier::REVERSED));
        }

        #[test]
        fn reversed_text_turns_back_to_normal_when_selected() {
            let (mut emu, _) = emulator();
            emu.feed(b"\x1b[7mhi");
            select(&mut emu, (0, 0), (1, 0));
            let style = emu.snapshot().expect("snapshot").rows[0][0].style;
            assert!(!style.add_modifier.contains(Modifier::REVERSED));
        }

        #[test]
        fn finishing_clears_the_highlight() {
            let (mut emu, _) = emulator();
            emu.feed(b"hello");
            select(&mut emu, (0, 0), (4, 0));
            emu.finish_selection().expect("finish");
            let row = &emu.snapshot().expect("snapshot").rows[0];
            assert!(row.iter().all(|c| !c.style.add_modifier.contains(Modifier::REVERSED)));
        }
    }

    mod text {
        use super::*;

        fn after(rows: u16, cols: u16, output: &[u8]) -> Emulator {
            let mut emu = Emulator::new(rows, cols, 100, &HostTheme::default(), Box::new(|_| {})).expect("emulator");
            emu.feed(output);
            emu
        }

        #[test]
        fn the_screen_joins_lines_the_terminal_wrapped() {
            let emu = after(4, 10, b"0123456789abcde\r\nnext");

            assert_eq!(emu.screen_text().expect("text"), "0123456789abcde\nnext");
        }

        #[test]
        fn empty_lines_at_the_end_are_left_out() {
            let emu = after(6, 20, b"hello   \r\n\r\nworld\r\n\r\n");

            assert_eq!(emu.screen_text().expect("text"), "hello\n\nworld");
        }

        #[test]
        fn the_screen_is_only_what_shows() {
            let emu = after(3, 10, b"one\r\ntwo\r\nthree\r\nfour\r\nfive");

            assert_eq!(emu.screen_text().expect("text"), "three\nfour\nfive");
        }

        #[test]
        fn the_last_lines_reach_into_the_scrollback() {
            let emu = after(3, 10, b"one\r\ntwo\r\nthree\r\nfour\r\nfive");

            assert_eq!(emu.last_lines(4).expect("text"), "two\nthree\nfour\nfive");
        }

        #[test]
        fn more_lines_than_there_are_give_them_all() {
            let emu = after(3, 10, b"one\r\ntwo");

            assert_eq!(emu.last_lines(50).expect("text"), "one\ntwo");
        }
    }

    mod style {
        use super::*;

        #[rstest]
        #[case::palette_stays_indexed(b"\x1b[31mx", Style::default().fg(Color::Indexed(1)).bg(Color::Reset))]
        #[case::extended_palette(b"\x1b[38;5;208mx", Style::default().fg(Color::Indexed(208)).bg(Color::Reset))]
        #[case::rgb(b"\x1b[38;2;10;200;30mx", Style::default().fg(Color::Rgb(10, 200, 30)).bg(Color::Reset))]
        #[case::background(b"\x1b[44mx", Style::default().fg(Color::Reset).bg(Color::Indexed(4)))]
        #[case::bold(b"\x1b[1mx", Style::default().fg(Color::Reset).bg(Color::Reset).add_modifier(Modifier::BOLD))]
        #[case::plain(b"x", Style::default())]
        fn maps_to_ratatui(#[case] output: &[u8], #[case] expected: Style) {
            assert_eq!(style_of(output), expected);
        }
    }

    mod replies {
        use super::*;

        #[rstest]
        #[case::cursor_position(b"\x1b[3;5H\x1b[6n", "\x1b[3;5R")]
        #[case::status(b"\x1b[5n", "\x1b[0n")]
        #[case::plain_output(b"hello", "")]
        fn answers_terminal_queries(#[case] output: &[u8], #[case] expected: &str) {
            assert_eq!(replies_to(output), expected);
        }

        #[test]
        fn answers_device_attributes() {
            assert!(replies_to(b"\x1b[c").starts_with("\x1b[?"));
        }
    }

    mod host_theme {
        use libghostty_vt::style::RgbColor;

        use super::*;

        const DARK: RgbColor = RgbColor { r: 0x12, g: 0x34, b: 0x56 };
        const LIGHT: RgbColor = RgbColor { r: 0xfd, g: 0xf6, b: 0xe3 };
        const RED: RgbColor = RgbColor { r: 0xcc, g: 0x11, b: 0x22 };

        fn themed_replies_to(theme: &HostTheme, output: &[u8]) -> String {
            let (mut emu, replies) = themed_emulator(theme);
            emu.feed(output);
            String::from_utf8_lossy(&replies.borrow()).into_owned()
        }

        fn with_background(background: RgbColor) -> HostTheme {
            HostTheme { background: Some(background), ..HostTheme::default() }
        }

        #[test]
        fn background_query_gets_the_host_background() {
            assert!(themed_replies_to(&with_background(DARK), b"\x1b]11;?\x07").contains("rgb:1212/3434/5656"));
        }

        #[test]
        fn foreground_query_gets_the_host_foreground() {
            let theme = HostTheme { foreground: Some(LIGHT), ..HostTheme::default() };
            assert!(themed_replies_to(&theme, b"\x1b]10;?\x07").contains("rgb:fdfd/f6f6/e3e3"));
        }

        #[test]
        fn palette_query_gets_the_host_palette() {
            let mut theme = HostTheme::default();
            theme.palette[1] = Some(RED);
            assert!(themed_replies_to(&theme, b"\x1b]4;1;?\x07").contains("rgb:cccc/1111/2222"));
        }

        #[test]
        fn background_query_is_unanswered_without_host_colors() {
            assert_eq!(replies_to(b"\x1b]11;?\x07"), "");
        }

        #[rstest]
        #[case::dark(DARK, "\x1b[?997;1n")]
        #[case::light(LIGHT, "\x1b[?997;2n")]
        fn color_scheme_query_follows_the_host_background(#[case] background: RgbColor, #[case] expected: &str) {
            assert_eq!(themed_replies_to(&with_background(background), b"\x1b[?996n"), expected);
        }

        #[test]
        fn palette_colors_still_render_as_indices() {
            let mut theme = HostTheme::default();
            theme.palette[1] = Some(RED);
            let (mut emu, _) = themed_emulator(&theme);
            emu.feed(b"\x1b[31mx");
            assert_eq!(emu.snapshot().expect("snapshot").rows[0][0].style.fg, Some(Color::Indexed(1)));
        }
    }

    mod modes {
        use super::*;

        fn after(output: &[u8]) -> Emulator {
            let (mut emu, _) = emulator();
            emu.feed(output);
            emu
        }

        #[rstest]
        #[case::none(b"", MouseMode::None)]
        #[case::x10(b"\x1b[?9h", MouseMode::Press)]
        #[case::normal(b"\x1b[?1000h", MouseMode::PressRelease)]
        #[case::button(b"\x1b[?1002h", MouseMode::ButtonMotion)]
        #[case::any(b"\x1b[?1003h", MouseMode::AnyMotion)]
        fn mouse_mode_follows_the_program(#[case] output: &[u8], #[case] expected: MouseMode) {
            assert_eq!(after(output).mouse_mode(), expected);
        }

        #[rstest]
        #[case::default(b"", MouseEncoding::Default)]
        #[case::utf8(b"\x1b[?1005h", MouseEncoding::Utf8)]
        #[case::sgr(b"\x1b[?1006h", MouseEncoding::Sgr)]
        fn mouse_encoding_follows_the_program(#[case] output: &[u8], #[case] expected: MouseEncoding) {
            assert_eq!(after(output).mouse_encoding(), expected);
        }

        #[rstest]
        #[case::off(b"", false)]
        #[case::on(b"\x1b[?1h", true)]
        fn application_cursor_follows_the_program(#[case] output: &[u8], #[case] expected: bool) {
            assert_eq!(after(output).application_cursor(), expected);
        }

        #[rstest]
        #[case::off(b"", false)]
        #[case::on(b"\x1b[?2004h", true)]
        fn bracketed_paste_follows_the_program(#[case] output: &[u8], #[case] expected: bool) {
            assert_eq!(after(output).bracketed_paste(), expected);
        }
    }

    mod scrollback {
        use super::*;

        fn numbered(lines: usize) -> Emulator {
            let mut emu = Emulator::new(4, 20, 100, &HostTheme::default(), Box::new(|_| {})).expect("emulator");
            let text: Vec<String> = (1..=lines).map(|i| format!("line-{i}")).collect();
            emu.feed(text.join("\r\n").as_bytes());
            emu
        }

        fn shown(emu: &mut Emulator) -> String {
            emu.snapshot().expect("snapshot").contents()
        }

        #[test]
        fn at_the_bottom_nothing_is_below() {
            let mut emu = numbered(10);

            assert_eq!((emu.below(), shown(&mut emu)), (0, "line-7\nline-8\nline-9\nline-10".to_string()));
        }

        #[test]
        fn scrolling_up_shows_earlier_lines() {
            let mut emu = numbered(10);

            emu.scroll(-3);

            assert_eq!(shown(&mut emu), "line-4\nline-5\nline-6\nline-7");
        }

        #[test]
        fn the_snapshot_says_how_many_rows_are_below_the_view() {
            let mut emu = numbered(10);

            emu.scroll(-3);

            assert_eq!((emu.below(), emu.snapshot().expect("snapshot").below), (3, 3));
        }

        #[test]
        fn scrolling_past_the_top_stops_at_the_first_line() {
            let mut emu = numbered(10);

            emu.scroll(-100);

            assert_eq!(shown(&mut emu), "line-1\nline-2\nline-3\nline-4");
        }

        #[test]
        fn new_output_keeps_the_view_and_grows_below() {
            let mut emu = numbered(10);
            emu.scroll(-3);

            emu.feed(b"\r\nline-11\r\nline-12");

            assert_eq!((shown(&mut emu), emu.below()), ("line-4\nline-5\nline-6\nline-7".to_string(), 5));
        }

        #[test]
        fn scrolling_down_to_the_end_leaves_nothing_below() {
            let mut emu = numbered(10);
            emu.scroll(-3);

            emu.scroll(3);

            assert_eq!((emu.below(), shown(&mut emu)), (0, "line-7\nline-8\nline-9\nline-10".to_string()));
        }

        #[test]
        fn scrolling_to_the_bottom_shows_the_last_lines_again() {
            let mut emu = numbered(10);
            emu.scroll(-3);

            emu.scroll_to_bottom();

            assert_eq!((emu.below(), shown(&mut emu)), (0, "line-7\nline-8\nline-9\nline-10".to_string()));
        }

        #[test]
        fn launches_read_the_active_screen_even_when_scrolled() {
            let mut emu = numbered(10);
            emu.scroll(-3);

            let active = emu.active_contents().expect("text");

            assert_eq!(active, "line-7\nline-8\nline-9\nline-10");
            assert_eq!(shown(&mut emu), "line-4\nline-5\nline-6\nline-7", "the view stays where it was");
        }

        #[test]
        fn a_program_that_turns_on_the_mouse_brings_the_view_back_down() {
            let mut emu = numbered(10);
            emu.scroll(-3);

            emu.feed(b"\x1b[?1000h");

            let snap = emu.snapshot().expect("snapshot");
            assert_eq!((snap.below, snap.contents()), (0, "line-7\nline-8\nline-9\nline-10".to_string()));
        }

        #[test]
        fn the_alternate_screen_has_nothing_below() {
            let mut emu = numbered(10);
            emu.scroll(-3);

            emu.feed(b"\x1b[?1049h");

            assert_eq!((emu.alternate_screen(), emu.snapshot().expect("snapshot").below), (true, 0));
        }

        #[rstest]
        #[case::on_by_default(b"", true)]
        #[case::turned_off(b"\x1b[?1007l", false)]
        fn alternate_scroll_follows_the_program(#[case] output: &[u8], #[case] expected: bool) {
            let mut emu = numbered(1);
            emu.feed(output);

            assert_eq!(emu.alt_scroll(), expected);
        }
    }
}
