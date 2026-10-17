use std::time::{Duration, Instant};

use ratatui::layout::Rect;

use super::App;
use super::control::{Handled, json, none};
use crate::control::contract::{Code, Refusal};
use crate::control::surface::{
    self, Area, Content, Cursor, Frame, Modes, Pane, Scroll, Size, Surface, Tables, lines, patch,
};
use crate::control::terminal::{Redraw, Redrawn, Resize, Resized};
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

enum Place {
    At(u64, Rect, Option<u64>),
    Paused(&'static str),
    Gone,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
enum Wants {
    #[default]
    Patch,
    Complete,
    Redraw,
}

struct Subscription {
    id: u64,
    client: u64,
    view: Option<u64>,
    projection: u64,
    tab: u64,
    area: Rect,
    revision: u64,
    frames: u64,
    dirty: bool,
    next: Instant,
    drawn: Instant,
    last: Content,
    patches: bool,
    wants: Wants,
}

fn pane_of(term: &mut Term, id: u64, rect: Rect, active: bool, tables: &mut Tables) -> Pane {
    let snapshot = term.emulator.surface_snapshot().unwrap_or_default();
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
        colours: emulator.program_colours().into(),
        lines: lines(snapshot.rows, &snapshot.links, tables),
    }
}

impl App {
    pub(in crate::app) fn surface_request(
        &mut self,
        client: u64,
        view: Option<u64>,
        surface: &Surface,
        now: Instant,
    ) -> Handled {
        let (tab, area, projection) = if let Some(view) = view {
            self.view_surface(view, surface)?
        } else {
            let Some(tab) = surface.tab else {
                return Err(Refusal::new(Code::Malformed, "`surface` needs the tab to stream: pass --tab"));
            };
            let area = surface.area().map_err(|message| Refusal::new(Code::Malformed, message))?;
            (tab, area, None)
        };
        if let Some(own) = self.surfaces.subscriptions.iter().find(|s| s.client == client) {
            let message =
                format!("this connection already streams tab {}; open another connection for another tab", own.tab);
            return Err(Refusal::from(message));
        }
        let content = self.surface_content(tab, area).ok_or_else(|| none("tab", tab))?;
        self.surfaces.issued += 1;
        let id = self.surfaces.issued;
        let shown = projection.map(|p| (view.unwrap_or_default(), p));
        let frame = self.surface_frame(id, tab, area, 1, shown, content.clone());
        log::info!(
            "control",
            "surface",
            client = client,
            surface = id,
            tab = tab,
            cols = area.width,
            rows = area.height,
            patches = surface.patches
        );
        self.surfaces.subscriptions.push(Subscription {
            id,
            client,
            view,
            projection: projection.unwrap_or_default(),
            tab,
            area,
            revision: 1,
            frames: 1,
            dirty: false,
            next: now + FRAME_EVERY,
            drawn: now,
            last: content,
            patches: surface.patches,
            wants: Wants::Patch,
        });
        Ok(Some(json(&frame)))
    }

    fn surface_content(&mut self, tab: u64, area: Rect) -> Option<Content> {
        let (p, w, t) = self.projects.tab_index(tab)?;
        let tab = &mut self.projects[p].workspaces[w].tabs[t];
        let layout = Layout::from(&tab.layout);
        let dividers = tab.layout.dividers(area);
        let active = tab.pane().map(|term| term.id);
        let mut tables = Tables::default();
        let mut panes = Vec::new();
        for (id, rect) in tab.shown(area) {
            if let Some(term) = tab.pane_by_mut(id) {
                panes.push(pane_of(term, id, rect, active == Some(id), &mut tables));
            }
        }
        let dividers = dividers
            .into_iter()
            .map(|d| surface::Divider { dir: d.dir, path: d.path, area: d.area.into(), line: d.line.into() })
            .collect();
        Some(Content {
            layout: Some(layout),
            dividers,
            styles: tables.styles.into_entries(),
            hyperlinks: tables.hyperlinks.into_entries(),
            panes,
        })
    }

    fn view_surface(&self, view: u64, surface: &Surface) -> Result<(u64, Rect, Option<u64>), Refusal> {
        if surface.tab.is_some() || surface.cols != 0 || surface.rows != 0 {
            let message = "a view's surface takes the view's tab and size: leave out --tab, --cols and --rows";
            return Err(Refusal::new(Code::Malformed, message));
        }
        match self.sub_place_of(view) {
            Place::At(tab, area, projection) => Ok((tab, area, projection)),
            Place::Paused(why) => Err(Refusal::from(format!("view {view} streams nothing now: {why}"))),
            Place::Gone => Err(none("view", view)),
        }
    }

    fn sub_place_of(&self, view: u64) -> Place {
        let Some(view) = self.views.get(view) else { return Place::Gone };
        match (view.eligible(), view.area()) {
            (Some(tab), Some(area)) => Place::At(tab, area, Some(view.projection)),
            _ => Place::Paused(view.why_not()),
        }
    }

    fn sub_place(&self, sub: &Subscription) -> Place {
        match sub.view {
            Some(view) => self.sub_place_of(view),
            None if self.projects.tab_index(sub.tab).is_some() => Place::At(sub.tab, sub.area, None),
            None => Place::Gone,
        }
    }

    fn surface_frame<B>(
        &self,
        surface: u64,
        tab: u64,
        area: Rect,
        revision: u64,
        view: Option<(u64, u64)>,
        content: B,
    ) -> Frame<B> {
        Frame {
            base: None,
            view: view.map(|(view, _)| view),
            projection: view.map(|(_, projection)| projection),
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
        self.settle_views();
        let mut out = Vec::new();
        let mut at = 0;
        while at < self.surfaces.subscriptions.len() {
            let client = self.surfaces.subscriptions[at].client;
            let (tab, area, projection) = match self.sub_place(&self.surfaces.subscriptions[at]) {
                Place::At(tab, area, projection) => (tab, area, projection),
                Place::Paused(_) => {
                    self.surfaces.subscriptions[at].dirty = false;
                    at += 1;
                    continue;
                }
                Place::Gone => {
                    self.end_surface(at, "closed");
                    out.push((client, Surfaced::End));
                    continue;
                }
            };
            let sub = &mut self.surfaces.subscriptions[at];
            let forced = projection.is_some_and(|p| p != sub.projection);
            if now.saturating_duration_since(sub.drawn) >= REFRESH_EVERY || forced {
                sub.dirty = true;
            }
            if !sub.dirty || now < sub.next {
                at += 1;
                continue;
            }
            if !ready.contains(&client) {
                sub.next = now + FRAME_EVERY;
                sub.wants = sub.wants.max(Wants::Complete);
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
            if let Some(content) = built {
                let view = sub.view.zip(projection);
                if let Some(text) = self.frame_text(at, content, (tab, area, view), forced) {
                    log::debug!("control", "surface frame", client = client, bytes = text.len());
                    out.push((client, Surfaced::Frame(text)));
                }
            }
            at += 1;
        }
        out
    }

    fn frame_text(
        &mut self,
        at: usize,
        content: Content,
        place: (u64, Rect, Option<(u64, u64)>),
        forced: bool,
    ) -> Option<String> {
        let (tab, area, view) = place;
        let sub = &mut self.surfaces.subscriptions[at];
        let redraw = sub.wants == Wants::Redraw;
        let wants = std::mem::take(&mut sub.wants);
        if !forced && !redraw && content == sub.last {
            return None;
        }
        let reshaped = tab != sub.tab || area != sub.area;
        let changes = (sub.patches && !forced && !reshaped && wants == Wants::Patch)
            .then(|| patch::changes(&sub.last, &content))
            .flatten();
        let id = sub.id;
        let base = sub.revision;
        sub.revision += 1;
        sub.frames += 1;
        sub.projection = view.map_or(0, |(_, projection)| projection);
        sub.tab = tab;
        sub.area = area;
        let revision = sub.revision;
        let text = match changes {
            Some(changes) if changes.panes.is_empty() => {
                sub.revision = base;
                sub.frames -= 1;
                sub.last = content;
                None
            }
            Some(changes) => {
                sub.last = content;
                let mut frame = self.surface_frame(id, tab, area, revision, view, changes);
                frame.base = Some(base);
                log::debug!("control", "surface sent", surface = id, revision = revision, kind = "patch");
                frame.streamed()
            }
            None => {
                sub.last = content.clone();
                let frame = self.surface_frame(id, tab, area, revision, view, content);
                log::debug!("control", "surface sent", surface = id, revision = revision, kind = "complete");
                frame.streamed()
            }
        };
        if text.is_none() {
            let sub = &mut self.surfaces.subscriptions[at];
            sub.revision = base;
            sub.frames = sub.frames.saturating_sub(1);
            sub.wants = Wants::Complete;
        }
        text
    }

    fn subscription_mut(&mut self, id: Option<u64>, name: &str) -> Result<&mut Subscription, Refusal> {
        let Some(id) = id else {
            let message = format!("`{name}` needs the surface: pass --surface");
            return Err(Refusal::new(Code::Malformed, message));
        };
        self.surfaces.subscriptions.iter_mut().find(|sub| sub.id == id).ok_or_else(|| {
            Refusal::new(
                Code::NotFound,
                format!("there is no surface {id}: its stream ended, or it belongs to an earlier start of the server"),
            )
        })
    }

    pub(in crate::app) fn redraw_surface_request(&mut self, redraw: &Redraw, now: Instant) -> Handled {
        let sub = self.subscription_mut(redraw.surface, "redraw")?;
        sub.wants = Wants::Redraw;
        sub.dirty = true;
        sub.next = now;
        Ok(Some(json(&Redrawn { surface: sub.id, tab: sub.tab })))
    }

    pub(in crate::app) fn resize_surface_request(&mut self, resize: &Resize, now: Instant) -> Handled {
        let area = Surface { cols: resize.cols, rows: resize.rows, ..Surface::default() }
            .area()
            .map_err(|message| Refusal::new(Code::Malformed, message))?;
        let sub = self.subscription_mut(resize.surface, "resize")?;
        let id = sub.id;
        if let Some(view) = sub.view {
            let message = format!("surface {id} follows view {view}, which sizes it: change the view with `view-set`");
            return Err(Refusal::from(message));
        }
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
        let places: Vec<Place> = self.surfaces.subscriptions.iter().map(|sub| self.sub_place(sub)).collect();
        let Some((_, tab)) = self.projects.tab_with_pane(pane) else { return };
        for (sub, place) in self.surfaces.subscriptions.iter_mut().zip(places) {
            if let Place::At(on, area, _) = place
                && on == id
                && tab.shown(area).iter().any(|(shown, _)| *shown == pane)
            {
                sub.dirty = true;
            }
        }
    }

    pub(super) fn surface_wake(&self, now: Instant) -> Option<Duration> {
        self.surfaces
            .subscriptions
            .iter()
            .filter(|sub| sub.dirty && matches!(self.sub_place(sub), Place::At(..)))
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
