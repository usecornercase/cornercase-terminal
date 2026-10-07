use serde_json::Value;

use crate::issues::http::text;

const INLINE: [&str; 9] =
    ["text", "hardBreak", "mention", "emoji", "inlineCard", "date", "status", "mediaInline", "placeholder"];

pub fn markdown(doc: &Value) -> String {
    match doc {
        Value::String(text) => text.clone(),
        Value::Object(_) => block(doc).trim_end().to_string(),
        _ => String::new(),
    }
}

fn kind(node: &Value) -> &str {
    node["type"].as_str().unwrap_or_default()
}

fn children(node: &Value) -> &[Value] {
    node["content"].as_array().map_or(&[], Vec::as_slice)
}

fn attr(node: &Value, name: &str) -> String {
    match &node["attrs"][name] {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        _ => String::new(),
    }
}

fn blocks(nodes: &[Value], gap: &str) -> String {
    nodes.iter().map(block).filter(|b| !b.trim().is_empty()).collect::<Vec<_>>().join(gap)
}

fn block(node: &Value) -> String {
    let content = children(node);
    match kind(node) {
        "paragraph" => inline(content),
        "heading" => {
            let level = attr(node, "level").parse::<usize>().unwrap_or(1).clamp(1, 6);
            format!("{} {}", "#".repeat(level), one_line(&inline(content)))
        }
        "bulletList" => list(node, None),
        "orderedList" => list(node, Some(attr(node, "order").parse().unwrap_or(1))),
        "taskList" => tasks(node),
        "decisionList" => {
            content.iter().map(|item| format!("- {}", inline(children(item)))).collect::<Vec<_>>().join("\n")
        }
        "codeBlock" => {
            let code: String = content.iter().map(|t| text(t, "/text")).collect();
            let fence = "`".repeat(longest_run(&code, '`').max(2) + 1);
            format!("{fence}{}\n{}\n{fence}", attr(node, "language"), code.trim_end_matches('\n'))
        }
        "blockquote" | "panel" => prefix_lines("> ", ">", &blocks(content, "\n\n")),
        "rule" => "---".into(),
        "table" => table(node),
        "expand" | "nestedExpand" => {
            let title = attr(node, "title");
            let title = if title.is_empty() { String::new() } else { format!("**{}**", escape(&title)) };
            [title, blocks(content, "\n\n")].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join("\n\n")
        }
        _ if content.iter().all(|c| INLINE.contains(&kind(c))) => inline_node(node),
        _ => blocks(content, "\n\n"),
    }
}

fn list(node: &Value, start: Option<u64>) -> String {
    let numbers = start.unwrap_or(1)..;
    numbers
        .zip(children(node))
        .map(|(number, item)| {
            let marker = if start.is_some() { format!("{number}. ") } else { "- ".into() };
            item_lines(&marker, &blocks(children(item), "\n"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn tasks(node: &Value) -> String {
    children(node)
        .iter()
        .map(|item| match kind(item) {
            "taskItem" => {
                let done = if attr(item, "state") == "DONE" { 'x' } else { ' ' };
                item_lines(&format!("- [{done}] "), &inline(children(item)))
            }
            _ => prefix_lines("  ", "", &block(item)),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn item_lines(marker: &str, body: &str) -> String {
    let pad = " ".repeat(marker.chars().count());
    let mut lines = body.lines();
    let first = format!("{marker}{}", lines.next().unwrap_or_default());
    std::iter::once(first)
        .chain(lines.map(|line| if line.is_empty() { String::new() } else { format!("{pad}{line}") }))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

fn prefix_lines(prefix: &str, empty: &str, text: &str) -> String {
    text.lines()
        .map(|line| if line.is_empty() { empty.to_string() } else { format!("{prefix}{line}") })
        .collect::<Vec<_>>()
        .join("\n")
}

fn table(node: &Value) -> String {
    let rows: Vec<Vec<String>> = children(node)
        .iter()
        .map(|row| {
            children(row)
                .iter()
                .map(|cell| one_line(&blocks(children(cell), "\n")).replace('|', "\\|"))
                .collect::<Vec<String>>()
        })
        .filter(|row| !row.is_empty())
        .collect();
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    if width == 0 {
        return String::new();
    }
    let line = |cells: &[String]| {
        let padded = (0..width).map(|i| cells.get(i).map_or("", String::as_str));
        format!("| {} |", padded.collect::<Vec<_>>().join(" | "))
    };
    let mut lines = vec![line(&rows[0]), line(&vec!["---".to_string(); width])];
    lines.extend(rows[1..].iter().map(|row| line(row)));
    lines.join("\n")
}

fn one_line(text: &str) -> String {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ")
}

fn inline(nodes: &[Value]) -> String {
    nodes.iter().map(inline_node).collect()
}

fn inline_node(node: &Value) -> String {
    match kind(node) {
        "text" => marked(node),
        "hardBreak" => "  \n".into(),
        "mention" => {
            let name = attr(node, "text");
            let name = name.trim_start_matches('@');
            format!("@{}", escape(if name.is_empty() { "someone" } else { name }))
        }
        "emoji" => {
            let emoji = attr(node, "text");
            if emoji.is_empty() { attr(node, "shortName") } else { emoji }
        }
        "inlineCard" | "blockCard" | "embedCard" => {
            let url = attr(node, "url");
            if url.is_empty() { String::new() } else { format!("<{url}>") }
        }
        "date" => attr(node, "timestamp").parse::<i64>().map(date).unwrap_or_default(),
        "status" => code_span(&attr(node, "text")),
        "media" | "mediaInline" | "placeholder" => String::new(),
        _ if children(node).is_empty() => escape(&attr(node, "text")),
        _ => inline(children(node)),
    }
}

fn marked(node: &Value) -> String {
    let raw = text(node, "/text");
    let marks: Vec<&Value> = node["marks"].as_array().into_iter().flatten().collect();
    let core = raw.trim();
    if core.is_empty() {
        return raw;
    }
    let lead = &raw[..raw.len() - raw.trim_start().len()];
    let trail = &raw[raw.trim_end().len()..];
    let is_code = marks.iter().any(|m| kind(m) == "code");
    let mut out = if is_code { code_span(core) } else { escape(core) };
    for mark in marks {
        out = match kind(mark) {
            "strong" => format!("**{out}**"),
            "em" => format!("*{out}*"),
            "strike" => format!("~~{out}~~"),
            "link" => {
                let href = attr(mark, "href").replace(' ', "%20").replace('(', "%28").replace(')', "%29");
                if href.is_empty() { out } else { format!("[{out}]({href})") }
            }
            _ => out,
        };
    }
    format!("{lead}{out}{trail}")
}

fn code_span(text: &str) -> String {
    let ticks = "`".repeat(longest_run(text, '`') + 1);
    let pad = if text.starts_with('`') || text.ends_with('`') { " " } else { "" };
    format!("{ticks}{pad}{text}{pad}{ticks}")
}

fn longest_run(text: &str, c: char) -> usize {
    let (mut longest, mut run) = (0, 0);
    for ch in text.chars() {
        run = if ch == c { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    longest
}

fn escape(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        let word = |j: Option<usize>| j.and_then(|j| chars.get(j)).is_some_and(|c| c.is_alphanumeric());
        let inside_a_word = c == '_' && word(i.checked_sub(1)) && word(Some(i + 1));
        if "\\`*_[]<~".contains(c) && !inside_a_word {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn date(millis: i64) -> String {
    let days = millis.div_euclid(86_400_000) + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted + 2) / 5 + 1;
    let month = if shifted < 10 { shifted + 3 } else { shifted - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use serde_json::json;

    use super::*;

    fn with_content(mut node: Value, content: Value) -> Value {
        node["content"] = content;
        node
    }

    fn doc(content: Value) -> Value {
        with_content(json!({ "type": "doc", "version": 1 }), content)
    }

    fn text_node(text: &str) -> Value {
        json!({ "type": "text", "text": text })
    }

    fn marked_node(text: &str, marks: Value) -> Value {
        let mut node = text_node(text);
        node["marks"] = marks;
        node
    }

    fn paragraph(content: Value) -> Value {
        with_content(json!({ "type": "paragraph" }), content)
    }

    fn item(content: Value) -> Value {
        with_content(json!({ "type": "listItem" }), content)
    }

    #[rstest]
    #[case::null(Value::Null, "")]
    #[case::plain_text(json!("already text"), "already text")]
    #[case::empty_doc(doc(json!([])), "")]
    fn takes_what_is_not_a_document_as_it_is(#[case] value: Value, #[case] expected: &str) {
        assert_eq!(markdown(&value), expected);
    }

    #[test]
    fn paragraphs_are_separated_by_a_blank_line() {
        let value = doc(json!([paragraph(json!([text_node("one")])), paragraph(json!([text_node("two")]))]));
        assert_eq!(markdown(&value), "one\n\ntwo");
    }

    #[test]
    fn headings_keep_their_level() {
        let value = doc(json!([{ "type": "heading", "attrs": { "level": 3 }, "content": [text_node("Steps")] }]));
        assert_eq!(markdown(&value), "### Steps");
    }

    #[rstest]
    #[case::strong(json!([{ "type": "strong" }]), "**bold**")]
    #[case::em(json!([{ "type": "em" }]), "*bold*")]
    #[case::strike(json!([{ "type": "strike" }]), "~~bold~~")]
    #[case::code(json!([{ "type": "code" }]), "`bold`")]
    #[case::link(json!([{ "type": "link", "attrs": { "href": "https://x.dev/a b" } }]), "[bold](https://x.dev/a%20b)")]
    #[case::two(json!([{ "type": "strong" }, { "type": "em" }]), "***bold***")]
    #[case::unknown(json!([{ "type": "textColor", "attrs": { "color": "#f00" } }]), "bold")]
    fn marks_become_markdown(#[case] marks: Value, #[case] expected: &str) {
        assert_eq!(markdown(&doc(json!([paragraph(json!([marked_node("bold", marks)]))]))), expected);
    }

    #[test]
    fn marks_leave_the_spaces_around_the_text_outside() {
        let value = doc(json!([paragraph(json!([
            text_node("a"),
            marked_node(" b ", json!([{ "type": "strong" }])),
            text_node("c")
        ]))]));
        assert_eq!(markdown(&value), "a **b** c");
    }

    #[rstest]
    #[case::stars("2 * 3 * 4", "2 \\* 3 \\* 4")]
    #[case::brackets("[not a link]", "\\[not a link\\]")]
    #[case::html("<div>", "\\<div>")]
    #[case::snake_case_stays("snake_case_name", "snake_case_name")]
    #[case::emphasis_underscores("_almost_", "\\_almost\\_")]
    fn plain_text_is_escaped(#[case] text: &str, #[case] expected: &str) {
        assert_eq!(markdown(&doc(json!([paragraph(json!([text_node(text)]))]))), expected);
    }

    #[test]
    fn code_spans_grow_around_backticks() {
        let value = doc(json!([paragraph(json!([marked_node("a`b", json!([{ "type": "code" }]))]))]));
        assert_eq!(markdown(&value), "``a`b``");
    }

    #[test]
    fn a_hard_break_ends_the_line() {
        let value = doc(json!([paragraph(json!([text_node("a"), { "type": "hardBreak" }, text_node("b")]))]));
        assert_eq!(markdown(&value), "a  \nb");
    }

    #[test]
    fn bullet_lists_nest() {
        let nested =
            json!({ "type": "bulletList", "content": [item(json!([paragraph(json!([text_node("inner")]))]))] });
        let value = doc(json!([{ "type": "bulletList", "content": [
            item(json!([paragraph(json!([text_node("outer")])), nested])),
            item(json!([paragraph(json!([text_node("next")]))]))
        ] }]));
        assert_eq!(markdown(&value), "- outer\n  - inner\n- next");
    }

    #[test]
    fn ordered_lists_count_from_their_start() {
        let value = doc(json!([{ "type": "orderedList", "attrs": { "order": 3 }, "content": [
            item(json!([paragraph(json!([text_node("three")]))])),
            item(json!([paragraph(json!([text_node("four")]))]))
        ] }]));
        assert_eq!(markdown(&value), "3. three\n4. four");
    }

    #[test]
    fn task_lists_become_checkboxes() {
        let value = doc(json!([{ "type": "taskList", "attrs": { "localId": "l" }, "content": [
            { "type": "taskItem", "attrs": { "state": "DONE" }, "content": [text_node("Reproduce")] },
            { "type": "taskItem", "attrs": { "state": "TODO" }, "content": [text_node("Fix")] }
        ] }]));
        assert_eq!(markdown(&value), "- [x] Reproduce\n- [ ] Fix");
    }

    #[test]
    fn code_blocks_keep_their_language() {
        let value = doc(json!([{ "type": "codeBlock", "attrs": { "language": "rust" }, "content": [
            text_node("fn main() {}\n")
        ] }]));
        assert_eq!(markdown(&value), "```rust\nfn main() {}\n```");
    }

    #[test]
    fn a_code_block_holding_a_fence_gets_a_longer_one() {
        let value = doc(json!([{ "type": "codeBlock", "content": [text_node("```")] }]));
        assert_eq!(markdown(&value), "````\n```\n````");
    }

    #[test]
    fn quotes_and_panels_are_quoted() {
        let value = doc(json!([
            { "type": "blockquote", "content": [paragraph(json!([text_node("a")])), paragraph(json!([text_node("b")]))] },
            { "type": "panel", "attrs": { "panelType": "info" }, "content": [paragraph(json!([text_node("c")]))] }
        ]));
        assert_eq!(markdown(&value), "> a\n>\n> b\n\n> c");
    }

    #[test]
    fn a_rule_is_a_thematic_break() {
        let value = doc(json!([paragraph(json!([text_node("a")])), { "type": "rule" }]));
        assert_eq!(markdown(&value), "a\n\n---");
    }

    #[test]
    fn tables_become_pipe_tables() {
        let cell = |kind: &str, text: &str| json!({ "type": kind, "content": [paragraph(json!([text_node(text)]))] });
        let value = doc(json!([{ "type": "table", "content": [
            { "type": "tableRow", "content": [cell("tableHeader", "Key"), cell("tableHeader", "Value")] },
            { "type": "tableRow", "content": [cell("tableCell", "a|b"), cell("tableCell", "1")] },
            { "type": "tableRow", "content": [cell("tableCell", "short")] }
        ] }]));
        assert_eq!(markdown(&value), "| Key | Value |\n| --- | --- |\n| a\\|b | 1 |\n| short |  |");
    }

    #[rstest]
    #[case::mention(json!({ "type": "mention", "attrs": { "id": "1", "text": "@Ana" } }), "@Ana")]
    #[case::mention_without_at(json!({ "type": "mention", "attrs": { "id": "1", "text": "Ana" } }), "@Ana")]
    #[case::emoji(json!({ "type": "emoji", "attrs": { "shortName": ":smile:", "text": "😄" } }), "😄")]
    #[case::emoji_name(json!({ "type": "emoji", "attrs": { "shortName": ":party:" } }), ":party:")]
    #[case::card(json!({ "type": "inlineCard", "attrs": { "url": "https://x.dev" } }), "<https://x.dev>")]
    #[case::date(json!({ "type": "date", "attrs": { "timestamp": "1759708800000" } }), "2025-10-06")]
    #[case::status(json!({ "type": "status", "attrs": { "text": "IN REVIEW", "color": "blue" } }), "`IN REVIEW`")]
    #[case::media(json!({ "type": "mediaInline", "attrs": { "id": "x" } }), "")]
    fn inline_nodes_become_text(#[case] node: Value, #[case] expected: &str) {
        assert_eq!(markdown(&doc(json!([paragraph(json!([node]))]))), expected);
    }

    #[test]
    fn an_expand_shows_its_title() {
        let value = doc(json!([{ "type": "expand", "attrs": { "title": "Logs" }, "content": [
            paragraph(json!([text_node("trace")]))
        ] }]));
        assert_eq!(markdown(&value), "**Logs**\n\ntrace");
    }

    #[test]
    fn unknown_nodes_show_their_text() {
        let value = doc(json!([
            { "type": "layoutSection", "content": [{ "type": "layoutColumn", "content": [paragraph(json!([text_node("left")]))] }] },
            { "type": "mediaSingle", "content": [{ "type": "media", "attrs": { "id": "x" } }] },
            { "type": "somethingNew", "content": [text_node("inline")] }
        ]));
        assert_eq!(markdown(&value), "left\n\ninline");
    }
}
