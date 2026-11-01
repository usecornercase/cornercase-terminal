use std::io::{self, Read};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use parking_lot::Mutex;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::ask::LATE;
use super::link::{self, Asked, Clock, Connect, Control, Ends, Envelope, Heard, Line, Wall};
use crate::control::contract::{Code, Refusal, Structured};
use crate::control::surface::mirror::{Mirror, Unapplied};
use crate::control::surface::{COLS, ROWS};
use crate::control::{Command, OpenView, Response, Snapshot, Surface, ViewInfo};
use crate::error::Error;
use crate::panics;
use crate::remote;
use alarm::{Alarm, Alarms, Timer};
use orders::{Order, Orders};
use screen::RemoteScreen;

mod alarm;
pub(crate) mod orders;
pub(crate) mod screen;

const UNREADABLE: &str = "cannot read the answer from there";
const NO_VIEW: &str = "the server there opened no view";
const REDRAWN: &str = "the server there could not draw the tab again";
const AFTER_A_BUG: u32 = 5;
const ANSWERS_WITHIN: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StreamReport {
    pub(crate) stream: u64,
    pub(crate) event: StreamEvent,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StreamEvent {
    Opened { view: u64 },
    Frame(Arc<RemoteScreen>),
    Down(String),
    Answer(orders::Answer),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Wanted {
    pub(crate) tab: u64,
    pub(crate) cols: u16,
    pub(crate) rows: u16,
}

impl Wanted {
    fn bounded(self) -> Self {
        let bound = |value: u16, range: &std::ops::RangeInclusive<u16>| value.clamp(*range.start(), *range.end());
        Self { tab: self.tab, cols: bound(self.cols, &COLS), rows: bound(self.rows, &ROWS) }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Opened {
    view: u64,
    boot: String,
    wanted: Wanted,
}

struct Timing {
    clock: Box<dyn Clock>,
    alarms: Alarms,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Fence {
    view: u64,
    projection: u64,
}

struct Shared {
    wanted: Mutex<Wanted>,
    opened: Mutex<Option<Opened>>,
    fence: Mutex<Fence>,
    alarms: Alarms,
}

impl Shared {
    fn new(wanted: Wanted, alarms: Alarms) -> Self {
        let (wanted, opened) = (Mutex::new(wanted.bounded()), Mutex::default());
        Self { wanted, opened, fence: Mutex::default(), alarms }
    }

    fn seen(&self, view: u64, projection: u64) -> u64 {
        let mut fence = self.fence.lock();
        if fence.view == view {
            fence.projection = fence.projection.max(projection);
        }
        fence.projection
    }
}

type Tell = Arc<dyn Fn(StreamEvent) + Send + Sync>;

fn teller<E>(stream: u64, tx: &Sender<E>, control: &Arc<Control>) -> Tell
where
    E: From<StreamReport> + Send + 'static,
{
    let (tx, control) = (tx.clone(), Arc::clone(control));
    Arc::new(move |event| {
        if !control.stopped() {
            let _ = tx.send(StreamReport { stream, event }.into());
        }
    })
}

#[derive(Default)]
struct Group {
    lines: Mutex<(bool, Vec<Arc<dyn Ends>>)>,
}

impl Group {
    fn add(&self, ends: &Arc<dyn Ends>) {
        let mut lines = self.lines.lock();
        if lines.0 {
            ends.cut();
        }
        lines.1.push(Arc::clone(ends));
    }
}

impl Ends for Group {
    fn cut(&self) {
        let mut lines = self.lines.lock();
        lines.0 = true;
        lines.1.iter().for_each(|ends| ends.cut());
    }

    fn why(&self) -> Option<String> {
        None
    }
}

pub(crate) struct Stream {
    id: u64,
    control: Arc<Control>,
    shared: Arc<Shared>,
    orders: Arc<Orders>,
}

impl Stream {
    pub(crate) fn start<E>(id: u64, connector: Box<dyn Connect>, wanted: Wanted, tx: &Sender<E>) -> Self
    where
        E: From<StreamReport> + Send + 'static,
    {
        let alarms = Alarms::new(Arc::new(Timer::new()), ANSWERS_WITHIN);
        Self::spawn(id, Arc::from(connector), wanted, tx, Timing { clock: Box::new(Wall), alarms })
    }

    fn spawn<E>(id: u64, connector: Arc<dyn Connect>, wanted: Wanted, tx: &Sender<E>, timing: Timing) -> Self
    where
        E: From<StreamReport> + Send + 'static,
    {
        let control = Arc::new(Control::default());
        let shared = Arc::new(Shared::new(wanted, timing.alarms));
        let tell = teller(id, tx, &control);
        let orders =
            Orders::start(Arc::clone(&connector), Arc::clone(&shared), Arc::clone(&control), Arc::clone(&tell));
        let running = Running {
            connector,
            shared: Arc::clone(&shared),
            control: Arc::clone(&control),
            orders: Arc::clone(&orders),
            clock: timing.clock,
            tell,
        };
        thread::spawn(move || running.run());
        Self { id, control, shared, orders }
    }

    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub(crate) fn resize(&self, cols: u16, rows: u16) {
        let mut wanted = self.shared.wanted.lock();
        let next = Wanted { cols, rows, ..*wanted }.bounded();
        if next != *wanted {
            *wanted = next;
            self.orders.push(Order::Size { cols: next.cols, rows: next.rows });
        }
    }

    pub(crate) fn send(&self, order: Order) {
        self.orders.push(order);
    }

    pub(crate) fn tab(&self) -> u64 {
        self.shared.wanted.lock().tab
    }

    pub(crate) fn show(&self, tab: u64) {
        let mut wanted = self.shared.wanted.lock();
        if wanted.tab != tab {
            wanted.tab = tab;
            self.orders.push(Order::Show(tab));
        }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.control.stop();
        self.orders.stop();
    }
}

enum Outcome {
    Stop,
    Failed(String),
    Dropped(String),
}

impl Outcome {
    fn redrawn(self) -> Self {
        match self {
            Self::Stop => Self::Stop,
            Self::Failed(_) => Self::Failed(REDRAWN.into()),
            Self::Dropped(_) => Self::Dropped(REDRAWN.into()),
        }
    }
}

struct Running {
    connector: Arc<dyn Connect>,
    shared: Arc<Shared>,
    control: Arc<Control>,
    orders: Arc<Orders>,
    clock: Box<dyn Clock>,
    tell: Tell,
}

impl Running {
    fn run(self) {
        loop {
            let served = panics::job(|| {
                self.serve();
                Ok(())
            });
            if served.is_ok() {
                return;
            }
            self.control.drop_line();
            (self.tell)(StreamEvent::Down(Error::Bug.to_string()));
            if self.clock.nap(&self.control, remote::backoff(AFTER_A_BUG)) == Asked::Stop {
                return;
            }
        }
    }

    fn serve(&self) {
        let mut failed: u32 = 0;
        loop {
            let why = match self.attempt() {
                Outcome::Stop => return,
                Outcome::Dropped(why) => {
                    failed = 1;
                    why
                }
                Outcome::Failed(why) => {
                    failed = failed.saturating_add(1);
                    why
                }
            };
            (self.tell)(StreamEvent::Down(why));
            match self.clock.nap(&self.control, remote::backoff(failed)) {
                Asked::Stop => return,
                Asked::Poke => failed = 0,
                Asked::Nothing => {}
            }
        }
    }

    fn attempt(&self) -> Outcome {
        let group = Arc::new(Group::default());
        let outcome = match self.streamed(&group) {
            Ok(outcome) | Err(outcome) => outcome,
        };
        let asked = self.control.release();
        group.cut();
        *self.shared.opened.lock() = None;
        match asked {
            Asked::Stop => Outcome::Stop,
            Asked::Poke => outcome.redrawn(),
            Asked::Nothing => outcome,
        }
    }

    fn streamed(&self, group: &Arc<Group>) -> Result<Outcome, Outcome> {
        let alarm = self.shared.alarms.set(Arc::clone(group) as Arc<dyn Ends>);
        let streamed = self.opening(group, &alarm);
        if alarm.rang() {
            return Err(Outcome::Failed(LATE.into()));
        }
        streamed
    }

    fn opening(&self, group: &Arc<Group>, alarm: &Alarm) -> Result<Outcome, Outcome> {
        let mut view = self.line(group)?;
        let wanted = *self.shared.wanted.lock();
        let opened = open_view(&mut view, wanted, &self.shared)?;
        self.opened(&opened);
        let reader = std::mem::replace(&mut view.reader, Box::new(io::empty()));
        drain(reader, opened.view, Arc::clone(&self.shared), Arc::clone(group));
        let mut surface = self.line(group)?;
        let envelope = Envelope { view: Some(opened.view), boot: Some(opened.boot.clone()) };
        let command = Command::Surface(Surface { patches: true, ..Surface::default() });
        if !link::send(&mut surface, &envelope, command) {
            return Err(Outcome::Failed(link::why(&surface, None)));
        }
        Ok(self.frames(&mut surface, &opened, alarm))
    }

    fn line(&self, group: &Arc<Group>) -> Result<Line, Outcome> {
        let line = self
            .connector
            .connect(self.control.killer())
            .map_err(|error| Outcome::Failed(link::clean(&remote::reason(&error))))?;
        group.add(&line.ends);
        let held: Arc<dyn Ends> = Arc::clone(group) as Arc<dyn Ends>;
        match self.control.hold(&held) {
            Asked::Nothing => Ok(line),
            Asked::Stop => Err(Outcome::Stop),
            Asked::Poke => Err(Outcome::Failed(REDRAWN.into())),
        }
    }

    fn opened(&self, opened: &Opened) {
        *self.shared.opened.lock() = Some(opened.clone());
        (self.tell)(StreamEvent::Opened { view: opened.view });
        let now = *self.shared.wanted.lock();
        if (now.cols, now.rows) != (opened.wanted.cols, opened.wanted.rows) {
            self.orders.push(Order::Size { cols: now.cols, rows: now.rows });
        }
        if now.tab != opened.wanted.tab {
            self.orders.push(Order::Show(now.tab));
        }
    }

    fn frames(&self, surface: &mut Line, opened: &Opened, alarm: &Alarm) -> Outcome {
        let mut frames = Frames::default();
        loop {
            let text = match link::listen(&mut surface.reader) {
                Heard::Answer(text) => text,
                Heard::Ended(said) if frames.live => return Outcome::Dropped(link::why(surface, said)),
                Heard::Ended(said) => return Outcome::Failed(link::why(surface, said)),
            };
            match self.take(&mut frames, &text, opened) {
                Ok(Some(screen)) => {
                    alarm.off();
                    frames.live = true;
                    (self.tell)(StreamEvent::Frame(screen));
                }
                Ok(None) => {}
                Err(why) => return Outcome::Failed(why),
            }
        }
    }

    fn take(&self, frames: &mut Frames, text: &str, opened: &Opened) -> Result<Option<Arc<RemoteScreen>>, String> {
        match frames.mirror.apply(text) {
            Ok(()) => {
                frames.redrawing = false;
                return Ok(self.showable(&frames.mirror, opened));
            }
            Err(Unapplied::Behind) if !frames.redrawing => {
                frames.redrawing = true;
                self.orders.push(Order::Redraw(frames.mirror.surface));
            }
            Err(Unapplied::Behind) => {}
            Err(Unapplied::Wrong(_)) => return Err(refusal(text)),
        }
        Ok(None)
    }

    fn showable(&self, mirror: &Mirror, opened: &Opened) -> Option<Arc<RemoteScreen>> {
        let projection = mirror.projection.unwrap_or_default();
        if mirror.boot != opened.boot || projection < self.shared.seen(opened.view, projection) {
            return None;
        }
        Some(Arc::new(RemoteScreen::of(mirror)))
    }
}

#[derive(Default)]
struct Frames {
    mirror: Mirror,
    redrawing: bool,
    live: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ViewLine {
    boot: String,
    view: Option<ViewInfo>,
}

#[derive(Deserialize)]
struct Okay<T> {
    ok: T,
}

fn open_view(line: &mut Line, wanted: Wanted, shared: &Shared) -> Result<Opened, Outcome> {
    let view = OpenView {
        cols: Some(wanted.cols),
        rows: Some(wanted.rows),
        active: Some(true),
        tab: Some(wanted.tab),
        visible: Some(true),
        ..OpenView::default()
    };
    let text = match link::send_once(line, &Envelope::default(), Command::View(view)) {
        Heard::Answer(text) => text,
        Heard::Ended(said) => return Err(Outcome::Failed(link::why(line, said))),
    };
    let snapshot: Snapshot<ViewLine> = read(&text).map_err(Outcome::Failed)?;
    let info = snapshot.report.view.ok_or_else(|| Outcome::Failed(NO_VIEW.into()))?;
    *shared.fence.lock() = Fence { view: info.id, projection: info.projection };
    Ok(Opened { view: info.id, boot: snapshot.report.boot, wanted })
}

fn drain(mut reader: Box<dyn Read + Send>, view: u64, shared: Arc<Shared>, group: Arc<Group>) {
    thread::spawn(move || {
        while let Heard::Answer(text) = link::listen(&mut reader) {
            if let Some(projection) = projection_of(&text) {
                shared.seen(view, projection);
            }
        }
        group.cut();
    });
}

fn projection_of(text: &str) -> Option<u64> {
    let line: Okay<Snapshot<ViewLine>> = serde_json::from_str(text).ok()?;
    line.ok.report.view.map(|view| view.projection)
}

fn read<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    serde_json::from_str::<Okay<T>>(text).map(|line| line.ok).map_err(|_| refusal(text))
}

fn refusal(text: &str) -> String {
    reply(text).err().map_or_else(|| UNREADABLE.into(), |refusal| refusal.message)
}

fn reply(text: &str) -> Result<Value, Refusal> {
    if let Ok(answer) = serde_json::from_str::<Structured>(text) {
        return match answer {
            Structured::Ok(value) => Ok(value),
            Structured::Error(refusal) => Err(Refusal::new(refusal.code, link::clean(&refusal.message))),
        };
    }
    match serde_json::from_str::<Response>(text) {
        Ok(Response::Ok(value)) => Ok(value),
        Ok(Response::Error(message)) => Err(Refusal::new(Code::Failed, link::clean(&message))),
        Err(_) => Err(Refusal::new(Code::Failed, UNREADABLE)),
    }
}

#[cfg(test)]
pub(crate) mod fake;
#[cfg(test)]
mod tests;
