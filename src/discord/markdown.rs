use crate::messenger::{Markup, markup};

const LIMIT: usize = 1900;
pub(super) const MAX: usize = 2000;
const MOST: usize = 4;
const CUT: &str = "*… the rest is in cornercase*";
const SPECIAL: &str = "\\*_~`|>#-[]()<:";
const FENCE: &str = "```";
const BROKEN_FENCE: &str = "`\u{200b}``";

pub(crate) struct Markdown;

impl Markup for Markdown {
    fn escape(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for c in text.chars() {
            if SPECIAL.contains(c) {
                out.push('\\');
            }
            out.push(c);
        }
        out
    }

    fn bold(&self, text: &str) -> String {
        format!("**{text}**")
    }

    fn pre(&self, text: &str, room: usize) -> String {
        let room = room.min(LIMIT).saturating_sub(length(FENCE) * 2 + 2);
        format!("{FENCE}\n{}\n{FENCE}", markup::tail(&text.replace(FENCE, BROKEN_FENCE), room, length))
    }

    fn room(&self, header: &str) -> usize {
        MAX.saturating_sub(length(header) + 2)
    }

    fn fits(&self, header: &str, part: &str) -> bool {
        length(part) <= self.room(header)
    }

    fn markdown(&self, source: &str) -> Vec<String> {
        markdown(source)
    }

    fn lines(&self, lines: &[String]) -> Vec<String> {
        markup::pack(lines, LIMIT, length)
    }
}

pub(super) fn length(text: &str) -> usize {
    text.chars().count()
}

#[derive(Debug, Default)]
struct Block {
    lines: Vec<String>,
    fence: Option<String>,
    gap: bool,
}

impl Block {
    fn opener(&self) -> Option<String> {
        self.fence.as_deref().map(shown)
    }

    fn body(&self) -> Vec<String> {
        match &self.fence {
            Some(open) if open.starts_with('~') => {
                self.lines.iter().map(|line| line.replace(FENCE, BROKEN_FENCE)).collect()
            }
            _ => self.lines.clone(),
        }
    }

    fn text(&self) -> String {
        match self.opener() {
            Some(open) => format!("{open}\n{}\n{}", self.body().join("\n"), closer(&open)),
            None => self.lines.join("\n"),
        }
    }
}

fn shown(open: &str) -> String {
    let tildes = open.chars().take_while(|c| *c == '~').count();
    format!("{}{}", "`".repeat(tildes), &open[tildes..])
}

fn closer(open: &str) -> String {
    let mark = open.trim_start().chars().next().unwrap_or('`');
    open.trim_start().chars().take_while(|c| *c == mark).collect()
}

fn opens(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("```") || line.starts_with("~~~")
}

fn closes(line: &str, open: &str) -> bool {
    let line = line.trim();
    let close = closer(open);
    line.starts_with(&close) && line.chars().all(|c| close.starts_with(c))
}

fn blocks(source: &str) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut block = Block::default();
    let mut gap = false;
    for line in source.lines() {
        if let Some(open) = &block.fence {
            if closes(line, open) {
                blocks.push(std::mem::take(&mut block));
            } else {
                block.lines.push(line.to_string());
            }
            continue;
        }
        if opens(line) {
            if !block.lines.is_empty() {
                blocks.push(std::mem::take(&mut block));
            }
            block = Block { lines: Vec::new(), fence: Some(line.trim_start().to_string()), gap };
            gap = false;
        } else if line.trim().is_empty() {
            if !block.lines.is_empty() {
                blocks.push(std::mem::take(&mut block));
            }
            gap = true;
        } else {
            if block.lines.is_empty() {
                block.gap = gap;
                gap = false;
            }
            block.lines.push(line.to_string());
        }
    }
    if !block.lines.is_empty() || block.fence.is_some() {
        blocks.push(block);
    }
    blocks
}

fn cut(line: &str, limit: usize) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return vec![String::new()];
    }
    chars.chunks(limit.max(1)).map(|chunk| chunk.iter().collect()).collect()
}

fn chunks(lines: &[String], limit: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    for piece in lines.iter().flat_map(|line| cut(line, limit)) {
        current = match current {
            Some(mut text) if length(&text) + 1 + length(&piece) <= limit => {
                text.push('\n');
                text.push_str(&piece);
                Some(text)
            }
            Some(text) => {
                out.push(text);
                Some(piece)
            }
            None => Some(piece),
        };
    }
    out.extend(current);
    out
}

fn pieces(block: &Block) -> Vec<String> {
    let text = block.text();
    if length(&text) <= LIMIT {
        return vec![text];
    }
    match block.opener() {
        Some(open) => {
            let close = closer(&open);
            let room = LIMIT.saturating_sub(length(&open) + length(&close) + 2);
            chunks(&block.body(), room).into_iter().map(|chunk| format!("{open}\n{chunk}\n{close}")).collect()
        }
        None => chunks(&block.lines, LIMIT),
    }
}

fn markdown(source: &str) -> Vec<String> {
    let mut messages: Vec<String> = Vec::new();
    for block in blocks(source) {
        let gap = if block.gap { "\n\n" } else { "\n" };
        let mut parts = pieces(&block).into_iter();
        let Some(first) = parts.next() else { continue };
        match messages.last_mut() {
            Some(message) if length(message) + length(gap) + length(&first) <= LIMIT => {
                message.push_str(gap);
                message.push_str(&first);
            }
            _ => messages.push(first),
        }
        messages.extend(parts);
    }
    messages.retain(|message| !message.trim().is_empty());
    if messages.len() > MOST {
        messages.truncate(MOST);
        if let Some(message) = messages.last_mut() {
            message.push_str("\n\n");
            message.push_str(CUT);
        }
    }
    messages
}

#[cfg(test)]
mod tests;
