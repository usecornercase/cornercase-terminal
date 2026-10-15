use std::path::Path;

use serde_json::Value;

use crate::agents::context::message::{Each, Role, Said, Spoken, joined, lines, texts, unreadable};
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

fn codex_spoken(line: &[u8]) -> Option<Spoken> {
    let line = std::str::from_utf8(line).ok().filter(|line| line.contains(r#""UserMessage""#))?;
    let entry: Value = serde_json::from_str(line).ok()?;
    let item = &entry["payload"]["item"];
    if entry["type"] != "event_msg" || entry["payload"]["type"] != "item_completed" || item["type"] != "UserMessage" {
        return None;
    }
    let text = joined(texts(item.get("content"), "text"))?;
    Some(Spoken { role: Role::Prompt, said: Said { text, written: entry["timestamp"].as_str().map(str::to_string) } })
}

pub(in crate::agents) fn said(path: &Path, each: Each) -> Result<()> {
    let Some(lines) = lines(path)? else { return Ok(()) };
    for line in lines {
        let line = line.map_err(|e| unreadable(path, &e))?;
        let spoken =
            codex_message(&line).map(|said| Spoken { role: Role::Reply, said }).or_else(|| codex_spoken(&line));
        if let Some(spoken) = spoken
            && !each(spoken)
        {
            return Ok(());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
