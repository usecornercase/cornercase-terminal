use std::io::{self, Write};

use ratatui::backend::{Backend, ClearType, CrosstermBackend, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Rect, Size};

use super::Outbox;
use crate::protocol::ServerMessage;

pub(super) struct FrameWriter {
    pub(super) buf: Vec<u8>,
    pub(super) out: Outbox,
}

impl Write for FrameWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.buf.is_empty() {
            self.out.send(ServerMessage::Frame(std::mem::take(&mut self.buf)));
        }
        Ok(())
    }
}

pub(super) struct CropBackend {
    pub(super) inner: CrosstermBackend<FrameWriter>,
    pub(super) area: Rect,
    pub(super) visible: Rect,
    pub(super) watched: Option<Rect>,
    pub(super) damaged: bool,
}

impl Backend for CropBackend {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let (visible, watched) = (self.visible, self.watched);
        let mut damaged = false;
        let shown = content.filter(|&(x, y, _)| visible.contains(Position::new(x, y)));
        let drawn = self.inner.draw(shown.inspect(|&(x, y, _)| {
            damaged |= watched.is_some_and(|r| r.contains(Position::new(x, y)));
        }));
        self.damaged |= damaged;
        drawn
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        let position = position.into();
        if !self.visible.contains(position) {
            return self.inner.hide_cursor();
        }
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> io::Result<()> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> io::Result<Size> {
        Ok(self.area.as_size())
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        Ok(WindowSize { columns_rows: self.area.as_size(), pixels: Size::default() })
    }

    fn flush(&mut self) -> io::Result<()> {
        Backend::flush(&mut self.inner)
    }
}
