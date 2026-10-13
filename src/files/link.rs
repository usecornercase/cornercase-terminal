use std::path::{Path, PathBuf};

use crate::config::expand_home;
use crate::term::emulator::Cell;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Link {
    pub(crate) start: u16,
    pub(crate) end: u16,
    pub(crate) path: String,
    pub(crate) lines: Option<(u32, u32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) path: String,
    pub(crate) lines: Option<(u32, u32)>,
}

const TRAILING: [char; 5] = ['.', ',', ':', ';', '#'];

fn path_char(symbol: &str) -> bool {
    let mut chars = symbol.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => c.is_alphanumeric() || "_-./~@+,%:#".contains(c),
        _ => false,
    }
}

fn number(text: &str) -> Option<u32> {
    (!text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())).then(|| text.parse().ok()).flatten()
}

fn location(rest: &str) -> Option<(u32, u32)> {
    let (first, tail) =
        rest.split_once(['-', ':']).map_or((rest, None), |(a, b)| (a, Some((b, rest.as_bytes()[a.len()]))));
    let first = number(first)?;
    match tail {
        Some((last, b'-')) => Some((first, number(last.trim_start_matches('L'))?.max(first))),
        _ => Some((first, first)),
    }
}

fn split(token: &str) -> (&str, Option<(u32, u32)>) {
    if let Some((path, rest)) = token.rsplit_once("#L") {
        return (path, location(rest));
    }
    let name = token.rfind('/').map_or(0, |i| i + 1);
    let colon = token[name..].find(':').map(|i| name + i);
    match colon.and_then(|i| Some((i, location(&token[i + 1..])?))) {
        Some((i, lines)) => (&token[..i], Some(lines)),
        None => (token, None),
    }
}

pub(crate) fn at(row: &[Cell], col: u16) -> Option<Link> {
    let col = usize::from(col);
    if !row.get(col).is_some_and(|c| path_char(&c.symbol)) {
        return None;
    }
    let start = (0..col).rev().take_while(|&i| path_char(&row[i].symbol)).last().unwrap_or(col);
    let mut end = (col..row.len()).take_while(|&i| path_char(&row[i].symbol)).last().map_or(col, |i| i + 1);
    let mut token: String = row[start..end].iter().map(|c| c.symbol.as_str()).collect();
    while token.ends_with(TRAILING) {
        token.pop();
        end -= 1;
    }
    if col >= end || token.contains("://") {
        return None;
    }
    let (path, lines) = split(&token);
    if path.is_empty() || path.chars().all(|c| c == '.' || c == '/') {
        return None;
    }
    let column = |i: usize| u16::try_from(i).unwrap_or(u16::MAX);
    Some(Link { start: column(start), end: column(end), path: path.to_string(), lines })
}

pub(crate) fn resolve(link: &Link, cwd: Option<&Path>, root: &Path, home: Option<&Path>) -> Option<Target> {
    let root = root.canonicalize().ok()?;
    let given = expand_home(&link.path, home);
    let candidates: Vec<PathBuf> = if given.is_absolute() {
        vec![given]
    } else {
        cwd.map(|dir| dir.join(&given)).into_iter().chain(std::iter::once(root.join(&given))).collect()
    };
    candidates.into_iter().find_map(|candidate| {
        let real = candidate.canonicalize().ok().filter(|p| p.is_file())?;
        let path = real.strip_prefix(&root).unwrap_or(&real).to_string_lossy().into_owned();
        Some(Target { path, lines: link.lines })
    })
}

#[cfg(test)]
mod tests;
