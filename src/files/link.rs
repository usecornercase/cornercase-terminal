use std::path::{Path, PathBuf};

use crate::config::expand_home;
use crate::os::fs::{reaches_network, simplified};
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
    pub(crate) real: PathBuf,
    pub(crate) lines: Option<(u32, u32)>,
}

impl Target {
    pub(crate) fn inside(&self) -> bool {
        Path::new(&self.path).is_relative()
    }
}

const TRAILING: [char; 5] = ['.', ',', ':', ';', '#'];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Separators {
    Slash,
    Both,
}

impl Separators {
    fn here() -> Self {
        if cfg!(windows) { Self::Both } else { Self::Slash }
    }

    fn matches(self, c: char) -> bool {
        c == '/' || (self == Self::Both && c == '\\')
    }
}

fn path_char(symbol: &str, separators: Separators) -> bool {
    let mut chars = symbol.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => c.is_alphanumeric() || "_-./~@+,%:#".contains(c) || separators.matches(c),
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

fn split(token: &str, separators: Separators) -> (&str, Option<(u32, u32)>) {
    if let Some((path, rest)) = token.rsplit_once("#L") {
        return (path, location(rest));
    }
    let name = token.rfind(|c| separators.matches(c)).map_or(0, |i| i + 1);
    let colon = token[name..].find(':').map(|i| name + i);
    match colon.and_then(|i| Some((i, location(&token[i + 1..])?))) {
        Some((i, lines)) => (&token[..i], Some(lines)),
        None => (token, None),
    }
}

pub(crate) fn at(row: &[Cell], col: u16) -> Option<Link> {
    at_with(row, col, Separators::here())
}

fn at_with(row: &[Cell], col: u16, separators: Separators) -> Option<Link> {
    let col = usize::from(col);
    let in_path = |i: usize| path_char(&row[i].symbol, separators);
    if col >= row.len() || !in_path(col) {
        return None;
    }
    let start = (0..col).rev().take_while(|&i| in_path(i)).last().unwrap_or(col);
    let mut end = (col..row.len()).take_while(|&i| in_path(i)).last().map_or(col, |i| i + 1);
    let mut token: String = row[start..end].iter().map(|c| c.symbol.as_str()).collect();
    while token.ends_with(TRAILING) {
        token.pop();
        end -= 1;
    }
    if col >= end || token.contains("://") {
        return None;
    }
    let (path, lines) = split(&token, separators);
    if path.is_empty() || path.chars().all(|c| c == '.' || separators.matches(c)) {
        return None;
    }
    let column = |i: usize| u16::try_from(i).unwrap_or(u16::MAX);
    Some(Link { start: column(start), end: column(end), path: path.to_string(), lines })
}

pub(crate) fn resolve(link: &Link, cwd: Option<&Path>, root: &Path, home: Option<&Path>) -> Option<Target> {
    if reaches_network(&link.path) {
        return None;
    }
    let root = std::fs::canonicalize(root).ok()?;
    let given = expand_home(&link.path, home);
    let candidates: Vec<PathBuf> = if given.is_absolute() {
        vec![given]
    } else {
        cwd.map(|dir| dir.join(&given)).into_iter().chain(std::iter::once(root.join(&given))).collect()
    };
    candidates.into_iter().find_map(|candidate| {
        let real = std::fs::canonicalize(&candidate).ok().filter(|p| p.is_file())?;
        let inside = real.strip_prefix(&root).ok().map(|inside| inside.to_string_lossy().into_owned());
        let real = simplified(real);
        let path = inside.unwrap_or_else(|| real.to_string_lossy().into_owned());
        Some(Target { path, real, lines: link.lines })
    })
}

pub(crate) fn under(
    row: &[Cell],
    col: u16,
    cwd: Option<&Path>,
    root: &Path,
    home: Option<&Path>,
) -> Option<(Link, Target)> {
    let found = at(row, col)?;
    let target = resolve(&found, cwd, root, home)?;
    Some((found, target))
}

#[cfg(test)]
mod tests;
