use std::io::{self, Write};
use std::iter::once;

use serde::Deserialize as _;
use serde_json::{Value, json};

use super::{not_an_event, skipped};
use crate::cli::args::terminal::InputArgs;
use crate::cli::lines::{self, Lines};
use crate::client::{self, answer};
use crate::control::terminal::{InputEvent, Sent};
use crate::control::{self, Command};
use crate::error::{Error, Result};

const ONLY_STANDARD_INPUT: &str =
    "--follow reads its events from standard input, one per line: give - as the only EVENT";

const NEEDS_A_PANE: &str = "this line names no pane and --pane was not given: add \"pane\": ID to the line";

pub(super) fn run(args: &InputArgs) -> Result<()> {
    only_standard_input(&args.events)?;
    if args.popup {
        client::use_popup_terminal()?;
    }
    let mut link = client::connect()?;
    let lines = lines::read(io::stdin());
    let template = control::Input { pane: args.pane, popup: args.popup, ..control::Input::default() };
    let mut printer = Printer::new(args.print.json, io::stdout(), io::stderr());
    typed(&lines, &template, |input| link.ask("input", Command::Input(input)), &mut printer)
}

fn only_standard_input(events: &[String]) -> Result<()> {
    match events {
        [only] if only == "-" => Ok(()),
        _ => Err(Error::WrongUsage(ONLY_STANDARD_INPUT.into())),
    }
}

enum Line {
    Event(Option<u64>, InputEvent),
    Wrong(String),
}

fn line(bytes: &[u8]) -> Option<Line> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return None;
    }
    let wrong = |e| Line::Wrong(not_an_event(e, String::from_utf8_lossy(bytes).trim_end()));
    Some(read(bytes).unwrap_or_else(wrong))
}

fn read(bytes: &[u8]) -> serde_json::Result<Line> {
    let mut value: Value = serde_json::from_slice(bytes)?;
    let pane = match value.as_object_mut().and_then(|object| object.remove("pane")) {
        Some(pane) => Option::<u64>::deserialize(pane)?,
        None => None,
    };
    Ok(Line::Event(pane, serde_json::from_value(value)?))
}

enum Part {
    Send(control::Input),
    Wrong(String),
}

fn parts(lines: impl IntoIterator<Item = Line>, template: &control::Input) -> Vec<Part> {
    let mut parts = Vec::new();
    for line in lines {
        match line {
            Line::Event(pane, event) => match pane.or(template.pane) {
                None if !template.popup => parts.push(Part::Wrong(NEEDS_A_PANE.into())),
                pane => add(&mut parts, pane, event, template),
            },
            Line::Wrong(message) => parts.push(Part::Wrong(message)),
        }
    }
    parts
}

fn add(parts: &mut Vec<Part>, pane: Option<u64>, event: InputEvent, template: &control::Input) {
    match parts.last_mut() {
        Some(Part::Send(last)) if last.pane == pane => last.events.push(event),
        _ => parts.push(Part::Send(control::Input { pane, events: vec![event], ..template.clone() })),
    }
}

fn typed(
    lines: &Lines,
    template: &control::Input,
    mut ask: impl FnMut(control::Input) -> Result<Value>,
    printer: &mut Printer<impl Write, impl Write>,
) -> Result<()> {
    while let Ok(first) = lines.recv() {
        let (waiting, failed) = waiting(first, lines);
        for part in parts(waiting.iter().filter_map(|bytes| line(bytes)), template) {
            if printer.part(part, &mut ask)? {
                return Ok(());
            }
        }
        if let Some(error) = failed {
            return Err(error.into());
        }
    }
    printer.finished()
}

fn waiting(first: io::Result<Vec<u8>>, lines: &Lines) -> (Vec<Vec<u8>>, Option<io::Error>) {
    let mut waiting = Vec::new();
    for read in once(first).chain(lines.try_iter()) {
        match read {
            Ok(bytes) => waiting.push(bytes),
            Err(error) => return (waiting, Some(error)),
        }
    }
    (waiting, None)
}

struct Printer<O, E> {
    json: bool,
    out: O,
    err: E,
    read: usize,
    failed: bool,
}

impl<O: Write, E: Write> Printer<O, E> {
    fn new(json: bool, out: O, err: E) -> Self {
        Self { json, out, err, read: 0, failed: false }
    }

    fn part(&mut self, part: Part, ask: &mut impl FnMut(control::Input) -> Result<Value>) -> Result<bool> {
        let input = match part {
            Part::Wrong(message) => return self.refused(None, 1, &message),
            Part::Send(input) => input,
        };
        let (pane, events) = (input.pane, input.events.len());
        match ask(input) {
            Ok(value) => self.sent(value, events),
            Err(Error::Control(message)) => self.refused(pane, events, &message),
            Err(e) => Err(e),
        }
    }

    fn take(&mut self, events: usize) -> usize {
        let first = self.read;
        self.read += events;
        first
    }

    fn sent(&mut self, value: Value, events: usize) -> Result<bool> {
        let first = self.take(events);
        if self.json {
            return self.line(&numbered(value, first, events));
        }
        for skip in answer::<Sent>(value)?.skipped {
            let _ = writeln!(self.err, "warning: event {} was not sent: {}", first + skip.event + 1, skipped(skip.why));
        }
        Ok(false)
    }

    fn refused(&mut self, pane: Option<u64>, events: usize, message: &str) -> Result<bool> {
        let first = self.take(events);
        self.failed = true;
        if !self.json {
            let _ = writeln!(self.err, "error: {} not sent: {message}", which(first, events));
            return Ok(false);
        }
        let mut value = json!({"error": message, "events": events});
        if let (Some(pane), Some(object)) = (pane, value.as_object_mut()) {
            object.insert("pane".into(), json!(pane));
        }
        self.line(&value)
    }

    fn line(&mut self, value: &Value) -> Result<bool> {
        crate::cli::quietly(writeln!(self.out, "{value}").and_then(|()| self.out.flush()))
    }

    fn finished(&self) -> Result<()> {
        if self.failed { Err(Error::Partial) } else { Ok(()) }
    }
}

fn numbered(mut value: Value, first: usize, events: usize) -> Value {
    if let Some(object) = value.as_object_mut() {
        for skip in object.get_mut("skipped").and_then(Value::as_array_mut).into_iter().flatten() {
            renumber(skip, first);
        }
        object.insert("events".into(), json!(events));
    }
    value
}

fn renumber(skip: &mut Value, first: usize) {
    let at = skip.get("event").and_then(Value::as_u64);
    if let (Some(at), Some(object)) = (at, skip.as_object_mut()) {
        object.insert("event".into(), json!(at + first as u64));
    }
}

fn which(first: usize, events: usize) -> String {
    if events == 1 {
        format!("event {} was", first + 1)
    } else {
        format!("events {} to {} were", first + 1, first + events)
    }
}

#[cfg(test)]
mod tests;
