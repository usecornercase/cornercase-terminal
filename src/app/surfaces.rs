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

enum Rendered {
    Kept(Option<(u64, Surfaced)>),
    Ended((u64, Surfaced)),
}

#[derive(Clone, Copy)]
struct Shown {
    surface: u64,
    tab: u64,
    area: Rect,
    view: Option<(u64, u64)>,
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

impl Subscription {
    fn due(&mut self, now: Instant, forced: bool, ready: bool) -> bool {
        if now.saturating_duration_since(self.drawn) >= REFRESH_EVERY || forced {
            self.dirty = true;
        }
        if !self.dirty || now < self.next {
            return false;
        }
        if !ready {
            self.next = now + FRAME_EVERY;
            self.wants = self.wants.max(Wants::Complete);
            return false;
        }
        true
    }
}

fn log_opened(client: u64, shown: Shown, patches: bool) {
    log::info!(
        "control",
        "surface",
        client = client,
        surface = shown.surface,
        tab = shown.tab,
        cols = shown.area.width,
        rows = shown.area.height,
        patches = patches
    );
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
        let (tab, area, projection) = self.surface_target(view, surface)?;
        self.refuse_second_stream(client)?;
        let content = self.surface_content(tab, area).ok_or_else(|| none("tab", tab))?;
        self.clients.surfaces.issued += 1;
        let id = self.clients.surfaces.issued;
        let shown = Shown { surface: id, tab, area, view: projection.map(|p| (view.unwrap_or_default(), p)) };
        let frame = self.surface_frame(shown, 1, content.clone());
        log_opened(client, shown, surface.patches);
        self.clients.surfaces.subscriptions.push(Subscription {
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

    fn surface_target(&self, view: Option<u64>, surface: &Surface) -> Result<(u64, Rect, Option<u64>), Refusal> {
        if let Some(view) = view {
            return self.view_surface(view, surface);
        }
        let Some(tab) = surface.tab else {
            return Err(Refusal::new(Code::Malformed, "`surface` needs the tab to stream: pass --tab"));
        };
        let area = surface.area().map_err(|message| Refusal::new(Code::Malformed, message))?;
        Ok((tab, area, None))
    }

    fn refuse_second_stream(&self, client: u64) -> Result<(), Refusal> {
        match self.clients.surfaces.subscriptions.iter().find(|s| s.client == client) {
            Some(own) => {
                let message =
                    format!("this connection already streams tab {}; open another connection for another tab", own.tab);
                Err(Refusal::from(message))
            }
            None => Ok(()),
        }
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
        let Some(view) = self.clients.views.get(view) else { return Place::Gone };
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

    fn surface_frame<B>(&self, shown: Shown, revision: u64, content: B) -> Frame<B> {
        Frame {
            base: None,
            view: shown.view.map(|(view, _)| view),
            projection: shown.view.map(|(_, projection)| projection),
            boot: control::server_token().to_string(),
            session: self.clients.events.snapshots.revision(),
            revision,
            surface: shown.surface,
            tab: shown.tab,
            cols: shown.area.width,
            rows: shown.area.height,
            content,
        }
    }

    pub(crate) fn render_surfaces(&mut self, ready: &[u64], now: Instant) -> Vec<(u64, Surfaced)> {
        self.settle_views();
        let mut out = Vec::new();
        let mut at = 0;
        while at < self.clients.surfaces.subscriptions.len() {
            match self.render_surface(at, ready, now) {
                Rendered::Kept(sent) => {
                    out.extend(sent);
                    at += 1;
                }
                Rendered::Ended(ended) => out.push(ended),
            }
        }
        out
    }

    fn render_surface(&mut self, at: usize, ready: &[u64], now: Instant) -> Rendered {
        let client = self.clients.surfaces.subscriptions[at].client;
        let (tab, area, projection) = match self.sub_place(&self.clients.surfaces.subscriptions[at]) {
            Place::At(tab, area, projection) => (tab, area, projection),
            Place::Paused(_) => {
                self.clients.surfaces.subscriptions[at].dirty = false;
                return Rendered::Kept(None);
            }
            Place::Gone => {
                self.end_surface(at, "closed");
                return Rendered::Ended((client, Surfaced::End));
            }
        };
        let sub = &mut self.clients.surfaces.subscriptions[at];
        let forced = projection.is_some_and(|p| p != sub.projection);
        if !sub.due(now, forced, ready.contains(&client)) {
            return Rendered::Kept(None);
        }
        let Some(built) = crate::panics::contain(|| self.surface_content(tab, area)) else {
            log::error!("control", "surface panicked", client = client, tab = tab);
            self.report_bug();
            self.end_surface(at, "bug");
            return Rendered::Ended((client, Surfaced::End));
        };
        let sub = &mut self.clients.surfaces.subscriptions[at];
        sub.drawn = now;
        sub.next = now + FRAME_EVERY;
        sub.dirty = false;
        let view = sub.view.zip(projection);
        let text = built.and_then(|content| self.frame_text(at, content, (tab, area, view), forced));
        Rendered::Kept(text.map(|text| {
            log::debug!("control", "surface frame", client = client, bytes = text.len());
            (client, Surfaced::Frame(text))
        }))
    }

    fn frame_text(
        &mut self,
        at: usize,
        content: Content,
        place: (u64, Rect, Option<(u64, u64)>),
        forced: bool,
    ) -> Option<String> {
        let (shown, base, changes) = self.next_frame(at, &content, place, forced)?;
        let sub = &mut self.clients.surfaces.subscriptions[at];
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
                self.patch_text(shown, revision, base, changes)
            }
            None => {
                sub.last = content.clone();
                self.complete_text(shown, revision, content)
            }
        };
        if text.is_none() {
            let sub = &mut self.clients.surfaces.subscriptions[at];
            sub.revision = base;
            sub.frames = sub.frames.saturating_sub(1);
            sub.wants = Wants::Complete;
        }
        text
    }

    fn next_frame(
        &mut self,
        at: usize,
        content: &Content,
        place: (u64, Rect, Option<(u64, u64)>),
        forced: bool,
    ) -> Option<(Shown, u64, Option<patch::Changes>)> {
        let (tab, area, view) = place;
        let sub = &mut self.clients.surfaces.subscriptions[at];
        let redraw = sub.wants == Wants::Redraw;
        let wants = std::mem::take(&mut sub.wants);
        if !forced && !redraw && *content == sub.last {
            return None;
        }
        let reshaped = tab != sub.tab || area != sub.area;
        let changes = (sub.patches && !forced && !reshaped && wants == Wants::Patch)
            .then(|| patch::changes(&sub.last, content))
            .flatten();
        let (surface, base) = (sub.id, sub.revision);
        sub.revision += 1;
        sub.frames += 1;
        sub.projection = view.map_or(0, |(_, projection)| projection);
        sub.tab = tab;
        sub.area = area;
        Some((Shown { surface, tab, area, view }, base, changes))
    }

    fn patch_text(&self, shown: Shown, revision: u64, base: u64, changes: patch::Changes) -> Option<String> {
        let mut frame = self.surface_frame(shown, revision, changes);
        frame.base = Some(base);
        log::debug!("control", "surface sent", surface = shown.surface, revision = revision, kind = "patch");
        frame.streamed()
    }

    fn complete_text(&self, shown: Shown, revision: u64, content: Content) -> Option<String> {
        let frame = self.surface_frame(shown, revision, content);
        log::debug!("control", "surface sent", surface = shown.surface, revision = revision, kind = "complete");
        frame.streamed()
    }

    fn subscription_mut(&mut self, id: Option<u64>, name: &str) -> Result<&mut Subscription, Refusal> {
        let Some(id) = id else {
            let message = format!("`{name}` needs the surface: pass --surface");
            return Err(Refusal::new(Code::Malformed, message));
        };
        self.clients.surfaces.subscriptions.iter_mut().find(|sub| sub.id == id).ok_or_else(|| {
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
        let sub = self.clients.surfaces.subscriptions.remove(at);
        log::info!("control", "surface ended", client = sub.client, reason = reason, frames = sub.frames);
    }

    pub(crate) fn touch_surfaces(&mut self) {
        self.clients.surfaces.subscriptions.iter_mut().for_each(|sub| sub.dirty = true);
    }

    pub(super) fn surface_printed(&mut self, pane: u64) {
        if self.clients.surfaces.subscriptions.is_empty() {
            return;
        }
        let Some((_, tab)) = self.projects.tab_with_pane(pane) else { return };
        let id = tab.id;
        let places: Vec<Place> = self.clients.surfaces.subscriptions.iter().map(|sub| self.sub_place(sub)).collect();
        let Some((_, tab)) = self.projects.tab_with_pane(pane) else { return };
        for (sub, place) in self.clients.surfaces.subscriptions.iter_mut().zip(places) {
            if let Place::At(on, area, _) = place
                && on == id
                && tab.shown(area).iter().any(|(shown, _)| *shown == pane)
            {
                sub.dirty = true;
            }
        }
    }

    pub(super) fn surface_wake(&self, now: Instant) -> Option<Duration> {
        self.clients
            .surfaces
            .subscriptions
            .iter()
            .filter(|sub| sub.dirty && matches!(self.sub_place(sub), Place::At(..)))
            .map(|sub| sub.next.saturating_duration_since(now))
            .min()
    }

    pub(crate) fn streams_surfaces(&self) -> bool {
        !self.clients.surfaces.subscriptions.is_empty()
    }

    pub(in crate::app) fn forget_surfaces(&mut self, client: u64) {
        if let Some(at) = self.clients.surfaces.subscriptions.iter().position(|sub| sub.client == client) {
            self.end_surface(at, "client left");
        }
    }
}
