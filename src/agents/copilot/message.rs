use std::path::Path;

use serde_json::Value;

use super::events::is_prompt;
use crate::agents::context::message::{self, Each, Role, Said, Spoken};
use crate::error::Result;

struct Chunks {
    call: String,
    written: Option<String>,
    parts: Vec<(u64, String)>,
}

impl Chunks {
    fn said(mut self) -> Option<Spoken> {
        self.parts.sort_by_key(|(index, _)| *index);
        let text = message::joined(self.parts.into_iter().map(|(_, text)| text))?;
        Some(Spoken { role: Role::Reply, said: Said { text, written: self.written } })
    }
}

fn reply(event: &Value) -> Option<&str> {
    let data = &event["data"];
    let main = data.get("parentToolCallId").is_none_or(Value::is_null);
    data["content"].as_str().filter(|content| main && !content.trim().is_empty())
}

fn flush_other_call(chunks: &mut Option<Chunks>, call: Option<&str>, each: Each) -> bool {
    if chunks.as_ref().is_some_and(|open| call != Some(open.call.as_str()))
        && let Some(spoken) = chunks.take().and_then(Chunks::said)
        && !each(spoken)
    {
        return false;
    }
    true
}

fn spoken_of(event: &Value, call: Option<&str>, chunks: &mut Option<Chunks>) -> Option<Spoken> {
    let data = &event["data"];
    let written = event["timestamp"].as_str().map(str::to_string);
    match event["type"].as_str() {
        Some("assistant.message") => {
            let text = reply(event)?;
            if data["chunkCount"].as_u64().is_some_and(|count| count > 1)
                && let Some(call) = call
            {
                let open = chunks.get_or_insert_with(|| Chunks { call: call.into(), written, parts: Vec::new() });
                open.parts.push((data["chunkIndex"].as_u64().unwrap_or(0), text.to_string()));
                return None;
            }
            message::joined([text.to_string()]).map(|text| Spoken { role: Role::Reply, said: Said { text, written } })
        }
        Some("user.message") if is_prompt(data) => data["content"]
            .as_str()
            .and_then(|text| message::joined([text.to_string()]))
            .map(|text| Spoken { role: Role::Prompt, said: Said { text, written } }),
        _ => None,
    }
}

pub(in crate::agents) fn said_all(dir: &Path, each: Each) -> Result<()> {
    let file = dir.join("events.jsonl");
    let Some(lines) = message::lines(&file)? else { return Ok(()) };
    let mut chunks: Option<Chunks> = None;
    for line in lines {
        let line = line.map_err(|e| message::unreadable(&file, &e))?;
        let Ok(event) = serde_json::from_slice::<Value>(&line) else { continue };
        let call = event["data"]["apiCallId"].as_str().filter(|_| event["type"].as_str() == Some("assistant.message"));
        if !flush_other_call(&mut chunks, call, each) {
            return Ok(());
        }
        if let Some(spoken) = spoken_of(&event, call, &mut chunks)
            && !each(spoken)
        {
            return Ok(());
        }
    }
    if let Some(spoken) = chunks.and_then(Chunks::said) {
        each(spoken);
    }
    Ok(())
}

pub(in crate::agents) fn last_message(dir: &Path) -> Result<Option<Said>> {
    let mut found = None;
    said_all(dir, &mut |spoken| {
        if spoken.role == Role::Reply {
            found = Some(spoken.said);
        }
        found.is_none()
    })?;
    Ok(found)
}

#[cfg(test)]
mod tests;
