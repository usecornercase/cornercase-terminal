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

pub(crate) fn clip_cells(s: &str, max: usize) -> String {
    let mut used = 0;
    s.chars()
        .take_while(|ch| {
            used += ch.width().unwrap_or(0);
            used <= max
        })
        .collect()
}

pub(crate) fn wrap_cells(s: &str, max: usize) -> Vec<String> {
    let (mut rows, mut row, mut used) = (Vec::new(), String::new(), 0);
    for ch in s.chars() {
        let width = ch.width().unwrap_or(0);
        if max > 0 && used > 0 && used + width > max {
            rows.push(std::mem::take(&mut row));
            used = 0;
        }
        used += width;
        row.push(ch);
    }
    rows.push(row);
    rows
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

pub(crate) fn hidden(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{ad}'
                | '\u{34f}'
                | '\u{600}'..='\u{605}'
                | '\u{61c}'
                | '\u{6dd}'
                | '\u{70f}'
                | '\u{890}'..='\u{891}'
                | '\u{8e2}'
                | '\u{115f}'..='\u{1160}'
                | '\u{17b4}'..='\u{17b5}'
                | '\u{180b}'..='\u{180f}'
                | '\u{200b}'..='\u{200f}'
                | '\u{2028}'..='\u{202e}'
                | '\u{2060}'..='\u{206f}'
                | '\u{3164}'
                | '\u{fe00}'..='\u{fe0f}'
                | '\u{feff}'
                | '\u{ffa0}'
                | '\u{fff0}'..='\u{fffb}'
                | '\u{110bd}'
                | '\u{110cd}'
                | '\u{13430}'..='\u{1343f}'
                | '\u{1bca0}'..='\u{1bca3}'
                | '\u{1d173}'..='\u{1d17a}'
                | '\u{e0000}'..='\u{e0fff}'
        )
}

pub(crate) fn escape_hidden(text: &str) -> String {
    text.chars().fold(String::with_capacity(text.len()), |mut out, c| {
        if hidden(c) {
            let _ = write!(out, "\\u{{{:x}}}", u32::from(c));
        } else {
            out.push(c);
        }
        out
    })
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

#[cfg(test)]
mod tests;
