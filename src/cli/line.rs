use std::cell::RefCell;
use std::io::{self, BufRead, Cursor};

use serde_json::Value;

use crate::error::Result;

thread_local! {
    static LINE: RefCell<Option<Line>> = const { RefCell::new(None) };
}

struct Line {
    stdin: Option<String>,
    printed: Printed,
}

#[derive(Default)]
pub(super) struct Printed {
    pub(super) json: Option<Value>,
    pub(super) text: String,
    pub(super) stderr: String,
}

pub(super) fn run(stdin: Option<String>, body: impl FnOnce() -> Result<bool>) -> (Result<bool>, Printed) {
    LINE.set(Some(Line { stdin, printed: Printed::default() }));
    let result = body();
    let printed = LINE.take().map(|line| line.printed).unwrap_or_default();
    (result, printed)
}

fn within<T>(then: impl FnOnce(&mut Line) -> T) -> Option<T> {
    LINE.with_borrow_mut(|line| line.as_mut().map(then))
}

pub(super) fn input() -> Box<dyn BufRead> {
    match within(|line| line.stdin.take().unwrap_or_default()) {
        Some(text) => Box::new(Cursor::new(text.into_bytes())),
        None => Box::new(io::stdin().lock()),
    }
}

pub(super) fn json(pretty: &str) -> bool {
    within(|line| match (&line.printed.json, serde_json::from_str(pretty)) {
        (None, Ok(value)) => line.printed.json = Some(value),
        _ => line.printed.text.push_str(pretty),
    })
    .is_some()
}

pub(super) fn text(text: &str) -> bool {
    within(|line| line.printed.text.push_str(text)).is_some()
}

pub(super) fn stderr(text: &str) -> bool {
    within(|line| line.printed.stderr.push_str(text)).is_some()
}

#[cfg(test)]
mod tests;
