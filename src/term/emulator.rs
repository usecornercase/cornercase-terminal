use std::cell::{Cell as Counter, RefCell};
use std::ops::Range;
use std::rc::Rc;

use crossterm::event::KeyEvent;
use libghostty_vt::Terminal;
use libghostty_vt::fmt::{Format, Formatter, FormatterOptions};
use libghostty_vt::key::{Encoder, KittyKeyFlags};
use libghostty_vt::render::CursorVisualStyle;
use libghostty_vt::render::{CellIterator, RenderState, RowIterator};
use libghostty_vt::screen::{CellWide, Screen, TrackedGridRef};
use libghostty_vt::selection::{FormatOptions, Selection};
use libghostty_vt::style::{Palette, StyleColor, Underline};
use libghostty_vt::terminal::{
    ClipboardLocation, ClipboardWrite, ColorScheme, Mode, Options, Point, PointCoordinate, ScrollViewport,
};
use ratatui::layout::Position;
use ratatui::style::{Color, Modifier, Style};
use unicode_width::UnicodeWidthChar;

use crate::agents::activity::Activity;
use crate::host::theme::HostTheme;
use crate::term::keys;
use crate::term::mouse::{MouseEncoding, MouseMode};
use crate::term::status;
use crate::text::Needle;

pub(crate) type Result<T> = libghostty_vt::error::Result<T>;
pub(crate) type ReplyFn = Box<dyn FnMut(&[u8])>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Cell {
    pub(crate) symbol: String,
    pub(crate) style: Style,
    pub(crate) spacer: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum CursorShape {
    #[default]
    Block,
    Bar,
    Underline,
    BlockHollow,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Scrollbar {
    pub(crate) total: u64,
    pub(crate) offset: u64,
    pub(crate) rows: u64,
}

impl Scrollbar {
    fn below(self) -> usize {
        usize::try_from(self.total.saturating_sub(self.offset.saturating_add(self.rows))).unwrap_or(usize::MAX)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub(crate) rows: Vec<Vec<Cell>>,
    pub(crate) cursor: Option<Position>,
    pub(crate) shape: CursorShape,
    pub(crate) blinking: bool,
    pub(crate) scroll: Scrollbar,
    pub(crate) below: usize,
}

impl Snapshot {
    pub(crate) fn contents(&self) -> String {
        self.rows
            .iter()
            .map(|row| row.iter().map(|c| c.symbol.as_str()).collect::<String>().trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub(crate) struct Emulator {
    vt: Terminal<'static, 'static>,
    render: RenderState<'static>,
    rows: RowIterator<'static>,
    cells: CellIterator<'static>,
    keys: Encoder<'static>,
    anchor: Option<TrackedGridRef>,
    copied: Rc<RefCell<Vec<String>>>,
    rung: Rc<Counter<u32>>,
    status: status::Status,
    reply: Rc<RefCell<ReplyFn>>,
}

impl Emulator {
    pub(crate) fn new(rows: u16, cols: u16, scrollback: usize, theme: &HostTheme, reply: ReplyFn) -> Result<Self> {
        let mut vt = Terminal::new(Options { cols, rows, max_scrollback: scrollback })?;
        let reply = Rc::new(RefCell::new(reply));
        let sink = Rc::clone(&reply);
        vt.on_pty_write(move |_, bytes| sink.borrow_mut()(bytes))?;
        let copied = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&copied);
        vt.on_clipboard_write(move |_, write| {
            if let Some(text) = clipboard_text(&write) {
                sink.borrow_mut().push(text);
            }
            Ok(())
        })?;
        let rung = Rc::new(Counter::new(0_u32));
        let bells = Rc::clone(&rung);
        vt.on_bell(move |_| bells.set(bells.get().saturating_add(1)))?;
        apply_theme(&mut vt, theme)?;
        Ok(Self {
            vt,
            render: RenderState::new()?,
            rows: RowIterator::new()?,
            cells: CellIterator::new()?,
            keys: Encoder::new()?,
            anchor: None,
            copied,
            rung,
            status: status::Status::default(),
            reply,
        })
    }

    pub(crate) fn set_theme(&mut self, theme: &HostTheme) -> Result<()> {
        apply_theme(&mut self.vt, theme)
    }

    #[cfg(test)]
    pub(crate) fn default_background(&self) -> Option<libghostty_vt::style::RgbColor> {
        self.vt.default_bg_color().ok().flatten()
    }

    pub(crate) fn take_copied(&mut self) -> Vec<String> {
        std::mem::take(&mut self.copied.borrow_mut())
    }

    pub(crate) fn take_bells(&mut self) -> u32 {
        self.rung.replace(0)
    }

    pub(crate) fn encode_key(&mut self, key: KeyEvent) -> Vec<u8> {
        self.keys.set_options_from_terminal(&self.vt);
        let kitty = self.vt.kitty_keyboard_flags().unwrap_or(KittyKeyFlags::DISABLED);
        let app_cursor = self.application_cursor();
        keys::encode(key, &mut self.keys, kitty, app_cursor)
    }

    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        let mut start = 0;
        for (at, &byte) in bytes.iter().enumerate() {
            if self.status.advance(byte) {
                self.vt.vt_write(&bytes[start..=at]);
                self.reply.borrow_mut()(status::REPLY);
                start = at + 1;
            }
        }
        self.vt.vt_write(&bytes[start..]);
    }

    pub(crate) fn status_process(&mut self, pid: Option<i32>) {
        self.status.process(pid);
    }

    pub(crate) fn program_activity(&self) -> Option<Activity> {
        self.status.activity()
    }

    pub(crate) fn finished_activity(&self, pid: i32) -> Option<Activity> {
        self.status.finished(pid)
    }

    pub(crate) fn acknowledge_status(&mut self, pid: i32) {
        self.status.acknowledge(pid);
    }

    pub(crate) fn resize(&mut self, rows: u16, cols: u16) -> Result<()> {
        self.vt.resize(cols, rows, 0, 0)
    }

    pub(crate) fn size(&self) -> Result<(u16, u16)> {
        Ok((self.vt.rows()?, self.vt.cols()?))
    }

    fn mode(&self, mode: Mode) -> bool {
        self.vt.mode(mode).unwrap_or(false)
    }

    pub(crate) fn application_cursor(&self) -> bool {
        self.mode(Mode::DECCKM)
    }

    pub(crate) fn title(&self) -> String {
        self.vt.title().map(str::to_string).unwrap_or_default()
    }

    pub(crate) fn bracketed_paste(&self) -> bool {
        self.mode(Mode::BRACKETED_PASTE)
    }

    pub(crate) fn alternate_screen(&self) -> bool {
        matches!(self.vt.active_screen(), Ok(Screen::Alternate))
    }

    pub(crate) fn alt_scroll(&self) -> bool {
        self.mode(Mode::ALT_SCROLL)
    }

    pub(crate) fn scroll(&mut self, rows: isize) {
        self.vt.scroll_viewport(ScrollViewport::Delta(rows));
    }

    pub(crate) fn scroll_to_bottom(&mut self) {
        self.vt.scroll_viewport(ScrollViewport::Bottom);
    }

    pub(crate) fn scroll_to_top(&mut self) {
        self.vt.scroll_viewport(ScrollViewport::Top);
    }

    pub(crate) fn scrollbar(&self) -> Scrollbar {
        self.vt.scrollbar().map_or_else(
            |_| Scrollbar::default(),
            |bar| Scrollbar { total: bar.total, offset: bar.offset, rows: bar.len },
        )
    }

    pub(crate) fn below(&self) -> usize {
        self.scrollbar().below()
    }

    pub(crate) fn focus_events(&self) -> bool {
        self.mode(Mode::FOCUS_EVENT)
    }

    pub(crate) fn kitty_keyboard(&self) -> u8 {
        self.vt.kitty_keyboard_flags().map_or(0, |flags| flags.bits())
    }

    pub(crate) fn active_contents(&mut self) -> Result<String> {
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

    pub(crate) fn mouse_mode(&self) -> MouseMode {
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

    pub(crate) fn mouse_encoding(&self) -> MouseEncoding {
        if self.mode(Mode::SGR_MOUSE) {
            MouseEncoding::Sgr
        } else if self.mode(Mode::UTF8_MOUSE) {
            MouseEncoding::Utf8
        } else {
            MouseEncoding::Default
        }
    }

    pub(crate) fn start_selection(&mut self, at: Position) -> Result<()> {
        self.vt.set_selection(None)?;
        self.anchor = Some(self.vt.track_grid_ref(viewport(at))?);
        Ok(())
    }

    pub(crate) fn extend_selection(&mut self, to: Position) -> Result<()> {
        let Some(anchor) = &self.anchor else { return Ok(()) };
        let Some(start) = anchor.snapshot(&self.vt)? else { return Ok(()) };
        let end = self.vt.grid_ref(viewport(to))?;
        self.vt.set_selection(Some(&Selection::new(start, end, false)))?;
        Ok(())
    }

    pub(crate) fn finish_selection(&mut self) -> Result<Option<String>> {
        self.anchor = None;
        let text = self.selection_text()?;
        self.vt.set_selection(None)?;
        Ok(text)
    }

    pub(crate) fn select(&mut self, from: Position, to: Position, rectangle: bool) -> Result<Option<String>> {
        self.anchor = None;
        let selection = Selection::new(self.vt.grid_ref(viewport(from))?, self.vt.grid_ref(viewport(to))?, rectangle);
        self.vt.set_selection(Some(&selection))?;
        self.selection_text()
    }

    pub(crate) fn selection_text(&self) -> Result<Option<String>> {
        let text = self.vt.format_selection_alloc(None, plain())?.map(|b| String::from_utf8_lossy(&b).into_owned());
        Ok(text.filter(|t| !t.is_empty()))
    }

    pub(crate) fn clear_selection(&mut self) -> Result<()> {
        self.anchor = None;
        self.vt.set_selection(None)?;
        Ok(())
    }

    pub(crate) fn screen_text(&self) -> Result<String> {
        let (rows, cols) = self.size()?;
        let at = |x, y| self.vt.grid_ref(Point::Active(PointCoordinate { x, y }));
        let selection =
            Selection::new(at(0, 0)?, at(cols.saturating_sub(1), u32::from(rows.saturating_sub(1)))?, false);
        let text = self.vt.format_selection_alloc(None, plain().with_selection(&selection))?;
        Ok(tail(&text.map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default(), None))
    }

    pub(crate) fn text(&self) -> Result<String> {
        let options = FormatterOptions::new().with_format(Format::Plain).with_unwrap(true).with_trim(true);
        let text = Formatter::new(&self.vt, options)?.format_alloc(None)?;
        Ok(String::from_utf8_lossy(&text).into_owned())
    }

    pub(crate) fn last_lines(&self, lines: usize) -> Result<String> {
        Ok(tail(&self.text()?, Some(lines)))
    }

    pub(crate) fn reveal(&mut self, needle: &Needle, near: Option<usize>) -> Result<bool> {
        let text = self.text()?;
        let hits = text.lines().enumerate().filter_map(|(i, line)| Some((i, line, needle.find(line)?)));
        let pick = |(i, ..): &(usize, &str, Range<usize>)| near.map_or(usize::MAX - i, |near| i.abs_diff(near));
        let Some((line, text, found)) = hits.min_by_key(pick) else { return Ok(false) };
        let (_, cols) = self.size()?;
        let Some(first) = self.line_row(line)? else { return Ok(false) };
        let last = text[found.clone()].chars().next_back().map_or(found.start, |c| found.end - c.len_utf8());
        let (from, to) = (locate(text, found.start, cols), locate(text, last, cols));
        let point = |(row, x): (usize, u16)| {
            Point::Screen(PointCoordinate { x, y: u32::try_from(first + row).unwrap_or(u32::MAX) })
        };
        let selection = Selection::new(self.vt.grid_ref(point(from))?, self.vt.grid_ref(point(to))?, false);
        self.anchor = None;
        self.vt.set_selection(Some(&selection))?;
        let selected = self.vt.format_selection_alloc(None, plain())?.map(|b| String::from_utf8_lossy(&b).into_owned());
        if selected.is_none_or(|selected| needle.find(&selected).is_none()) {
            self.vt.set_selection(None)?;
            return Ok(false);
        }
        let (rows, _) = self.size()?;
        self.vt.scroll_viewport(ScrollViewport::Row(first.saturating_sub(usize::from(rows) / 3)));
        Ok(true)
    }

    pub(crate) fn line_row(&self, line: usize) -> Result<Option<usize>> {
        let (mut number, mut first) = (0usize, None);
        for y in 0..self.vt.total_rows()? {
            let at = Point::Screen(PointCoordinate { x: 0, y: u32::try_from(y).unwrap_or(u32::MAX) });
            if self.vt.grid_ref(at)?.row()?.is_wrap_continuation()? {
                continue;
            }
            if first.is_some() {
                break;
            }
            if number == line {
                first = Some(y);
            }
            number += 1;
        }
        Ok(first)
    }

    pub(crate) fn snapshot(&mut self) -> Result<Snapshot> {
        if self.below() > 0 && self.mouse_mode() != MouseMode::None {
            self.scroll_to_bottom();
        }
        let snap = self.render.update(&self.vt)?;
        let cursor =
            if snap.cursor_visible()? { snap.cursor_viewport()?.map(|vp| Position::new(vp.x, vp.y)) } else { None };
        let shape = match snap.cursor_visual_style() {
            Ok(CursorVisualStyle::Bar) => CursorShape::Bar,
            Ok(CursorVisualStyle::Underline) => CursorShape::Underline,
            Ok(CursorVisualStyle::BlockHollow) => CursorShape::BlockHollow,
            _ => CursorShape::Block,
        };
        let blinking = snap.cursor_blinking().unwrap_or(false);
        let mut rows = Vec::new();
        let mut text = String::new();
        let mut row_it = self.rows.update(&snap)?;
        while let Some(row) = row_it.next() {
            let mut cells = Vec::new();
            let mut spacer = false;
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
                let tail = std::mem::take(&mut spacer);
                spacer = cell.graphemes_len()? > 0 && cell.raw_cell()?.wide()? == CellWide::Wide;
                cells.push(Cell { symbol, style, spacer: tail });
            }
            rows.push(cells);
        }
        let scroll = self.scrollbar();
        Ok(Snapshot { rows, cursor, shape, blinking, scroll, below: scroll.below() })
    }
}

fn plain<'t, 's>() -> FormatOptions<'t, 's> {
    FormatOptions::new().with_emit_format(Format::Plain).with_unwrap(true).with_trim(true)
}

fn locate(text: &str, byte: usize, cols: u16) -> (usize, u16) {
    let cols = usize::from(cols.max(1));
    let (mut row, mut x) = (0usize, 0usize);
    let mut chars = text.char_indices();
    for (at, c) in chars.by_ref() {
        let width = c.width().unwrap_or(0);
        if x + width > cols {
            row += 1;
            x = 0;
        }
        if at >= byte {
            return (row, u16::try_from(x).unwrap_or(u16::MAX));
        }
        x += width;
    }
    (row, u16::try_from(x).unwrap_or(u16::MAX))
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
mod tests;
