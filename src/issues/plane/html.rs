use crate::issues::md::{self, code_span, escape, item_lines, longest_run, one_line, prefix_lines};

const BLOCKS: [&str; 20] = [
    "p",
    "div",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "ul",
    "ol",
    "li",
    "pre",
    "blockquote",
    "hr",
    "table",
    "figure",
    "section",
    "details",
    "summary",
    "article",
];
const VOID: [&str; 8] = ["br", "hr", "img", "input", "col", "meta", "link", "wbr"];

struct Element {
    tag: String,
    attrs: Vec<(String, String)>,
    children: Vec<Node>,
}

enum Node {
    Text(String),
    Element(Element),
}

impl Element {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
    }
}

pub fn markdown(html: &str) -> String {
    blocks(&parse(html), "\n\n").trim().to_string()
}

fn parse(html: &str) -> Vec<Node> {
    let mut stack = vec![Element { tag: String::new(), attrs: Vec::new(), children: Vec::new() }];
    let mut rest = html;
    while !rest.is_empty() {
        let Some(start) = rest.find('<') else {
            push(&mut stack, Node::Text(unescape(rest)));
            break;
        };
        if start > 0 {
            push(&mut stack, Node::Text(unescape(&rest[..start])));
            rest = &rest[start..];
        }
        let Some(end) = tag_end(rest) else {
            push(&mut stack, Node::Text(unescape(rest)));
            break;
        };
        open_or_close(&mut stack, &rest[1..end]);
        rest = &rest[end + 1..];
    }
    while stack.len() > 1 {
        if let Some(done) = stack.pop() {
            push(&mut stack, Node::Element(done));
        }
    }
    stack.pop().map_or_else(Vec::new, |root| root.children)
}

fn push(stack: &mut [Element], node: Node) {
    if let Some(top) = stack.last_mut() {
        top.children.push(node);
    }
}

fn tag_end(rest: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in rest.char_indices().skip(1) {
        match quote {
            Some(open) if c == open => quote = None,
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '>' => return Some(i),
            Some(_) | None => {}
        }
    }
    None
}

fn open_or_close(stack: &mut Vec<Element>, inner: &str) {
    if inner.starts_with('!') || inner.starts_with('?') {
        return;
    }
    if let Some(name) = inner.strip_prefix('/') {
        close(stack, &name.trim().to_ascii_lowercase());
        return;
    }
    let self_closing = inner.ends_with('/');
    let inner = inner.trim_end_matches('/');
    let name_end = inner.find(char::is_whitespace).unwrap_or(inner.len());
    let tag = inner[..name_end].to_ascii_lowercase();
    if tag.is_empty() {
        return;
    }
    let void = self_closing || VOID.contains(&tag.as_str());
    let element = Element { attrs: attributes(&inner[name_end..]), tag, children: Vec::new() };
    if void {
        push(stack, Node::Element(element));
    } else {
        stack.push(element);
    }
}

fn close(stack: &mut Vec<Element>, name: &str) {
    let Some(at) = stack.iter().rposition(|open| open.tag == name) else {
        return;
    };
    if at == 0 {
        return;
    }
    while stack.len() > at {
        let Some(done) = stack.pop() else {
            return;
        };
        push(stack, Node::Element(done));
    }
}

fn attributes(text: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut attrs = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '=' {
            i += 1;
        }
        let name: String = chars[start..i].iter().collect::<String>().to_ascii_lowercase();
        let value: String = if chars.get(i) == Some(&'=') {
            i += 1;
            match chars.get(i) {
                Some(&quote) if quote == '"' || quote == '\'' => {
                    let from = i + 1;
                    let len = chars[from..].iter().position(|&c| c == quote).unwrap_or(chars.len() - from);
                    i = from + len + 1;
                    chars[from..from + len].iter().collect()
                }
                _ => {
                    let from = i;
                    while i < chars.len() && !chars[i].is_whitespace() {
                        i += 1;
                    }
                    chars[from..i].iter().collect()
                }
            }
        } else {
            String::new()
        };
        if !name.is_empty() {
            attrs.push((name, unescape(&value)));
        }
    }
    attrs
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let found = rest.find(';').filter(|&end| end <= 10).and_then(|end| entity(&rest[1..end]).map(|c| (end, c)));
        if let Some((end, c)) = found {
            out.push(c);
            rest = &rest[end + 1..];
        } else {
            out.push('&');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ => {
            let digits = name.strip_prefix('#')?;
            let code = match digits.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => digits.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

fn blocks(nodes: &[Node], gap: &str) -> String {
    let mut parts = Vec::new();
    let mut run = String::new();
    for node in nodes {
        match node {
            Node::Element(element) if BLOCKS.contains(&element.tag.as_str()) => {
                flush(&mut parts, &mut run);
                parts.push(block(element));
            }
            _ => run.push_str(&inline_node(node)),
        }
    }
    flush(&mut parts, &mut run);
    parts.into_iter().filter(|part| !part.trim().is_empty()).collect::<Vec<_>>().join(gap)
}

fn flush(parts: &mut Vec<String>, run: &mut String) {
    let text = run.trim();
    if !text.is_empty() {
        parts.push(text.to_string());
    }
    run.clear();
}

fn block(element: &Element) -> String {
    let content = &element.children;
    match element.tag.as_str() {
        "p" => inline(content).trim().to_string(),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let level = element.tag[1..].parse::<usize>().unwrap_or(1).clamp(1, 6);
            format!("{} {}", "#".repeat(level), one_line(&inline(content)))
        }
        "ul" => list(element, None),
        "ol" => list(element, Some(element.attr("start").and_then(|start| start.parse().ok()).unwrap_or(1))),
        "pre" => code_block(element),
        "blockquote" => prefix_lines("> ", ">", &blocks(content, "\n\n")),
        "hr" => "---".into(),
        "table" => table(element),
        "li" => blocks(content, "\n"),
        _ => blocks(content, "\n\n"),
    }
}

fn elements<'a>(nodes: &'a [Node], tag: &str) -> Vec<&'a Element> {
    nodes
        .iter()
        .filter_map(|node| match node {
            Node::Element(element) if element.tag == tag => Some(element),
            _ => None,
        })
        .collect()
}

fn task_marker(done: bool) -> String {
    format!("- [{}] ", if done { 'x' } else { ' ' })
}

fn list(element: &Element, start: Option<u64>) -> String {
    let tasks = element.attr("data-type") == Some("taskList");
    elements(&element.children, "li")
        .into_iter()
        .zip(start.unwrap_or(1)..)
        .map(|(item, number)| {
            let marker = match (item.attr("data-checked"), tasks, start) {
                (Some(checked), _, _) => task_marker(checked == "true"),
                (None, true, _) => task_marker(false),
                (None, false, Some(_)) => format!("{number}. "),
                (None, false, None) => "- ".to_string(),
            };
            item_lines(&marker, &blocks(&item.children, "\n"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn plain(nodes: &[Node]) -> String {
    nodes
        .iter()
        .map(|node| match node {
            Node::Text(text) => text.clone(),
            Node::Element(element) if element.tag == "br" => "\n".to_string(),
            Node::Element(element) => plain(&element.children),
        })
        .collect()
}

fn code_block(element: &Element) -> String {
    let language = element
        .children
        .iter()
        .find_map(|node| match node {
            Node::Element(code) if code.tag == "code" => code.attr("class"),
            _ => None,
        })
        .and_then(|class| class.split_whitespace().find_map(|name| name.strip_prefix("language-")))
        .unwrap_or_default();
    let code = plain(&element.children);
    let fence = "`".repeat(longest_run(&code, '`').max(2) + 1);
    format!("{fence}{language}\n{}\n{fence}", code.trim_end_matches('\n'))
}

fn rows<'a>(nodes: &'a [Node], found: &mut Vec<&'a Element>) {
    for node in nodes {
        if let Node::Element(element) = node {
            if element.tag == "tr" {
                found.push(element);
            } else {
                rows(&element.children, found);
            }
        }
    }
}

fn table(element: &Element) -> String {
    let mut found = Vec::new();
    rows(&element.children, &mut found);
    let grid: Vec<Vec<String>> = found
        .iter()
        .map(|row| {
            row.children
                .iter()
                .filter_map(|node| match node {
                    Node::Element(cell) if cell.tag == "td" || cell.tag == "th" => {
                        Some(one_line(&blocks(&cell.children, "\n")).replace('|', "\\|"))
                    }
                    _ => None,
                })
                .collect::<Vec<String>>()
        })
        .filter(|row| !row.is_empty())
        .collect();
    md::grid(&grid)
}

fn inline(nodes: &[Node]) -> String {
    nodes.iter().map(inline_node).collect()
}

fn collapse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if !c.is_whitespace() {
            out.push(c);
        } else if !out.ends_with(' ') {
            out.push(' ');
        }
    }
    out
}

fn wrap(open: &str, close: &str, inner: &str) -> String {
    let core = inner.trim();
    if core.is_empty() {
        return inner.to_string();
    }
    let lead = &inner[..inner.len() - inner.trim_start().len()];
    let trail = &inner[inner.trim_end().len()..];
    format!("{lead}{open}{core}{close}{trail}")
}

fn inline_node(node: &Node) -> String {
    match node {
        Node::Text(text) => escape(&collapse(text)),
        Node::Element(element) => inline_element(element),
    }
}

fn inline_element(element: &Element) -> String {
    let inner = || inline(&element.children);
    match element.tag.as_str() {
        "br" => "  \n".into(),
        "strong" | "b" => wrap("**", "**", &inner()),
        "em" | "i" => wrap("*", "*", &inner()),
        "s" | "del" | "strike" => wrap("~~", "~~", &inner()),
        "code" => {
            let code = one_line(&plain(&element.children));
            if code.is_empty() { String::new() } else { code_span(&code) }
        }
        "a" => {
            let text = inner();
            let href = element.attr("href").unwrap_or_default();
            let href = href.replace(' ', "%20").replace('(', "%28").replace(')', "%29");
            match (href.is_empty(), text.trim().is_empty()) {
                (true, _) => text,
                (false, true) => format!("<{href}>"),
                (false, false) => format!("[{}]({href})", text.trim()),
            }
        }
        "img" => match element.attr("src") {
            Some(src) if !src.is_empty() => format!("![{}]({src})", escape(element.attr("alt").unwrap_or_default())),
            _ => String::new(),
        },
        "script" | "style" | "input" => String::new(),
        _ => inner(),
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::strong("<p>Fix the <strong>login</strong> flow</p>", "Fix the **login** flow")]
    #[case::emphasis_and_strike("<p><em>a</em> and <s>b</s></p>", "*a* and ~~b~~")]
    #[case::nested_marks("<p><strong><em>x</em></strong></p>", "***x***")]
    #[case::space_inside_a_mark("<p><strong>a </strong>b</p>", "**a** b")]
    #[case::inline_code("<p>run <code>cargo test</code></p>", "run `cargo test`")]
    #[case::link(r#"<p><a href="https://plane.so/x" target="_blank">Plane</a></p>"#, "[Plane](https://plane.so/x)")]
    #[case::link_without_text(r#"<p><a href="https://plane.so/x"></a></p>"#, "<https://plane.so/x>")]
    #[case::entities("<p>a &amp; b &lt;c&gt; &#65;</p>", "a & b \\<c> A")]
    #[case::hard_break("<p>one<br>two</p>", "one  \ntwo")]
    #[case::image(r#"<p><img src="https://x.dev/a.png" alt="shot"></p>"#, "![shot](https://x.dev/a.png)")]
    #[case::unknown_tags_keep_their_text("<custom-tag>text</custom-tag>", "text")]
    #[case::plain_text("just text", "just text")]
    #[case::nothing("", "")]
    #[case::empty_paragraph("<p></p>", "")]
    #[case::stray_closing_tag("</div><p>a</p>", "a")]
    fn turns_inline_html_into_markdown(#[case] html: &str, #[case] expected: &str) {
        assert_eq!(markdown(html), expected);
    }

    #[rstest]
    #[case::heading("<h2>Plan</h2><p>Body</p>", "## Plan\n\nBody")]
    #[case::paragraphs_separated_by_newlines("<p>a</p>\n<p>b</p>", "a\n\nb")]
    #[case::rule("<p>a</p><hr><p>b</p>", "a\n\n---\n\nb")]
    #[case::quote("<blockquote><p>quoted</p></blockquote>", "> quoted")]
    #[case::code_block("<pre><code class=\"language-rust\">fn main() {}\n</code></pre>", "```rust\nfn main() {}\n```")]
    #[case::code_block_with_entities("<pre><code>a &lt; b</code></pre>", "```\na < b\n```")]
    fn turns_blocks_into_markdown(#[case] html: &str, #[case] expected: &str) {
        assert_eq!(markdown(html), expected);
    }

    #[rstest]
    #[case::bullets("<ul><li><p>one</p></li><li><p>two</p></li></ul>", "- one\n- two")]
    #[case::numbers_from_start(r#"<ol start="3"><li><p>a</p></li><li><p>b</p></li></ol>"#, "3. a\n4. b")]
    #[case::nested("<ul><li><p>a</p><ul><li><p>b</p></li></ul></li></ul>", "- a\n  - b")]
    #[case::items_without_paragraphs("<ul><li>a</li><li>b</li></ul>", "- a\n- b")]
    #[case::tasks(
        concat!(
            r#"<ul data-type="taskList">"#,
            r#"<li data-checked="true"><label><input type="checkbox" checked="checked"><span></span></label>"#,
            "<div><p>done</p></div></li>",
            r#"<li data-checked="false"><label><input type="checkbox"><span></span></label>"#,
            "<div><p>todo</p></div></li></ul>"
        ),
        "- [x] done\n- [ ] todo"
    )]
    fn turns_lists_into_markdown(#[case] html: &str, #[case] expected: &str) {
        assert_eq!(markdown(html), expected);
    }

    #[test]
    fn a_table_becomes_a_markdown_table() {
        let html = "<table><tbody><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2|3</td></tr></tbody></table>";
        assert_eq!(markdown(html), "| A | B |\n| --- | --- |\n| 1 | 2\\|3 |");
    }

    #[test]
    fn attributes_may_be_quoted_bare_or_empty() {
        let attrs = attributes(r#" href="a b" data-x=1 checked  title='it"s'"#);
        let expected = [("href", "a b"), ("data-x", "1"), ("checked", ""), ("title", "it\"s")];
        assert_eq!(attrs, expected.map(|(k, v)| (k.to_string(), v.to_string())));
    }
}
