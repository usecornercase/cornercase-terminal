use std::fmt::Write;
use std::ops::Range;
use std::path::Path;

use regex::{Regex, RegexBuilder};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub(crate) const CRUMB_SEPARATOR: &str = " › ";

pub(crate) fn folder_name(path: &Path, home: Option<&Path>) -> String {
    if home == Some(path) {
        return "~".into();
    }
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

pub(crate) fn display_path(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

pub(crate) fn truncate_right(s: &str, max: usize) -> String {
    if s.chars().count() <= max || max < 2 {
        return s.to_string();
    }
    let head: String = s.chars().take(max - 1).collect();
    format!("{}…", head.trim_end_matches('…'))
}

pub(crate) fn cells(s: &str) -> usize {
    s.width()
}

pub(crate) fn truncate_cells(s: &str, max: usize) -> String {
    if cells(s) <= max || max < 2 {
        return s.to_string();
    }
    let mut used = 0;
    let head: String = s
        .chars()
        .take_while(|ch| {
            used += ch.width().unwrap_or(0);
            used < max
        })
        .collect();
    format!("{}…", head.trim_end_matches('…'))
}

pub(crate) fn truncate_left(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max || max < 2 {
        return s.to_string();
    }
    let tail: String = s.chars().skip(n - (max - 1)).collect();
    format!("…{}", tail.trim_start_matches('…'))
}

pub(crate) fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Debug, Clone)]
pub(crate) struct Needle(Regex);

impl Needle {
    pub(crate) fn new(query: &str) -> Option<Self> {
        let query = query.trim();
        if query.is_empty() {
            return None;
        }
        RegexBuilder::new(&regex::escape(query)).case_insensitive(true).build().ok().map(Self)
    }

    pub(crate) fn find(&self, text: &str) -> Option<Range<usize>> {
        self.0.find(text).map(|m| m.range())
    }
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

#[cfg(test)]
mod tests;
