use std::cell::{Cell as Counter, RefCell};
use std::ops::Range;
use std::rc::Rc;

use crossterm::event::KeyEvent;
use libghostty_vt::Terminal;
use libghostty_vt::error::Error;
use libghostty_vt::fmt::{Format, Formatter, FormatterOptions};
use libghostty_vt::key::{Encoder, KittyKeyFlags};
use libghostty_vt::render::CursorVisualStyle;
use libghostty_vt::render::{CellIteration, CellIterator, RenderState, RowIteration, RowIterator};
use libghostty_vt::screen::{CellContentTag, CellWide, Screen, TrackedGridRef};
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) enum UnderlineStyle {
    #[default]
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) struct Decor {
    pub(crate) underline: UnderlineStyle,
    pub(crate) underline_colour: Color,
    pub(crate) overline: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Cell {
    pub(crate) symbol: String,
    pub(crate) style: Style,
    pub(crate) spacer: bool,
    pub(crate) decor: Decor,
    pub(crate) link: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ProgramColours {
    pub(crate) palette: Vec<(u8, [u8; 3])>,
    pub(crate) foreground: Option<[u8; 3]>,
    pub(crate) background: Option<[u8; 3]>,
    pub(crate) cursor: Option<[u8; 3]>,
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
    pub(crate) fn below(self) -> usize {
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
    pub(crate) links: Vec<String>,
}

impl Snapshot {
    pub(crate) fn lit_lines(&self) -> Vec<(String, String)> {
        let line = |row: &[Cell], lit: bool| {
            let text: String = row
                .iter()
                .map(|c| if lit && c.style.add_modifier.contains(Modifier::DIM) { " " } else { c.symbol.as_str() })
                .collect();
            text.trim_end().to_string()
        };
        self.rows.iter().map(|row| (line(row, false), line(row, true))).collect()
    }

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
    scheme: Option<ColorScheme>,
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
            scheme: scheme_of(theme),
        })
    }

    pub(crate) fn set_theme(&mut self, theme: &HostTheme) -> Result<()> {
        self.vt.set_default_fg_color(None)?;
        self.vt.set_default_bg_color(None)?;
        self.vt.set_default_color_palette(None)?;
        self.vt.on_color_scheme(|_| None)?;
        apply_theme(&mut self.vt, theme)?;
        let scheme = scheme_of(theme);
        if scheme != self.scheme {
            self.scheme = scheme;
            if let Some(scheme) = scheme
                && self.mode(Mode::COLOR_SCHEME_REPORT)
            {
                let mut buf = [0u8; 16];
                let len = scheme.encode_report(&mut buf)?;
                (self.reply.borrow_mut())(&buf[..len]);
            }
        }
        Ok(())
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
        self.active_snapshot().map(|s| s.contents())
    }

    pub(crate) fn active_snapshot(&mut self) -> Result<Snapshot> {
        let scrolled = self.vt.scrollbar().ok().filter(|_| self.below() > 0);
        if scrolled.is_some() {
            self.scroll_to_bottom();
        }
        let snapshot = self.snapshot();
        if let Some(bar) = scrolled {
            self.vt.scroll_viewport(ScrollViewport::Row(usize::try_from(bar.offset).unwrap_or(usize::MAX)));
        }
        snapshot
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
        let Some(hit) = best_hit(&text, needle, near) else { return Ok(false) };
        let Some(first) = self.line_row(hit.0)? else { return Ok(false) };
        if !self.select_hit(hit, first, needle)? {
            return Ok(false);
        }
        let (rows, _) = self.size()?;
        self.vt.scroll_viewport(ScrollViewport::Row(first.saturating_sub(usize::from(rows) / 3)));
        Ok(true)
    }

    fn select_hit(&mut self, (_, text, found): Hit<'_>, first: usize, needle: &Needle) -> Result<bool> {
        let (_, cols) = self.size()?;
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
        self.capture(false)
    }

    pub(crate) fn surface_snapshot(&mut self) -> Result<Snapshot> {
        self.capture(true)
    }

    pub(crate) fn program_colours(&self) -> ProgramColours {
        let (Ok(now), Ok(default)) = (self.vt.color_palette(), self.vt.default_color_palette()) else {
            return ProgramColours::default();
        };
        let rgb = |c: libghostty_vt::style::RgbColor| [c.r, c.g, c.b];
        let palette = (0..=u8::MAX)
            .filter(|&i| now.0[usize::from(i)] != default.0[usize::from(i)])
            .map(|i| (i, rgb(now.0[usize::from(i)])))
            .collect();
        let changed = |now: Result<Option<_>>, default: Result<Option<_>>| {
            let now = now.ok().flatten()?;
            (Some(now) != default.ok().flatten()).then(|| rgb(now))
        };
        ProgramColours {
            palette,
            foreground: changed(self.vt.fg_color(), self.vt.default_fg_color()),
            background: changed(self.vt.bg_color(), self.vt.default_bg_color()),
            cursor: changed(self.vt.cursor_color(), self.vt.default_cursor_color()),
        }
    }

    fn capture(&mut self, links: bool) -> Result<Snapshot> {
        if self.below() > 0 && self.mouse_mode() != MouseMode::None {
            self.scroll_to_bottom();
        }
        let snap = self.render.update(&self.vt)?;
        let cursor =
            if snap.cursor_visible()? { snap.cursor_viewport()?.map(|vp| Position::new(vp.x, vp.y)) } else { None };
        let shape = shape_of(snap.cursor_visual_style());
        let blinking = snap.cursor_blinking().unwrap_or(false);
        let mut capture =
            RowCapture { vt: &self.vt, links, text: String::new(), uri: vec![0u8; 2048], found: Vec::new() };
        let mut rows = Vec::new();
        let mut row_it = self.rows.update(&snap)?;
        let mut y = 0u32;
        while let Some(row) = row_it.next() {
            rows.push(capture.row(&mut self.cells, row, y)?);
            y += 1;
        }
        let links = capture.found;
        let scroll = self.scrollbar();
        Ok(Snapshot { rows, cursor, shape, blinking, scroll, below: scroll.below(), links })
    }
}

type Hit<'a> = (usize, &'a str, Range<usize>);

fn best_hit<'a>(text: &'a str, needle: &Needle, near: Option<usize>) -> Option<Hit<'a>> {
    let hits = text.lines().enumerate().filter_map(|(i, line)| Some((i, line, needle.find(line)?)));
    let pick = |(i, ..): &Hit<'_>| near.map_or(usize::MAX - i, |near| i.abs_diff(near));
    hits.min_by_key(pick)
}

fn shape_of(style: Result<CursorVisualStyle>) -> CursorShape {
    match style {
        Ok(CursorVisualStyle::Bar) => CursorShape::Bar,
        Ok(CursorVisualStyle::Underline) => CursorShape::Underline,
        Ok(CursorVisualStyle::BlockHollow) => CursorShape::BlockHollow,
        _ => CursorShape::Block,
    }
}

struct RowCapture<'a> {
    vt: &'a Terminal<'static, 'static>,
    links: bool,
    text: String,
    uri: Vec<u8>,
    found: Vec<String>,
}

impl RowCapture<'_> {
    fn row(&mut self, cells: &mut CellIterator<'static>, row: &RowIteration<'static, '_>, y: u32) -> Result<Vec<Cell>> {
        let linked = self.links && row.raw_row()?.has_hyperlink()?;
        let mut captured = Vec::new();
        let mut spacer = false;
        let mut cell_it = cells.update(row)?;
        while let Some(cell) = cell_it.next() {
            let at = linked.then(|| (u16::try_from(captured.len()).unwrap_or(u16::MAX), y));
            let (one, wide) = self.cell(cell, at, std::mem::take(&mut spacer))?;
            spacer = wide;
            captured.push(one);
        }
        Ok(captured)
    }

    fn cell(&mut self, cell: &CellIteration<'static, '_>, at: Option<(u16, u32)>, tail: bool) -> Result<(Cell, bool)> {
        self.text.clear();
        if cell.graphemes_len()? > 0 {
            cell.graphemes_utf8(&mut self.text)?;
        }
        self.text.retain(|c| !c.is_control());
        let symbol = if self.text.is_empty() { " ".to_string() } else { self.text.clone() };
        let raw = cell.raw_cell()?;
        let (style, decor) = if cell.has_styling()? {
            let s = cell.style()?;
            (to_style(s), to_decor(s))
        } else {
            (background(raw)?, Decor::default())
        };
        let style = if cell.is_selected()? { selected(style) } else { style };
        let wide = cell.graphemes_len()? > 0 && raw.wide()? == CellWide::Wide;
        let link = match at {
            Some((x, y)) if raw.has_hyperlink()? => link_at(self.vt, x, y, &mut self.uri, &mut self.found)?,
            _ => None,
        };
        Ok((Cell { symbol, style, spacer: tail, decor, link }, wide))
    }
}

fn link_at(
    vt: &Terminal<'static, 'static>,
    x: u16,
    y: u32,
    buf: &mut Vec<u8>,
    found: &mut Vec<String>,
) -> Result<Option<usize>> {
    let at = vt.grid_ref(Point::Viewport(PointCoordinate { x, y }))?;
    let len = match at.hyperlink_uri(buf) {
        Err(Error::OutOfSpace { required }) => {
            buf.resize(required, 0);
            at.hyperlink_uri(buf)?
        }
        other => other?,
    };
    if len == 0 {
        return Ok(None);
    }
    let mut uri = String::from_utf8_lossy(&buf[..len]).into_owned();
    uri.retain(|c| !c.is_control());
    if let Some(i) = found.iter().rposition(|known| *known == uri) {
        return Ok(Some(i));
    }
    found.push(uri);
    Ok(Some(found.len() - 1))
}

fn background(raw: libghostty_vt::screen::Cell) -> Result<Style> {
    Ok(match raw.content_tag()? {
        CellContentTag::BgColorPalette => {
            Style::default().fg(Color::Reset).bg(Color::Indexed(raw.bg_color_palette()?.0))
        }
        CellContentTag::BgColorRgb => {
            let c = raw.bg_color_rgb()?;
            Style::default().fg(Color::Reset).bg(Color::Rgb(c.r, c.g, c.b))
        }
        CellContentTag::Codepoint | CellContentTag::CodepointGrapheme => Style::default(),
    })
}

fn to_decor(s: libghostty_vt::style::Style) -> Decor {
    let underline = match s.underline {
        Underline::None => UnderlineStyle::None,
        Underline::Double => UnderlineStyle::Double,
        Underline::Curly => UnderlineStyle::Curly,
        Underline::Dotted => UnderlineStyle::Dotted,
        Underline::Dashed => UnderlineStyle::Dashed,
        _ => UnderlineStyle::Single,
    };
    Decor { underline, underline_colour: to_color(s.underline_color), overline: s.overline }
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
    if let Some(scheme) = scheme_of(theme) {
        vt.on_color_scheme(move |_| Some(scheme))?;
    }
    Ok(())
}

fn scheme_of(theme: &HostTheme) -> Option<ColorScheme> {
    theme.is_light().map(|light| if light { ColorScheme::Light } else { ColorScheme::Dark })
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
