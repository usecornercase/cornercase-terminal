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

use crate::host::theme::HostTheme;
use crate::term::keys;
use crate::term::mouse::{MouseEncoding, MouseMode};

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
mod tests;
