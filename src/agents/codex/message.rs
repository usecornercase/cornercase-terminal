use std::path::Path;

use serde_json::Value;

use crate::agents::context::message::{Said, joined, lines, texts, unreadable};
use crate::error::Result;

const CODEX_ASSISTANT: &str = r#""role":"assistant""#;

pub(in crate::agents) fn codex(path: &Path) -> Result<Option<Said>> {
    let Some(lines) = lines(path)? else { return Ok(None) };
    for line in lines {
        let line = line.map_err(|e| unreadable(path, &e))?;
        if let Some(message) = codex_message(&line) {
            return Ok(Some(message));
        }
    }
    Ok(None)
}

fn codex_message(line: &[u8]) -> Option<Said> {
    let line = std::str::from_utf8(line).ok().filter(|line| line.contains(CODEX_ASSISTANT))?;
    let entry: Value = serde_json::from_str(line).ok()?;
    let payload = entry.get("payload")?;
    let reply = entry.get("type")?.as_str()? == "response_item"
        && payload.get("type")?.as_str()? == "message"
        && payload.get("role")?.as_str()? == "assistant";
    let text = joined(texts(payload.get("content").filter(|_| reply), "output_text"))?;
    Some(Said { text, written: entry.get("timestamp").and_then(Value::as_str).map(str::to_string) })
}

#[cfg(test)]
mod tests;
