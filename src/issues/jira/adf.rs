use serde_json::Value;

use crate::time::date;

use crate::http::text;
use crate::issues::md::{self, code_span, escape, item_lines, longest_run, one_line, prefix_lines};

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
    md::grid(&rows)
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

#[cfg(test)]
mod tests;
