use std::io;

use crate::os::memory::{self, Memory};
use crate::text;

pub(crate) const OVERRIDE: &str = "CORNERCASE_MEMORY";

pub(crate) type Reader = fn() -> io::Result<Memory>;

pub(crate) fn read() -> io::Result<Memory> {
    match std::env::var(OVERRIDE).ok().filter(|value| !value.trim().is_empty()) {
        Some(value) => given(&value).ok_or_else(|| io::Error::other(format!("{OVERRIDE} is not `total,available`"))),
        None => memory::read(),
    }
}

pub(crate) fn given(value: &str) -> Option<Memory> {
    let (total, available) = value.split_once(',')?;
    let total = text::parse_size(total)?;
    Some(Memory { total, available: text::parse_size(available)?.min(total) })
}

pub(crate) fn shown(bytes: u64) -> String {
    let size = text::size(bytes);
    match size.strip_suffix(" GB").and_then(|amount| amount.strip_suffix(".0")) {
        Some(whole) => format!("{whole} GB"),
        None => size,
    }
}

pub(crate) fn free_of(available: u64, total: u64) -> String {
    format!("{} free of {}", shown(available), shown(total))
}

#[cfg(test)]
mod tests;
