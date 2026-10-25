use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::highlight::syntax;

const MIN_WIDTH: usize = 10;
const MIN_CELL: usize = 3;
const DIM: Style = Style::new().fg(Color::DarkGray);
const CODE: Style = Style::new().fg(Color::Yellow);
const REFERENCE: Style = Style::new().fg(Color::Green);
const MENTION: Style = Style::new().fg(Color::Cyan);
const DONE: Style = Style::new().fg(Color::Green);
const CELL_SEPARATOR: &str = " │ ";

type Segment = (String, Style);

pub(crate) fn render(source: &str, width: usize) -> Vec<Line<'static>> {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut renderer = Renderer::new(width.max(MIN_WIDTH));
    for event in Parser::new_ext(source, options) {
        renderer.event(event);
    }
    renderer.finish()
}

pub(crate) fn warm(source: &str) {
    let mut code: Option<(String, String)> = None;
    for event in Parser::new_ext(source, Options::empty()) {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => code = Some((info.to_string(), String::new())),
            Event::Text(text) => {
                if let Some((_, body)) = &mut code {
                    body.push_str(&text);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((info, body)) = code.take() {
                    syntax::paint_fence(body.trim_end_matches('\n'), &info);
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn wrap_text(text: &str, style: Style, width: usize) -> Vec<Line<'static>> {
    wrap_segments(&[(text.to_string(), style)], width)
}

pub(crate) fn wrap_segments(segments: &[(String, Style)], width: usize) -> Vec<Line<'static>> {
    wrap(segments, width.max(MIN_WIDTH), Vec::new(), &[])
}

struct Item {
    indent: usize,
    marker: Option<Vec<Segment>>,
}

#[derive(Default)]
struct Table {
    rows: Vec<Vec<Vec<Segment>>>,
    header_rows: usize,
}

struct Renderer {
    width: usize,
    lines: Vec<Line<'static>>,
    inline: Vec<Segment>,
    styles: Vec<Style>,
    quote: usize,
    lists: Vec<Option<u64>>,
    items: Vec<Item>,
    code: Option<(String, String)>,
    links: Vec<(usize, String)>,
    table: Option<Table>,
    in_comment: bool,
}

impl Renderer {
    fn new(width: usize) -> Self {
        Self {
            width,
            lines: Vec::new(),
            inline: Vec::new(),
            styles: Vec::new(),
            quote: 0,
            lists: Vec::new(),
            items: Vec::new(),
            code: None,
            links: Vec::new(),
            table: None,
            in_comment: false,
        }
    }

    fn style(&self) -> Style {
        self.styles.last().copied().unwrap_or_default()
    }

    fn push_style(&mut self, style: Style) {
        self.styles.push(self.style().patch(style));
    }

    fn event(&mut self, event: Event) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.text(&text),
            Event::Code(code) => self.push(code.to_string(), self.style().patch(CODE)),
            Event::Html(html) | Event::InlineHtml(html) => {
                let text = self.strip_html(&html);
                self.text(&text);
            }
            Event::SoftBreak | Event::HardBreak => self.push("\n".into(), Style::default()),
            Event::Rule => self.rule(),
            Event::TaskListMarker(done) => self.task_marker(done),
            _ => {}
        }
    }

    fn rule(&mut self) {
        self.flush();
        self.gap();
        let prefix = self.prefix(false);
        let room = self.width.saturating_sub(spans_width(&prefix));
        let mut spans = prefix;
        spans.push(Span::styled("─".repeat(room), DIM));
        self.lines.push(Line::from(spans));
    }

    fn task_marker(&mut self, done: bool) {
        if let Some(item) = self.items.last_mut() {
            let (mark, style) = if done { ("[x] ", DONE) } else { ("[ ] ", Style::default()) };
            item.marker.get_or_insert_with(Vec::new).push((mark.into(), style));
            item.indent += mark.chars().count();
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::BlockQuote(_)
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::Table(_) => self.start_block(tag),
            Tag::List(_) | Tag::Item => self.start_list(&tag),
            Tag::TableHead | Tag::TableRow | Tag::TableCell => self.start_table_part(&tag),
            Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. } | Tag::Image { .. } => {
                self.start_inline(tag);
            }
            _ => {}
        }
    }

    fn start_block(&mut self, tag: Tag) {
        self.flush();
        match tag {
            Tag::Paragraph => {
                if self.items.last().is_none_or(|i| i.marker.is_none()) {
                    self.gap();
                }
            }
            Tag::Heading { .. } => {
                self.gap();
                self.push_style(Style::new().add_modifier(Modifier::BOLD));
            }
            Tag::BlockQuote(_) => {
                self.gap();
                self.quote += 1;
            }
            Tag::CodeBlock(kind) => self.start_code(&kind),
            Tag::HtmlBlock | Tag::Table(_) => {
                self.gap();
                if matches!(tag, Tag::Table(_)) {
                    self.table = Some(Table::default());
                }
            }
            _ => {}
        }
    }

    fn start_code(&mut self, kind: &CodeBlockKind) {
        self.gap();
        let language = match kind {
            CodeBlockKind::Fenced(info) => info.trim().to_string(),
            CodeBlockKind::Indented => String::new(),
        };
        if !language.is_empty() {
            let mut spans = self.prefix(false);
            spans.push(Span::styled(language.clone(), DIM));
            self.lines.push(Line::from(spans));
        }
        self.code = Some((language, String::new()));
    }

    fn start_list(&mut self, tag: &Tag) {
        self.flush();
        match tag {
            Tag::List(first) => {
                if self.items.is_empty() {
                    self.gap();
                }
                self.lists.push(*first);
            }
            Tag::Item => {
                let base = self.items.last().map_or(0, |i| i.indent);
                let marker = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        let marker = format!("{n}. ");
                        *n += 1;
                        marker
                    }
                    _ => "• ".into(),
                };
                let indent = base + marker.chars().count();
                self.items.push(Item { indent, marker: Some(vec![(marker, Style::default())]) });
            }
            _ => {}
        }
    }

    fn start_table_part(&mut self, tag: &Tag) {
        match tag {
            Tag::TableHead | Tag::TableRow => self.start_row(matches!(tag, Tag::TableHead)),
            Tag::TableCell => {
                if let Some(row) = self.table.as_mut().and_then(|t| t.rows.last_mut()) {
                    row.push(Vec::new());
                }
            }
            _ => {}
        }
    }

    fn start_row(&mut self, head: bool) {
        let Some(table) = &mut self.table else { return };
        table.rows.push(Vec::new());
        if head {
            table.header_rows = table.rows.len();
            self.push_style(Style::new().add_modifier(Modifier::BOLD));
        }
    }

    fn start_inline(&mut self, tag: Tag) {
        match tag {
            Tag::Emphasis => self.push_style(Style::new().add_modifier(Modifier::ITALIC)),
            Tag::Strong => self.push_style(Style::new().add_modifier(Modifier::BOLD)),
            Tag::Strikethrough => self.push_style(Style::new().add_modifier(Modifier::CROSSED_OUT)),
            Tag::Link { dest_url, .. } => self.links.push((self.inline.len(), dest_url.to_string())),
            Tag::Image { .. } => {
                self.push("[image: ".into(), DIM);
                self.push_style(DIM);
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph | TagEnd::HtmlBlock => self.flush(),
            TagEnd::Heading(_) => {
                self.flush();
                self.styles.pop();
            }
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.quote = self.quote.saturating_sub(1);
            }
            TagEnd::CodeBlock => self.code_block(),
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
            }
            TagEnd::Item => self.end_item(),
            TagEnd::Table => self.table_block(),
            TagEnd::TableHead | TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                self.styles.pop();
            }
            TagEnd::Link => self.end_link(),
            TagEnd::Image => {
                self.styles.pop();
                self.push("]".into(), DIM);
            }
            _ => {}
        }
    }

    fn end_item(&mut self) {
        self.flush();
        if self.items.last().is_some_and(|i| i.marker.is_some()) {
            let spans = self.prefix(true);
            self.lines.push(Line::from(spans));
        }
        self.items.pop();
    }

    fn end_link(&mut self) {
        let Some((start, url)) = self.links.pop() else { return };
        let text: String = self.inline.get(start..).unwrap_or_default().iter().map(|(t, _)| t.as_str()).collect();
        if !url.is_empty() && text.trim() != url {
            self.push(format!(" ({url})"), DIM);
        }
    }

    fn text(&mut self, text: &str) {
        if let Some((_, code)) = &mut self.code {
            code.push_str(text);
            return;
        }
        let style = self.style();
        for segment in decorate(text, style) {
            self.push(segment.0, segment.1);
        }
    }

    fn push(&mut self, text: String, style: Style) {
        if text.is_empty() {
            return;
        }
        if let Some(table) = &mut self.table {
            if let Some(cell) = table.rows.last_mut().and_then(|row| row.last_mut()) {
                cell.push((text.replace('\n', " "), style));
            }
            return;
        }
        self.inline.push((text, style));
    }

    fn prefix(&mut self, first: bool) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        if self.quote > 0 {
            spans.push(Span::styled("│ ".repeat(self.quote), DIM));
        }
        if let Some(item) = self.items.last_mut() {
            match item.marker.take_if(|_| first) {
                Some(marker) => {
                    let width: usize = marker.iter().map(|(t, _)| t.chars().count()).sum();
                    spans.push(Span::raw(" ".repeat(item.indent.saturating_sub(width))));
                    spans.extend(marker.into_iter().map(|(t, s)| Span::styled(t, s)));
                }
                None => spans.push(Span::raw(" ".repeat(item.indent))),
            }
        }
        spans
    }

    fn gap(&mut self) {
        if self.lines.last().is_some_and(|line| !is_blank(line)) {
            let line = if self.quote > 0 {
                Line::from(Span::styled("│ ".repeat(self.quote).trim_end().to_string(), DIM))
            } else {
                Line::default()
            };
            self.lines.push(line);
        }
    }

    fn flush(&mut self) {
        let segments = std::mem::take(&mut self.inline);
        if segments.iter().all(|(t, _)| t.trim().is_empty()) {
            return;
        }
        let first = self.prefix(true);
        let rest = self.prefix(false);
        self.lines.extend(wrap(&segments, self.width, first, &rest));
    }

    fn code_block(&mut self) {
        let (language, code) = self.code.take().unwrap_or_default();
        let code = code.trim_end_matches('\n');
        let prefix = self.prefix(false);
        let room = self.width.saturating_sub(spans_width(&prefix) + 2);
        let highlighted = syntax::paint_fence(code, &language);
        let plain: Vec<Vec<Segment>> = match &highlighted {
            Some(_) => Vec::new(),
            None => code.split('\n').map(|line| vec![(line.replace('\t', "    "), CODE)]).collect(),
        };
        let lines = highlighted.as_deref().unwrap_or(&plain);
        for line in lines {
            let mut spans = prefix.clone();
            spans.push(Span::styled("│ ", DIM));
            spans.extend(cut_segments(line, room).into_iter().map(|(t, s)| Span::styled(t, s)));
            self.lines.push(Line::from(spans));
        }
    }

    fn table_block(&mut self) {
        let Some(table) = self.table.take() else { return };
        let prefix = self.prefix(false);
        let room = self.width.saturating_sub(spans_width(&prefix));
        let widths = column_widths(&table.rows, room);
        for (r, row) in table.rows.iter().enumerate() {
            let mut spans = prefix.clone();
            spans.extend(table_row(row, &widths));
            self.lines.push(Line::from(spans));
            if r + 1 == table.header_rows {
                let rule: Vec<String> = widths.iter().map(|w| "─".repeat(*w)).collect();
                let mut spans = prefix.clone();
                spans.push(Span::styled(rule.join("─┼─"), DIM));
                self.lines.push(Line::from(spans));
            }
        }
    }

    fn strip_html(&mut self, html: &str) -> String {
        let mut out = String::new();
        let mut rest = html;
        loop {
            if self.in_comment {
                match self.end_comment(rest) {
                    Some(after) => rest = after,
                    None => return out,
                }
            }
            let Some(start) = rest.find('<') else {
                out.push_str(rest);
                return out;
            };
            out.push_str(&rest[..start]);
            rest = &rest[start..];
            if let Some(after) = rest.strip_prefix("<!--") {
                self.in_comment = true;
                rest = after;
                continue;
            }
            let Some(end) = rest.find('>') else {
                out.push_str(rest);
                return out;
            };
            let tag = rest[1..end].trim().to_lowercase();
            if tag.starts_with("br") {
                out.push('\n');
            } else if tag.starts_with("img") {
                out.push_str("[image]");
            }
            rest = &rest[end + 1..];
        }
    }

    fn end_comment<'a>(&mut self, rest: &'a str) -> Option<&'a str> {
        let end = rest.find("-->")?;
        self.in_comment = false;
        Some(&rest[end + 3..])
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        self.flush();
        while self.lines.last().is_some_and(is_blank) {
            self.lines.pop();
        }
        self.lines
    }
}

fn column_widths(rows: &[Vec<Vec<Segment>>], room: usize) -> Vec<usize> {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return Vec::new();
    }
    let mut widths = vec![0; columns];
    for row in rows {
        for (c, cell) in row.iter().enumerate() {
            widths[c] = widths[c].max(segments_width(cell));
        }
    }
    let separators = CELL_SEPARATOR.chars().count() * (columns - 1);
    while widths.iter().sum::<usize>() + separators > room {
        let Some(widest) = (0..columns).filter(|&c| widths[c] > MIN_CELL).max_by_key(|&c| widths[c]) else {
            break;
        };
        widths[widest] -= 1;
    }
    widths
}

fn table_row(row: &[Vec<Segment>], widths: &[usize]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (c, width) in widths.iter().enumerate() {
        if c > 0 {
            spans.push(Span::styled(CELL_SEPARATOR, DIM));
        }
        let cell = row.get(c).map(Vec::as_slice).unwrap_or_default();
        let cut = cut_segments(cell, *width);
        let used = segments_width(&cut);
        spans.extend(cut.into_iter().map(|(t, s)| Span::styled(t, s)));
        spans.push(Span::raw(" ".repeat(width - used)));
    }
    spans
}

fn is_blank(line: &Line) -> bool {
    line.spans.iter().all(|s| s.content.trim().is_empty() || s.content.trim() == "│")
}

fn spans_width(spans: &[Span]) -> usize {
    spans.iter().map(|s| s.content.chars().count()).sum()
}

fn segments_width(segments: &[Segment]) -> usize {
    segments.iter().map(|(t, _)| t.chars().count()).sum()
}

fn cut_segments(segments: &[Segment], width: usize) -> Vec<Segment> {
    if segments_width(segments) <= width {
        return segments.to_vec();
    }
    let mut out = Vec::new();
    let mut room = width.saturating_sub(1);
    for (text, style) in segments {
        if room == 0 {
            break;
        }
        let part: String = text.chars().take(room).collect();
        room -= part.chars().count();
        out.push((part, *style));
    }
    let style = out.last().map(|(_, s)| *s).unwrap_or_default();
    out.push(("…".into(), style));
    out
}

fn decorate(text: &str, style: Style) -> Vec<Segment> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<Segment> = Vec::new();
    let mut plain = String::new();
    let mut i = 0;
    while i < chars.len() {
        let starts_word = i == 0 || chars[i - 1].is_whitespace() || matches!(chars[i - 1], '(' | '[');
        if let Some((len, extra)) = starts_word.then(|| reference_at(&chars[i..])).flatten() {
            if !plain.is_empty() {
                out.push((std::mem::take(&mut plain), style));
            }
            out.push((chars[i..i + len].iter().collect(), style.patch(extra)));
            i += len;
        } else {
            plain.push(chars[i]);
            i += 1;
        }
    }
    if !plain.is_empty() {
        out.push((plain, style));
    }
    out
}

fn reference_at(chars: &[char]) -> Option<(usize, Style)> {
    let ends = |n: usize| chars.get(n).is_none_or(|c| !c.is_alphanumeric() && *c != '_');
    let digits_from = |n: usize| chars[n..].iter().take_while(|c| c.is_ascii_digit()).count();
    match chars.first()? {
        '#' => {
            let digits = digits_from(1);
            (digits > 0 && ends(1 + digits)).then_some((1 + digits, REFERENCE))
        }
        '@' => {
            let name = chars[1..].iter().take_while(|c| c.is_ascii_alphanumeric() || **c == '-').count();
            (name > 0 && chars[1].is_ascii_alphanumeric() && ends(1 + name)).then_some((1 + name, MENTION))
        }
        c if c.is_ascii_alphabetic() => {
            let team = chars.iter().take_while(|c| c.is_ascii_alphanumeric()).count();
            let prefix: String = chars[..team].iter().collect();
            let shaped = prefix.eq_ignore_ascii_case("sc")
                || (prefix.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) && c.is_ascii_uppercase());
            if !shaped || chars.get(team) != Some(&'-') {
                return None;
            }
            let digits = digits_from(team + 1);
            (digits > 0 && ends(team + 1 + digits)).then_some((team + 1 + digits, REFERENCE))
        }
        _ => None,
    }
}

enum Token {
    Word(Vec<Segment>, usize),
    Break,
}

fn tokens(segments: &[Segment]) -> Vec<Token> {
    let mut out = Vec::new();
    let mut word: Vec<Segment> = Vec::new();
    let mut width = 0;
    let end_word = |word: &mut Vec<Segment>, width: &mut usize, out: &mut Vec<Token>| {
        if !word.is_empty() {
            out.push(Token::Word(std::mem::take(word), std::mem::take(width)));
        }
    };
    for (text, style) in segments {
        for c in text.chars() {
            if c == '\n' {
                end_word(&mut word, &mut width, &mut out);
                out.push(Token::Break);
            } else if c.is_whitespace() {
                end_word(&mut word, &mut width, &mut out);
            } else {
                match word.last_mut() {
                    Some((part, s)) if s == style => part.push(c),
                    _ => word.push((c.to_string(), *style)),
                }
                width += 1;
            }
        }
    }
    end_word(&mut word, &mut width, &mut out);
    out
}

struct Wrap<'a> {
    width: usize,
    rest: &'a [Span<'static>],
    lines: Vec<Line<'static>>,
    spans: Vec<Span<'static>>,
    used: usize,
    empty: bool,
    breaks: usize,
}

impl Wrap<'_> {
    fn new_line(&mut self) {
        self.lines.push(Line::from(std::mem::replace(&mut self.spans, self.rest.to_vec())));
        self.used = spans_width(self.rest);
    }

    fn word(&mut self, parts: Vec<Segment>, word_width: usize) {
        if self.breaks > 0 {
            self.new_line();
            for _ in 1..self.breaks {
                self.lines.push(Line::from(self.rest.to_vec()));
            }
            self.breaks = 0;
            self.empty = true;
        }
        if !self.empty && self.used + 1 + word_width > self.width {
            self.new_line();
            self.empty = true;
        }
        if !self.empty {
            self.spans.push(Span::raw(" "));
            self.used += 1;
        }
        self.empty = false;
        if word_width <= self.width.saturating_sub(self.used) {
            self.spans.extend(parts.into_iter().map(|(t, s)| Span::styled(t, s)));
            self.used += word_width;
            return;
        }
        for (text, style) in parts {
            self.chop(&text, style);
        }
    }

    fn chop(&mut self, text: &str, style: Style) {
        let mut chars = text.chars().peekable();
        while chars.peek().is_some() {
            if self.used >= self.width {
                self.new_line();
            }
            let room = self.width.saturating_sub(self.used).max(1);
            let chunk: String = chars.by_ref().take(room).collect();
            self.used += chunk.chars().count();
            self.spans.push(Span::styled(chunk, style));
        }
    }
}

fn wrap(segments: &[Segment], width: usize, first: Vec<Span<'static>>, rest: &[Span<'static>]) -> Vec<Line<'static>> {
    let used = spans_width(&first);
    let mut wrap = Wrap { width, rest, lines: Vec::new(), spans: first, used, empty: true, breaks: 0 };
    for token in tokens(segments) {
        match token {
            Token::Break => {
                if !(wrap.empty && wrap.lines.is_empty()) {
                    wrap.breaks += 1;
                }
            }
            Token::Word(parts, word_width) => wrap.word(parts, word_width),
        }
    }
    if !wrap.empty {
        wrap.lines.push(Line::from(wrap.spans));
    }
    wrap.lines
}

#[cfg(test)]
mod tests;
