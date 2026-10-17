use std::time::{Duration, Instant};

use ratatui::layout::Rect;

use super::App;
use super::control::{Handled, json, none};
use crate::control::contract::{Code, Refusal};
use crate::control::surface::{self, Area, Content, Cursor, Frame, Modes, Pane, Scroll, Size, Styles, Surface, lines};
use crate::control::terminal::{Resize, Resized};
use crate::control::{self, Layout};
use crate::log;
use crate::project::Hierarchy;
use crate::term::Term;

pub(crate) const FRAME_EVERY: Duration = Duration::from_millis(16);
pub(super) const REFRESH_EVERY: Duration = Duration::from_millis(500);

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Surfaced {
    Frame(String),
    End,
}

#[derive(Default)]
pub(super) struct Surfaces {
    subscriptions: Vec<Subscription>,
    issued: u64,
}

struct Subscription {
    id: u64,
    client: u64,
    tab: u64,
    area: Rect,
    revision: u64,
    frames: u64,
    dirty: bool,
    next: Instant,
    drawn: Instant,
    last: Content,
}

fn pane_of(term: &mut Term, id: u64, rect: Rect, active: bool, styles: &mut Styles) -> Pane {
    let snapshot = term.emulator.snapshot().unwrap_or_default();
    let (rows, cols) = term.emulator.size().unwrap_or_default();
    let cursor = snapshot.cursor.map(|at| Cursor {
        x: at.x,
        y: at.y,
        shape: snapshot.shape.into(),
        blinking: snapshot.blinking,
    });
    let emulator = &term.emulator;
    let modes = Modes {
        mouse: emulator.mouse_mode().into(),
        mouse_encoding: emulator.mouse_encoding().into(),
        alternate_screen: emulator.alternate_screen(),
        alternate_scroll: emulator.alt_scroll(),
        bracketed_paste: emulator.bracketed_paste(),
        application_cursor: emulator.application_cursor(),
        focus_events: emulator.focus_events(),
        kitty_keyboard: emulator.kitty_keyboard(),
    };
    let scroll = Scroll { total: snapshot.scroll.total, offset: snapshot.scroll.offset, rows: snapshot.scroll.rows };
    Pane {
        pane: id,
        active,
        rect: Area::from(rect),
        size: Size { cols, rows },
        cursor,
        modes,
        scroll,
        lines: lines(snapshot.rows, styles),
    }
}

impl App {
    pub(in crate::app) fn surface_request(&mut self, client: u64, surface: &Surface, now: Instant) -> Handled {
        let Some(tab) = surface.tab else {
            return Err(Refusal::new(Code::Malformed, "`surface` needs the tab to stream: pass --tab"));
        };
        let area = surface.area().map_err(|message| Refusal::new(Code::Malformed, message))?;
        if let Some(own) = self.surfaces.subscriptions.iter().find(|s| s.client == client) {
            let message =
                format!("this connection already streams tab {}; open another connection for another tab", own.tab);
            return Err(Refusal::from(message));
        }
        let content = self.surface_content(tab, area).ok_or_else(|| none("tab", tab))?;
        self.surfaces.issued += 1;
        let id = self.surfaces.issued;
        let frame = self.surface_frame(id, tab, area, 1, content.clone());
        log::info!(
            "control",
            "surface",
            client = client,
            surface = id,
            tab = tab,
            cols = area.width,
            rows = area.height
        );
        self.surfaces.subscriptions.push(Subscription {
            id,
            client,
            tab,
            area,
            revision: 1,
            frames: 1,
            dirty: false,
            next: now + FRAME_EVERY,
            drawn: now,
            last: content,
        });
        Ok(Some(json(&frame)))
    }

    fn surface_content(&mut self, tab: u64, area: Rect) -> Option<Content> {
        let (p, w, t) = self.projects.tab_index(tab)?;
        let tab = &mut self.projects[p].workspaces[w].tabs[t];
        let layout = Layout::from(&tab.layout);
        let dividers = tab.layout.dividers(area);
        let active = tab.pane().map(|term| term.id);
        let mut styles = Styles::default();
        let mut panes = Vec::new();
        for (id, rect) in tab.shown(area) {
            if let Some(term) = tab.pane_by_mut(id) {
                panes.push(pane_of(term, id, rect, active == Some(id), &mut styles));
            }
        }
        let dividers = dividers
            .into_iter()
            .map(|d| surface::Divider { dir: d.dir, path: d.path, area: d.area.into(), line: d.line.into() })
            .collect();
        Some(Content { layout: Some(layout), dividers, styles: styles.into_table(), panes })
    }

    fn surface_frame(&self, surface: u64, tab: u64, area: Rect, revision: u64, content: Content) -> Frame {
        Frame {
            boot: control::server_token().to_string(),
            session: self.events.snapshots.revision(),
            revision,
            surface,
            tab,
            cols: area.width,
            rows: area.height,
            content,
        }
    }

    pub(crate) fn render_surfaces(&mut self, ready: &[u64], now: Instant) -> Vec<(u64, Surfaced)> {
        let mut out = Vec::new();
        let mut at = 0;
        while at < self.surfaces.subscriptions.len() {
            let (id, client, tab, area) = {
                let sub = &self.surfaces.subscriptions[at];
                (sub.id, sub.client, sub.tab, sub.area)
            };
            if self.projects.tab_index(tab).is_none() {
                self.end_surface(at, "closed");
                out.push((client, Surfaced::End));
                continue;
            }
            let sub = &mut self.surfaces.subscriptions[at];
            if now.saturating_duration_since(sub.drawn) >= REFRESH_EVERY {
                sub.dirty = true;
            }
            if !sub.dirty || now < sub.next {
                at += 1;
                continue;
            }
            if !ready.contains(&client) {
                sub.next = now + FRAME_EVERY;
                at += 1;
                continue;
            }
            let Some(built) = crate::panics::contain(|| self.surface_content(tab, area)) else {
                log::error!("control", "surface panicked", client = client, tab = tab);
                self.report_bug();
                self.end_surface(at, "bug");
                out.push((client, Surfaced::End));
                continue;
            };
            let sub = &mut self.surfaces.subscriptions[at];
            sub.drawn = now;
            sub.next = now + FRAME_EVERY;
            sub.dirty = false;
            if let Some(content) = built.filter(|content| *content != sub.last) {
                sub.revision += 1;
                sub.frames += 1;
                let revision = sub.revision;
                sub.last = content.clone();
                let frame = self.surface_frame(id, tab, area, revision, content);
                if let Some(text) = frame.streamed() {
                    log::debug!("control", "surface frame", client = client, revision = revision, bytes = text.len());
                    out.push((client, Surfaced::Frame(text)));
                }
            }
            at += 1;
        }
        out
    }

    pub(in crate::app) fn resize_surface_request(&mut self, resize: &Resize, now: Instant) -> Handled {
        let Some(id) = resize.surface else {
            return Err(Refusal::new(Code::Malformed, "`resize` needs the surface to lay out again: pass --surface"));
        };
        let area = Surface { tab: None, cols: resize.cols, rows: resize.rows }
            .area()
            .map_err(|message| Refusal::new(Code::Malformed, message))?;
        let Some(sub) = self.surfaces.subscriptions.iter_mut().find(|sub| sub.id == id) else {
            return Err(Refusal::new(
                Code::NotFound,
                format!("there is no surface {id}: its stream ended, or it belongs to an earlier start of the server"),
            ));
        };
        sub.area = area;
        sub.dirty = true;
        sub.next = now;
        sub.last = Content::default();
        log::debug!("control", "surface resized", surface = id, cols = area.width, rows = area.height);
        Ok(Some(json(&Resized { surface: id, tab: sub.tab, cols: area.width, rows: area.height })))
    }

    fn end_surface(&mut self, at: usize, reason: &str) {
        let sub = self.surfaces.subscriptions.remove(at);
        log::info!("control", "surface ended", client = sub.client, reason = reason, frames = sub.frames);
    }

    pub(crate) fn touch_surfaces(&mut self) {
        self.surfaces.subscriptions.iter_mut().for_each(|sub| sub.dirty = true);
    }

    pub(super) fn surface_printed(&mut self, pane: u64) {
        if self.surfaces.subscriptions.is_empty() {
            return;
        }
        let Some((_, tab)) = self.projects.tab_with_pane(pane) else { return };
        let id = tab.id;
        for sub in self.surfaces.subscriptions.iter_mut().filter(|sub| sub.tab == id) {
            if tab.shown(sub.area).iter().any(|(shown, _)| *shown == pane) {
                sub.dirty = true;
            }
        }
    }

    pub(super) fn surface_wake(&self, now: Instant) -> Option<Duration> {
        self.surfaces
            .subscriptions
            .iter()
            .filter(|sub| sub.dirty)
            .map(|sub| sub.next.saturating_duration_since(now))
            .min()
    }

    pub(crate) fn streams_surfaces(&self) -> bool {
        !self.surfaces.subscriptions.is_empty()
    }

    pub(in crate::app) fn forget_surfaces(&mut self, client: u64) {
        if let Some(at) = self.surfaces.subscriptions.iter().position(|sub| sub.client == client) {
            self.end_surface(at, "client left");
        }
    }
}
