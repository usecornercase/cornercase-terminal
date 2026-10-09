use crate::issues::markup::{code_span, escape, longest_run};

#[derive(Default)]
struct Node {
    tag: String,
    attrs: Vec<(String, String)>,
    text: String,
    children: Vec<Self>,
}

impl Node {
    fn attr(&self, name: &str) -> &str {
        self.attrs.iter().find(|(key, _)| key == name).map_or("", |(_, value)| value)
    }

    fn plain(&self) -> String {
        format!("{}{}", self.text, self.children.iter().map(Self::plain).collect::<String>())
    }

    fn content(&self) -> String {
        self.children.iter().map(Self::render).collect::<String>()
    }

    fn render(&self) -> String {
        if self.tag.is_empty() {
            return escape(&self.text);
        }
        let body = self.content();
        match self.tag.as_str() {
            "script" | "style" | "label" => String::new(),
            "p" | "div" | "section" => format!("{}\n\n", body.trim()),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = self.tag.as_bytes()[1] - b'0';
                format!("{} {}\n\n", "#".repeat(usize::from(level)), body.trim())
            }
            "br" => "  \n".into(),
            "hr" => "---\n\n".into(),
            "strong" | "b" => format!("**{body}**"),
            "em" | "i" => format!("*{body}*"),
            "s" | "del" => format!("~~{body}~~"),
            "code" => code_span(&self.plain()),
            "pre" => {
                let code = self.plain();
                let fence = "~".repeat(longest_run(&code, '~').max(2) + 1);
                let language = self
                    .children
                    .iter()
                    .find(|n| n.tag == "code")
                    .map_or("", |n| n.attr("class").strip_prefix("language-").unwrap_or_default());
                format!("{fence}{language}\n{}\n{fence}\n\n", code.trim_end_matches('\n'))
            }
            "a" if !self.attr("href").is_empty() => {
                let href = self.attr("href").replace(' ', "%20").replace('(', "%28").replace(')', "%29");
                format!("[{body}]({href})")
            }
            "img" => escape(self.attr("alt")),
            "blockquote" => {
                let quote = body.trim().lines().map(|l| format!("> {l}")).collect::<Vec<_>>().join("\n");
                format!("{quote}\n\n")
            }
            "ul" | "ol" => self.list(),
            "input" if self.attr("type") == "checkbox" => {
                let checked = self.attrs.iter().any(|(k, _)| k == "checked");
                format!("[{}] ", if checked { 'x' } else { ' ' })
            }
            _ => body,
        }
    }

    fn list(&self) -> String {
        let start = self.attr("start").parse::<u64>().unwrap_or(1);
        let mut rows = Vec::new();
        for (index, item) in self.children.iter().filter(|n| n.tag == "li").enumerate() {
            let marker = if self.tag == "ol" {
                format!("{}. ", start.saturating_add(u64::try_from(index).unwrap_or_default()))
            } else if item.attr("data-type") == "taskItem" {
                format!("- [{}] ", if item.attr("data-checked") == "true" { 'x' } else { ' ' })
            } else {
                "- ".into()
            };
            let body = item.content();
            let mut lines = body.trim().lines();
            let mut row = format!("{marker}{}", lines.next().unwrap_or_default());
            for line in lines {
                row.push('\n');
                if !line.is_empty() {
                    row.push_str(&" ".repeat(marker.len()));
                    row.push_str(line);
                }
            }
            rows.push(row);
        }
        format!("{}\n\n", rows.join("\n"))
    }
}

pub fn markdown(input: &str) -> Option<String> {
    let mut stack = vec![Node { tag: "root".into(), ..Node::default() }];
    let mut rest = input;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.get(after.find("-->")? + 3..)?;
        } else if let Some(after) = rest.strip_prefix('<') {
            let end = tag_end(after)?;
            let raw = after[..end].trim();
            rest = &after[end + 1..];
            if raw.starts_with('!') {
                continue;
            }
            if let Some(closing) = raw.strip_prefix('/') {
                let node = stack.pop()?;
                if node.tag != closing.trim().to_ascii_lowercase() {
                    return None;
                }
                stack.last_mut()?.children.push(node);
                continue;
            }
            let (tag, attrs) = attributes(raw.trim_end_matches('/'))?;
            let node = Node { tag, attrs, ..Node::default() };
            if raw.ends_with('/') || ["br", "hr", "img", "input", "meta", "link", "wbr"].contains(&node.tag.as_str()) {
                stack.last_mut()?.children.push(node);
            } else {
                if stack.len() >= 64 {
                    return None;
                }
                stack.push(node);
            }
        } else {
            let end = rest.find('<').unwrap_or(rest.len());
            let mut text = entities(&rest[..end]);
            if !stack.iter().any(|n| n.tag == "pre" || n.tag == "code") {
                text = collapse(&text);
            }
            stack.last_mut()?.children.push(Node { text, ..Node::default() });
            rest = &rest[end..];
        }
    }
    (stack.len() == 1).then(|| stack[0].content().trim().to_string())
}

fn tag_end(input: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in input.char_indices() {
        if quote == Some(c) {
            quote = None;
        } else if quote.is_none() {
            if c == '>' {
                return Some(i);
            }
            if matches!(c, '\'' | '"') {
                quote = Some(c);
            }
        }
    }
    None
}

fn attributes(input: &str) -> Option<(String, Vec<(String, String)>)> {
    let end = input.find(char::is_whitespace).unwrap_or(input.len());
    let tag = input[..end].to_ascii_lowercase();
    if tag.is_empty() {
        return None;
    }
    let mut attrs = Vec::new();
    let mut rest = input[end..].trim();
    while !rest.is_empty() {
        let end = rest.find(|c: char| c.is_whitespace() || c == '=').unwrap_or(rest.len());
        if end == 0 {
            return None;
        }
        let name = rest[..end].to_ascii_lowercase();
        rest = rest[end..].trim_start();
        let value = if let Some(after) = rest.strip_prefix('=') {
            rest = after.trim_start();
            let first = rest.chars().next()?;
            let value;
            if matches!(first, '\'' | '"') {
                let after = &rest[1..];
                let end = after.find(first)?;
                value = &after[..end];
                rest = &after[end + 1..];
            } else {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                value = &rest[..end];
                rest = &rest[end..];
            }
            entities(value)
        } else {
            String::new()
        };
        attrs.push((name, value));
        rest = rest.trim_start();
    }
    Some((tag, attrs))
}

fn collapse(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_whitespace() {
            if !out.ends_with(' ') {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn entities(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let decoded = after.find(';').filter(|end| *end <= 16).and_then(|end| {
            let name = &after[..end];
            let c = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" => Some('\''),
                "nbsp" => Some(' '),
                _ => name
                    .strip_prefix("#x")
                    .or_else(|| name.strip_prefix("#X"))
                    .and_then(|n| u32::from_str_radix(n, 16).ok())
                    .or_else(|| name.strip_prefix('#').and_then(|n| n.parse().ok()))
                    .and_then(char::from_u32),
            };
            c.map(|c| (end, c))
        });
        if let Some((end, c)) = decoded {
            out.push(c);
            rest = &after[end + 1..];
        } else {
            out.push('&');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::heading(
        "<h2>Fix &amp; ship</h2><p>A <strong>bold</strong> <em>word</em>.</p>",
        "## Fix & ship\n\nA **bold** *word*."
    )]
    #[case::list("<ul><li><p>One</p></li><li>Two</li></ul>", "- One\n- Two")]
    #[case::ordered("<ol start='3'><li>Three</li></ol>", "3. Three")]
    #[case::tasks(
        "<ul data-type='taskList'><li data-type='taskItem' data-checked='true'><label><input type='checkbox' checked></label><div><p>Done</p></div></li></ul>",
        "- [x] Done"
    )]
    #[case::checkbox("<ul><li><input type='checkbox'>Todo</li></ul>", "- [ ] Todo")]
    #[case::code("<pre><code class='language-rust'>a &lt; b\n  c</code></pre>", "~~~rust\na < b\n  c\n~~~")]
    #[case::link(
        "<p><a href='https://example.com/a?x=1&amp;y=2'>Link</a></p>",
        "[Link](https://example.com/a?x=1&y=2)"
    )]
    #[case::quote("<blockquote><p>Quoted</p></blockquote>", "> Quoted")]
    #[case::unknown("<custom>A &#233; &#x1f600; &unknown;</custom>", "A é 😀 &unknown;")]
    #[case::quoted_angle("<a href='https://example.com/?q=>'>Link</a>", "[Link](https://example.com/?q=>)")]
    fn renders_rich_text(#[case] html: &str, #[case] expected: &str) {
        assert_eq!(markdown(html).as_deref(), Some(expected));
    }

    #[rstest]
    #[case::unclosed("<p>Text")]
    #[case::wrong_close("<p>Text</div>")]
    #[case::broken_tag("<p title='text>")]
    fn malformed_html_uses_the_plain_text_fallback(#[case] html: &str) {
        assert_eq!(markdown(html), None);
    }

    #[test]
    fn deeply_nested_html_does_not_overflow_the_stack() {
        assert_eq!(markdown(&format!("{}text{}", "<div>".repeat(100), "</div>".repeat(100))), None);
    }
}
