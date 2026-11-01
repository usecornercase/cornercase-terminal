use std::collections::VecDeque;
use std::mem;
use std::sync::Arc;
use std::thread;

use parking_lot::{Condvar, Mutex};
use serde_json::Value;

use super::{Group, Shared, StreamEvent, Tell, reply};
use crate::control::contract::{Code, Refusal};
use crate::control::surface::COLS;
use crate::control::terminal::InputEvent;
use crate::control::{Command, Focus, Input, Item, Ratio, Redraw, Scroll, Selection, SetView};
use crate::machines::ask::LATE;
use crate::machines::link::{self, Asked, Connect, Control, Envelope, Heard, Line};
use crate::panics;
use crate::remote;
use crate::text;

const LOST: &str = "the link ended";
const JOINED_BYTES: usize = 1 << 20;
const EVENT_BYTES: usize = 64;
const CHARS_PER_CELL: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Order {
    Size { cols: u16, rows: u16 },
    Show(u64),
    Redraw(u64),
    Input(Input),
    Scroll(Scroll),
    Select(Selection),
    Copy(Selection),
    Ratio(Ratio),
    Focus(u64),
}

impl Order {
    fn replaces(&self, waiting: &Self) -> bool {
        match (self, waiting) {
            (Self::Ratio(next), Self::Ratio(old)) => (next.tab, &next.path) == (old.tab, &old.path),
            _ => matches!(
                (self, waiting),
                (Self::Size { .. }, Self::Size { .. })
                    | (Self::Show(_), Self::Show(_))
                    | (Self::Select(_) | Self::Copy(_), Self::Select(_))
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Answer {
    Done { order: Order, value: Value },
    Selected(Option<String>),
    NotReading,
    Refused { order: Order, refusal: Refusal },
}

#[derive(Default)]
struct Queue {
    orders: VecDeque<Order>,
    stop: bool,
}

#[derive(Default)]
pub(super) struct Orders {
    queue: Mutex<Queue>,
    wake: Condvar,
    control: Control,
}

impl Orders {
    pub(super) fn start(
        connector: Arc<dyn Connect>,
        shared: Arc<Shared>,
        stream: Arc<Control>,
        tell: Tell,
    ) -> Arc<Self> {
        let orders = Arc::new(Self::default());
        let sender = Sender { orders: Arc::clone(&orders), connector, shared, stream, tell };
        thread::spawn(move || sender.run());
        orders
    }

    pub(super) fn push(&self, order: Order) {
        let mut queue = self.queue.lock();
        queue.orders.retain(|waiting| !order.replaces(waiting));
        if let Some(order) = joined(queue.orders.back_mut(), order) {
            queue.orders.push_back(order);
        }
        self.wake.notify_all();
    }

    pub(super) fn stop(&self) {
        self.queue.lock().stop = true;
        self.wake.notify_all();
        self.control.stop();
    }

    fn next(&self) -> Option<Order> {
        let mut queue = self.queue.lock();
        loop {
            if queue.stop {
                return None;
            }
            if let Some(order) = queue.orders.pop_front() {
                return Some(order);
            }
            self.wake.wait(&mut queue);
        }
    }
}

fn joined(waiting: Option<&mut Order>, order: Order) -> Option<Order> {
    match (waiting, order) {
        (Some(Order::Input(last)), Order::Input(next))
            if (last.pane, last.popup, last.size) == (next.pane, next.popup, next.size)
                && weight(&last.events) + weight(&next.events) <= JOINED_BYTES =>
        {
            last.events.extend(next.events);
            None
        }
        (_, order) => Some(order),
    }
}

fn weight(events: &[InputEvent]) -> usize {
    events
        .iter()
        .map(|event| match event {
            InputEvent::Text(text) | InputEvent::Paste(text) => text.len() + EVENT_BYTES,
            InputEvent::Key(_) | InputEvent::Mouse(_) => EVENT_BYTES,
        })
        .sum()
}

fn selected(text: &str, most: usize) -> String {
    text.chars()
        .filter_map(|c| match c {
            '\n' => Some(c),
            '\r' => None,
            c if c.is_whitespace() => Some(' '),
            c => (!text::hidden(c)).then_some(c),
        })
        .take(most)
        .collect()
}

fn most(selection: &Selection) -> usize {
    let cols = selection.size.map_or(*COLS.end(), |size| size.cols);
    let (from, to) = (selection.from.map_or(0, |at| at.y), selection.to.map_or(0, |at| at.y));
    (usize::from(cols) + 1) * (usize::from(from.abs_diff(to)) + 1) * CHARS_PER_CELL
}

struct Sender {
    orders: Arc<Orders>,
    connector: Arc<dyn Connect>,
    shared: Arc<Shared>,
    stream: Arc<Control>,
    tell: Tell,
}

impl Sender {
    fn run(self) {
        let mut line = None;
        while let Some(order) = self.orders.next() {
            let sent = panics::job(|| {
                self.send(&mut line, order);
                Ok(())
            });
            if sent.is_err() {
                self.drop_line(&mut line);
            }
        }
        self.drop_line(&mut line);
    }

    fn send(&self, line: &mut Option<Line>, order: Order) {
        let Some((view, envelope, command)) = self.request(&order) else { return };
        let group = Arc::new(Group::default());
        let late = self.shared.alarms.set(Group::held(&group));
        let heard = match self.connected(line, &group) {
            Ok(connected) => link::send_once(connected, &envelope, command),
            Err(why) => {
                let why = if late.rang() { LATE.into() } else { why };
                return self.refused(order, view, Refusal::new(Code::Unavailable, why));
            }
        };
        match heard {
            Heard::Answer(text) => self.answered(order, view, &text),
            Heard::Ended(_) if late.rang() => {
                self.drop_line(line);
                self.refused(order, view, Refusal::new(Code::Unavailable, LATE));
            }
            Heard::Ended(said) => {
                let why = line.as_ref().map_or_else(|| LOST.into(), |ended| link::why(ended, said));
                self.drop_line(line);
                self.refused(order, view, Refusal::new(Code::Unavailable, why));
            }
        }
    }

    fn request(&self, order: &Order) -> Option<(u64, Envelope, Command)> {
        let opened = self.shared.opened.lock().clone()?;
        let id = opened.view;
        let pane = Envelope { view: None, boot: Some(opened.boot) };
        let view = Envelope { view: Some(opened.view), ..pane.clone() };
        let focus = |item| Command::Focus(Focus { item, reveal: None });
        let (envelope, command) = match order {
            Order::Size { cols, rows } => {
                (view, Command::ViewSet(SetView { cols: Some(*cols), rows: Some(*rows), ..SetView::default() }))
            }
            Order::Show(tab) => (view, focus(Item::Tab(*tab))),
            Order::Focus(id) => (view, focus(Item::Pane(*id))),
            Order::Redraw(surface) => (pane, Command::Redraw(Redraw { surface: Some(*surface) })),
            Order::Input(input) => (pane, Command::Input(input.clone())),
            Order::Scroll(scroll) => (pane, Command::Scroll(scroll.clone())),
            Order::Select(selection) | Order::Copy(selection) => (pane, Command::Selection(selection.clone())),
            Order::Ratio(ratio) => (pane, Command::Ratio(ratio.clone())),
        };
        Some((id, envelope, command))
    }

    fn connected<'a>(&self, line: &'a mut Option<Line>, group: &Arc<Group>) -> Result<&'a mut Line, String> {
        if self.orders.control.hold(&Group::held(group)) == Asked::Stop {
            return Err(LOST.into());
        }
        if line.is_none() {
            let connected = self.connector.connect(&group.killer).map_err(|e| link::clean(&remote::reason(&e)))?;
            *line = Some(connected);
        }
        let connected = line.as_mut().ok_or_else(|| String::from(LOST))?;
        group.add(&connected.ends);
        Ok(connected)
    }

    fn drop_line(&self, line: &mut Option<Line>) {
        self.orders.control.release();
        if let Some(ended) = mem::take(line) {
            ended.ends.cut();
        }
    }

    fn answered(&self, order: Order, view: u64, text: &str) {
        match reply(text) {
            Ok(value) => {
                if let Some(projection) = value.get("projection").and_then(Value::as_u64) {
                    self.shared.seen(view, projection);
                }
                let answer = match order {
                    Order::Redraw(_) | Order::Select(_) => return,
                    Order::Copy(selection) => {
                        let most = most(&selection);
                        Answer::Selected(value.get("text").and_then(Value::as_str).map(|text| selected(text, most)))
                    }
                    order => Answer::Done { order, value: link::cleaned(value) },
                };
                (self.tell)(StreamEvent::Answer(answer));
            }
            Err(refusal) => self.refused(order, view, refusal),
        }
    }

    fn refused(&self, order: Order, view: u64, refusal: Refusal) {
        let answer = match (order, refusal.code) {
            (Order::Redraw(_), _) => return self.reopen(view, None),
            (_, Code::Conflict | Code::NotFound) => return,
            (_, Code::Busy) => Answer::NotReading,
            (order @ (Order::Size { .. } | Order::Show(_)), Code::Unavailable) => {
                let why = refusal.message.clone();
                (self.tell)(StreamEvent::Answer(Answer::Refused { order, refusal }));
                return self.reopen(view, Some(why));
            }
            (order, _) => Answer::Refused { order, refusal },
        };
        (self.tell)(StreamEvent::Answer(answer));
    }

    fn reopen(&self, view: u64, why: Option<String>) {
        if self.shared.opened.lock().as_ref().is_some_and(|opened| opened.view == view) {
            *self.shared.lost.lock() = why;
            self.stream.poke();
        }
    }
}

#[cfg(test)]
mod tests;
