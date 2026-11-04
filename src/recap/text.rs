use std::collections::BTreeMap;

use super::Place;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Segment {
    pub(crate) text: String,
    pub(crate) bold: bool,
    pub(crate) quote: bool,
    pub(crate) link: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Heading,
    Bullet,
    Paragraph,
    Blank,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Block {
    pub(crate) kind: Kind,
    pub(crate) segments: Vec<Segment>,
}

enum Piece<'a> {
    Text(&'a str),
    Id(&'a str),
}

fn is_id(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(chars.next(), Some('c' | 'p')) && chars.clone().next().is_some() && chars.all(|c| c.is_ascii_digit())
}

fn pieces(line: &str) -> Vec<Piece<'_>> {
    let mut found = Vec::new();
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        if let Some(close) = after.find(']').filter(|close| is_id(&after[..*close])) {
            found.push(Piece::Text(&rest[..open]));
            found.push(Piece::Id(&after[..close]));
            rest = &after[close + 1..];
        } else {
            found.push(Piece::Text(&rest[..=open]));
            rest = after;
        }
    }
    found.push(Piece::Text(rest));
    found
}

pub(crate) fn label(id: &str, place: &Place) -> String {
    if let ("pull", Some(number)) = (place.kind.as_str(), place.number) {
        return format!("#{number}");
    }
    let name = place.name();
    if name.is_empty() { id.to_string() } else { name }
}

pub(crate) fn plain(text: &str, places: &BTreeMap<String, Place>) -> String {
    text.lines()
        .map(|line| {
            pieces(line)
                .into_iter()
                .map(|piece| match piece {
                    Piece::Text(text) => text.to_string(),
                    Piece::Id(id) => places.get(id).map_or_else(|| format!("[{id}]"), |place| label(id, place)),
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

struct Styles {
    bold: bool,
    quote: bool,
}

fn styled(text: &str, styles: &mut Styles, out: &mut Vec<Segment>) {
    let mut current = String::new();
    let flush = |current: &mut String, styles: &Styles, out: &mut Vec<Segment>| {
        if !current.is_empty() {
            out.push(Segment { text: std::mem::take(current), bold: styles.bold, quote: styles.quote, link: None });
        }
    };
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '*' && chars.peek() == Some(&'*') {
            chars.next();
            flush(&mut current, styles, out);
            styles.bold = !styles.bold;
        } else if c == '"' {
            if styles.quote {
                current.push(c);
                flush(&mut current, styles, out);
                styles.quote = false;
            } else {
                flush(&mut current, styles, out);
                styles.quote = true;
                current.push(c);
            }
        } else {
            current.push(c);
        }
    }
    flush(&mut current, styles, out);
}

fn segments(line: &str, places: &BTreeMap<String, Place>) -> Vec<Segment> {
    let mut styles = Styles { bold: false, quote: false };
    let mut out = Vec::new();
    for piece in pieces(line) {
        match piece {
            Piece::Text(text) => styled(text, &mut styles, &mut out),
            Piece::Id(id) => match places.get(id) {
                Some(place) => out.push(Segment {
                    text: label(id, place),
                    bold: styles.bold,
                    quote: styles.quote,
                    link: Some(id.to_string()),
                }),
                None => styled(&format!("[{id}]"), &mut styles, &mut out),
            },
        }
    }
    out
}

fn block(line: &str, places: &BTreeMap<String, Place>) -> Block {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return Block { kind: Kind::Blank, segments: Vec::new() };
    }
    if trimmed.starts_with('#') {
        let title = trimmed.trim_start_matches('#').trim();
        return Block { kind: Kind::Heading, segments: segments(title, places) };
    }
    for marker in ["- ", "* ", "• "] {
        if let Some(item) = trimmed.strip_prefix(marker) {
            return Block { kind: Kind::Bullet, segments: segments(item, places) };
        }
    }
    Block { kind: Kind::Paragraph, segments: segments(trimmed, places) }
}

pub(crate) fn parse(text: &str, places: &BTreeMap<String, Place>) -> Vec<Block> {
    let mut blocks: Vec<Block> = text.lines().map(|line| block(line, places)).collect();
    while blocks.first().is_some_and(|b| b.kind == Kind::Blank) {
        blocks.remove(0);
    }
    while blocks.last().is_some_and(|b| b.kind == Kind::Blank) {
        blocks.pop();
    }
    blocks.dedup_by(|a, b| a.kind == Kind::Blank && b.kind == Kind::Blank);
    blocks
}

#[cfg(test)]
mod tests;
