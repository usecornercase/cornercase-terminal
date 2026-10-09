use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

use ratatui::buffer::{Buffer, CellDiffOption};
use ratatui::layout::Rect;

use super::{App, AppEvent};
use crate::error::Result;
use crate::files::disk::Body;
use crate::graphics::encode::{self, Key};
use crate::graphics::{Missing, Picture, Protocol, Support, kitty};
use crate::log::{self, Job, Level};
use crate::panics;
use crate::ui::files::{self as panel, Image, MARKER, Screen, View};

const MAX_PAYLOADS: usize = 8;
const MAX_PAYLOAD_BYTES: usize = 64 << 20;
const MAX_JOBS: usize = 2;
const PLACEHOLDER: char = '\u{10EEEE}';
const NOT_PREPARED: &str = "cornercase could not prepare this image for this terminal, see server.log";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sight {
    pub support: Support,
    pub visible: Rect,
    pub background: Option<(u8, u8, u8)>,
    pub lo: u8,
    pub sent: Option<u64>,
}

impl Default for Sight {
    fn default() -> Self {
        Self {
            support: Support::default(),
            visible: Rect::new(0, 0, u16::MAX, u16::MAX),
            background: None,
            lo: 0,
            sent: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    pub key: Key,
    pub rect: Rect,
    pub path: String,
    pub payload: Option<Arc<Vec<u8>>>,
    pub drawn: bool,
}

#[derive(Debug, Default)]
pub struct Payloads {
    ready: VecDeque<(Key, Arc<Vec<u8>>)>,
    failed: VecDeque<Key>,
    running: HashSet<Key>,
}

impl Payloads {
    fn get(&mut self, key: &Key) -> Option<Arc<Vec<u8>>> {
        let at = self.ready.iter().position(|(k, _)| k == key)?;
        let entry = self.ready.remove(at)?;
        let payload = Arc::clone(&entry.1);
        self.ready.push_back(entry);
        Some(payload)
    }

    fn failed(&self, key: &Key) -> bool {
        self.failed.contains(key)
    }

    fn start(&mut self, key: Key) -> bool {
        if self.running.len() >= MAX_JOBS || self.running.contains(&key) {
            return false;
        }
        self.running.insert(key)
    }

    fn answered(&mut self, key: Key, payload: Option<Vec<u8>>) {
        self.running.remove(&key);
        let Some(payload) = payload else {
            self.failed.push_back(key);
            if self.failed.len() > MAX_PAYLOADS {
                self.failed.pop_front();
            }
            return;
        };
        self.ready.retain(|(k, _)| *k != key);
        self.ready.push_back((key, Arc::new(payload)));
        while self.ready.len() > MAX_PAYLOADS || (self.ready.len() > 1 && self.bytes() > MAX_PAYLOAD_BYTES) {
            self.ready.pop_front();
        }
    }

    fn bytes(&self) -> usize {
        self.ready.iter().map(|(_, payload)| payload.len()).sum()
    }
}

fn own(buf: &mut Buffer, rect: Rect, protocol: Protocol) -> bool {
    let mut owned = true;
    for p in rect.intersection(buf.area).positions() {
        let cell = &mut buf[p];
        let ours = match protocol {
            Protocol::Kitty => cell.symbol().starts_with(PLACEHOLDER),
            Protocol::Iterm | Protocol::Sixel { .. } => cell.symbol() == MARKER,
        };
        if ours {
            if protocol != Protocol::Kitty {
                cell.set_diff_option(CellDiffOption::Skip);
            }
        } else {
            owned = false;
            cell.set_diff_option(CellDiffOption::None);
        }
    }
    owned
}

pub fn owned(buf: &mut Buffer, mut placed: Placed) -> Placed {
    if placed.drawn {
        placed.drawn = own(buf, placed.rect, placed.key.protocol) || placed.key.protocol == Protocol::Kitty;
    }
    placed
}

impl App {
    pub fn picture(&self) -> Option<u64> {
        match &self.shown_picture()?.1.body {
            Body::Image(picture) => Some(picture.id),
            _ => None,
        }
    }

    fn shown_picture(&self) -> Option<(String, Arc<crate::files::disk::Content>)> {
        if !self.files_shown() {
            return None;
        }
        let workspace = self.project()?.workspace()?.id;
        let viewer = self.files.viewer(workspace)?;
        Some((viewer.path.clone(), Arc::clone(viewer.content.as_ref()?)))
    }

    fn image_for(&mut self, sight: &Sight, area: Rect, modal: bool) -> Option<(Image, Option<Placed>)> {
        let (path, content) = self.shown_picture()?;
        let Body::Image(picture) = &content.body else { return None };
        let Some(protocol) = sight.support.protocol else {
            let missing = sight.support.missing.clone().unwrap_or(Missing::Unknown { name: None });
            return Some((Image::Message(missing.lines()), None));
        };
        let room = panel::image_room(self.layout(area).shown(self.nav).changes, sight.visible);
        let fit = encode::fit(picture.width, picture.height, sight.support.cell, (room.width, room.height), protocol)?;
        let rect = panel::image_rect(room, fit.cols, fit.rows);
        let kitty_id = if protocol == Protocol::Kitty { kitty::id(sight.support.id_hi, sight.lo) } else { 0 };
        let key = Key {
            picture: picture.id,
            protocol,
            tmux: sight.support.tmux,
            fit,
            background: sight.background,
            kitty_id,
        };
        if self.images.failed(&key) {
            return Some((Image::Message(vec![NOT_PREPARED.into()]), None));
        }
        let payload = self.payload(&key, picture, &path);
        let image = match protocol {
            _ if modal => Image::Blank,
            Protocol::Kitty if payload.is_some() || sight.sent == Some(picture.id) => {
                Image::Placeholders { id: kitty_id, rect }
            }
            Protocol::Iterm | Protocol::Sixel { .. } if payload.is_some() => Image::Markers(rect),
            _ => Image::Blank,
        };
        let drawn = image != Image::Blank;
        Some((image, Some(Placed { key, rect, path, payload, drawn })))
    }

    pub(super) fn files_seen(&mut self, sight: &Sight, area: Rect, modal: bool) -> (Option<View>, Option<Placed>) {
        let (image, placed) = self.image_for(sight, area, modal).unzip();
        let mut files = if self.files_shown() { self.files_view() } else { None };
        if let Some(View { screen: Screen::File(file), .. }) = &mut files {
            file.image = image;
        }
        (files, placed.flatten())
    }

    fn payload(&mut self, key: &Key, picture: &Arc<Picture>, path: &str) -> Option<Arc<Vec<u8>>> {
        if let Some(payload) = self.images.get(key) {
            return Some(payload);
        }
        if self.images.start(*key) {
            let (tx, key, picture) = (self.tx.clone(), *key, Arc::clone(picture));
            let (cols, rows) = (key.fit.cols, key.fit.rows);
            let job = Job::new(Level::Debug, "images", "encode")
                .with("path", path)
                .with("protocol", key.protocol.id())
                .with("cols", cols)
                .with("rows", rows)
                .begin();
            std::thread::spawn(move || {
                let result = panics::job(|| Ok(encode::encode(&picture, &key)));
                let bytes = result.as_ref().map_or(0, Vec::len);
                job.with("bytes", bytes).finish(&result);
                let _ = tx.send(AppEvent::Encoded { key, result });
            });
        }
        None
    }

    pub(super) fn encoded(&mut self, key: Key, result: Result<Vec<u8>>) {
        let payload = match result {
            Ok(payload) if !payload.is_empty() => Some(payload),
            Ok(_) => {
                log::warning!("images", "the encoder made nothing", protocol = key.protocol.id());
                None
            }
            Err(e) => {
                log::warning!("images", "encoding failed", protocol = key.protocol.id(), error = e);
                None
            }
        };
        self.images.answered(key, payload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphics::{CellSize, Tmux};

    fn key(picture: u64) -> Key {
        let fit = encode::Fit { cols: 4, rows: 2, width: 40, height: 40, cell: CellSize { width: 10, height: 20 } };
        Key { picture, protocol: Protocol::Iterm, tmux: Tmux::None, fit, background: None, kitty_id: 0 }
    }

    #[test]
    fn keeps_a_few_payloads_and_drops_the_oldest() {
        let mut payloads = Payloads::default();
        for picture in 0..=u64::try_from(MAX_PAYLOADS).expect("fits") {
            assert!(payloads.start(key(picture)));
            payloads.answered(key(picture), Some(vec![1]));
        }

        assert!(payloads.get(&key(0)).is_none(), "the oldest went first");
        assert!(payloads.get(&key(1)).is_some());
    }

    #[test]
    fn a_payload_used_again_is_kept_longer() {
        let mut payloads = Payloads::default();
        for picture in 0..u64::try_from(MAX_PAYLOADS).expect("fits") {
            payloads.answered(key(picture), Some(vec![1]));
        }
        payloads.get(&key(0));

        payloads.answered(key(99), Some(vec![1]));

        assert!(payloads.get(&key(0)).is_some() && payloads.get(&key(1)).is_none());
    }

    #[test]
    fn big_payloads_push_out_older_ones_but_never_the_last() {
        let mut payloads = Payloads::default();
        payloads.answered(key(1), Some(vec![0; MAX_PAYLOAD_BYTES / 2 + 1]));
        payloads.answered(key(2), Some(vec![0; MAX_PAYLOAD_BYTES / 2 + 1]));
        payloads.answered(key(3), Some(vec![0; MAX_PAYLOAD_BYTES + 1]));

        assert_eq!((payloads.get(&key(2)).is_none(), payloads.get(&key(3)).is_some()), (true, true));
    }

    #[test]
    fn runs_few_jobs_at_once_and_never_the_same_twice() {
        let mut payloads = Payloads::default();

        assert!(payloads.start(key(1)));
        assert!(!payloads.start(key(1)));
        assert!(payloads.start(key(2)));
        assert!(!payloads.start(key(3)), "two jobs at most");
        payloads.answered(key(1), None);
        assert!(payloads.failed(&key(1)) && payloads.start(key(3)));
    }

    #[test]
    fn a_cell_drawn_over_the_markers_is_written_and_the_image_is_not_owned() {
        let rect = Rect::new(1, 1, 3, 2);
        let mut buf = Buffer::empty(Rect::new(0, 0, 6, 4));
        for p in rect.positions() {
            buf[p].set_symbol(MARKER).set_diff_option(CellDiffOption::Skip);
        }
        assert!(own(&mut buf, rect, Protocol::Iterm));

        buf[(2, 1)].set_symbol("m");

        assert!(!own(&mut buf, rect, Protocol::Iterm));
        assert_eq!((buf[(2, 1)].diff_option, buf[(1, 1)].diff_option), (CellDiffOption::None, CellDiffOption::Skip));
    }
}
