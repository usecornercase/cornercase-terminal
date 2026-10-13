use std::fmt::Write as _;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

pub(crate) const LIMIT: usize = 4000;
pub(crate) const MAX: usize = 4096;
const MOST: usize = 4;
const CUT: &str = "… the rest is in cornercase";

pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

pub(crate) fn length(text: &str) -> usize {
    text.encode_utf16().count()
}

pub(crate) fn plain(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut tag = false;
    for c in html.chars() {
        match c {
            '<' => tag = true,
            '>' if tag => tag = false,
            c if !tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&")
}

pub(crate) fn pre(text: &str, room: usize) -> String {
    format!("<pre>{}</pre>", escape(&tail(text, room.min(LIMIT))))
}

pub(crate) fn room(header: &str) -> usize {
    MAX.saturating_sub(length(&plain(header)) + 2)
}

pub(crate) fn fits(header: &str, part: &str) -> bool {
    length(&plain(part)) <= room(header)
}

pub(crate) fn tail(text: &str, limit: usize) -> String {
    let mut kept: Vec<&str> = Vec::new();
    let mut used = 0;
    for line in text.lines().rev() {
        let size = length(line) + 1;
        if used + size > limit {
            break;
        }
        used += size;
        kept.push(line);
    }
    kept.reverse();
    kept.join("\n")
}

pub(crate) fn markdown(source: &str) -> Vec<String> {
    pack(blocks(source))
}

#[derive(Debug, Default)]
struct Block {
    html: String,
    text: String,
    code: bool,
}

impl Block {
    fn text(&mut self, text: &str) {
        self.html.push_str(&escape(text));
        self.text.push_str(text);
    }

    fn tag(&mut self, tag: &str) {
        self.html.push_str(tag);
    }

    fn line(&mut self) {
        if !self.text.is_empty() && !self.text.ends_with('\n') {
            self.text("\n");
        }
    }

    fn done(mut self) -> Option<Self> {
        let trimmed = self.html.trim_end_matches('\n').len();
        self.html.truncate(trimmed);
        let trimmed = self.text.trim_end_matches('\n').len();
        self.text.truncate(trimmed);
        (!self.text.trim().is_empty()).then_some(self)
    }
}

#[derive(Default)]
struct Builder {
    blocks: Vec<Block>,
    block: Block,
    level: usize,
    lists: Vec<Option<u64>>,
    links: Vec<bool>,
    fenced: bool,
}

impl Builder {
    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph | Tag::HtmlBlock => self.level += 1,
            Tag::Heading { .. } => {
                self.level += 1;
                self.block.tag("<b>");
            }
            Tag::CodeBlock(kind) => {
                self.level += 1;
                self.block.code = self.level == 1;
                self.block.line();
                let language = match kind {
                    CodeBlockKind::Fenced(lang) => lang.split_whitespace().next().unwrap_or_default().to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.fenced = !language.is_empty() && language.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
                if self.fenced {
                    self.block.tag(&format!("<pre><code class=\"language-{language}\">"));
                } else {
                    self.block.tag("<pre>");
                }
            }
            Tag::List(first) => {
                self.level += 1;
                self.block.line();
                self.lists.push(first);
            }
            Tag::Item => {
                self.level += 1;
                self.block.line();
                let indent = "  ".repeat(self.lists.len().saturating_sub(1));
                let marker = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        *n += 1;
                        format!("{}. ", *n - 1)
                    }
                    _ => "• ".into(),
                };
                self.block.text(&format!("{indent}{marker}"));
            }
            Tag::BlockQuote(_) => {
                self.level += 1;
                self.block.tag("<blockquote>");
            }
            Tag::Emphasis => self.block.tag("<i>"),
            Tag::Strong => self.block.tag("<b>"),
            Tag::Strikethrough => self.block.tag("<s>"),
            Tag::Link { dest_url, .. } => {
                let web = dest_url.starts_with("https://") || dest_url.starts_with("http://");
                if web {
                    self.block.tag(&format!("<a href=\"{}\">", escape(&dest_url)));
                }
                self.links.push(web);
            }
            _ => {}
        }
    }

    fn end(&mut self, end: TagEnd) {
        match end {
            TagEnd::Paragraph => {
                self.leave();
                if self.level > 0 {
                    self.block.line();
                }
            }
            TagEnd::HtmlBlock => self.leave(),
            TagEnd::Heading(_) => {
                self.block.tag("</b>");
                self.leave();
            }
            TagEnd::CodeBlock => {
                let trimmed = self.block.html.trim_end_matches('\n').len();
                self.block.html.truncate(trimmed);
                self.block.tag(if self.fenced { "</code></pre>" } else { "</pre>" });
                self.block.text("\n");
                self.leave();
            }
            TagEnd::List(_) => {
                self.lists.pop();
                self.leave();
            }
            TagEnd::Item => {
                self.block.line();
                self.leave();
            }
            TagEnd::BlockQuote(_) => {
                let trimmed = self.block.html.trim_end_matches('\n').len();
                self.block.html.truncate(trimmed);
                self.block.tag("</blockquote>");
                self.leave();
            }
            TagEnd::Emphasis => self.block.tag("</i>"),
            TagEnd::Strong => self.block.tag("</b>"),
            TagEnd::Strikethrough => self.block.tag("</s>"),
            TagEnd::Link => self.close_link(),
            _ => {}
        }
    }

    fn close_link(&mut self) {
        if self.links.pop() == Some(true) {
            self.block.tag("</a>");
        }
    }

    fn leave(&mut self) {
        self.level = self.level.saturating_sub(1);
        if self.level == 0 {
            let block = std::mem::take(&mut self.block);
            self.blocks.extend(block.done());
        }
    }

    fn event(&mut self, event: Event) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(end) => self.end(end),
            Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => self.block.text(&text),
            Event::Code(code) => {
                self.block.tag("<code>");
                self.block.text(&code);
                self.block.tag("</code>");
            }
            Event::SoftBreak | Event::HardBreak => self.block.text("\n"),
            Event::Rule => {
                self.block.line();
                self.block.text("———\n");
                if self.level == 0 {
                    self.blocks.extend(std::mem::take(&mut self.block).done());
                }
            }
            Event::TaskListMarker(done) => self.block.text(if done { "☑ " } else { "☐ " }),
            _ => {}
        }
    }
}

fn blocks(source: &str) -> Vec<Block> {
    let mut builder = Builder::default();
    for event in Parser::new_ext(source, Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS) {
        builder.event(event);
    }
    let Builder { mut blocks, block, .. } = builder;
    blocks.extend(block.done());
    blocks
}

fn cut(line: &str, limit: usize) -> usize {
    let mut units = 0;
    for (i, c) in line.char_indices() {
        units += c.len_utf16();
        if units > limit {
            return i;
        }
    }
    line.len()
}

fn chunks(text: &str, limit: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for line in text.split('\n') {
        let mut line = line;
        while length(line) > limit {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            let at = cut(line, limit);
            out.push(line[..at].to_string());
            line = &line[at..];
        }
        if !current.is_empty() && length(&current) + 1 + length(line) > limit {
            out.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

fn pack(blocks: Vec<Block>) -> Vec<String> {
    let mut pieces: Vec<(String, usize)> = Vec::new();
    for block in blocks {
        let size = length(&block.text);
        if size <= LIMIT {
            pieces.push((block.html, size));
            continue;
        }
        let wrap = |chunk: &str| if block.code { format!("<pre>{}</pre>", escape(chunk)) } else { escape(chunk) };
        pieces.extend(chunks(&block.text, LIMIT).iter().map(|chunk| (wrap(chunk), length(chunk))));
    }
    let mut messages: Vec<(String, usize)> = Vec::new();
    for (html, size) in pieces {
        match messages.last_mut() {
            Some((message, used)) if *used + 2 + size <= LIMIT => {
                message.push_str("\n\n");
                message.push_str(&html);
                *used += 2 + size;
            }
            _ => messages.push((html, size)),
        }
    }
    if messages.len() > MOST {
        messages.truncate(MOST);
        if let Some((message, _)) = messages.last_mut() {
            let _ = write!(message, "\n\n<i>{CUT}</i>");
        }
    }
    messages.into_iter().map(|(message, _)| message).collect()
}

#[cfg(test)]
mod tests;
