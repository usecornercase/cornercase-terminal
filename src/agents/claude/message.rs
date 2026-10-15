use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::agents::claude::transcript::{ASSISTANT, SYNTHETIC};
use crate::agents::context::message::{Each, Role, Said, Spoken, joined, lines, texts, unreadable};
use crate::error::Result;

#[derive(Deserialize)]
struct Entry {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(rename = "isSidechain")]
    sidechain: Option<bool>,
    #[serde(rename = "isApiErrorMessage")]
    api_error: Option<bool>,
    timestamp: Option<String>,
    message: Option<Body>,
}

#[derive(Deserialize)]
struct Body {
    id: Option<String>,
    model: Option<String>,
    content: Option<Value>,
}

struct Reply {
    id: Option<String>,
    texts: Vec<String>,
    written: Option<String>,
}

impl Reply {
    pub(in crate::agents) fn claude(line: &[u8]) -> Option<Self> {
        let line = std::str::from_utf8(line).ok().filter(|line| line.contains(ASSISTANT))?;
        let entry: Entry = serde_json::from_str(line).ok()?;
        let body = entry.message.filter(|_| entry.kind.as_deref() == Some("assistant"))?;
        let synthetic = body.model.as_deref() == Some(SYNTHETIC) && entry.api_error != Some(true);
        if entry.sidechain == Some(true) || synthetic {
            return None;
        }
        Some(Self { id: body.id, texts: texts(body.content.as_ref(), "text"), written: entry.timestamp })
    }

    fn message(self) -> Option<Said> {
        Some(Said { text: joined(self.texts)?, written: self.written })
    }
}
pub(in crate::agents) fn claude(path: &Path) -> Result<Option<Said>> {
    let Some(lines) = lines(path)? else { return Ok(None) };
    let mut newest: Option<Reply> = None;
    for line in lines {
        let line = line.map_err(|e| unreadable(path, &e))?;
        let Some(reply) = Reply::claude(&line) else { continue };
        match &mut newest {
            None if joined(reply.texts.iter().cloned()).is_some() => newest = Some(reply),
            None => {}
            Some(found) if found.id.is_some() && found.id == reply.id => {
                found.texts.splice(0..0, reply.texts);
            }
            Some(_) => break,
        }
    }
    Ok(newest.and_then(Reply::message))
}

fn prompt(line: &[u8]) -> Option<Spoken> {
    let line = std::str::from_utf8(line).ok().filter(|line| line.contains("origin"))?;
    let entry: Value = serde_json::from_str(line).ok()?;
    if entry["isSidechain"] == true || entry["isMeta"] == true {
        return None;
    }
    let text = match entry["type"].as_str()? {
        "user" if entry["origin"]["kind"] == "human" => joined(texts(entry["message"].get("content"), "text"))?,
        "attachment"
            if entry["attachment"]["type"] == "queued_command"
                && entry["attachment"]["commandMode"] == "prompt"
                && entry["attachment"]["origin"]["kind"] == "human" =>
        {
            joined([entry["attachment"]["prompt"].as_str()?.to_string()])?
        }
        _ => return None,
    };
    Some(Spoken { role: Role::Prompt, said: Said { text, written: entry["timestamp"].as_str().map(str::to_string) } })
}

pub(in crate::agents) fn said(path: &Path, each: Each) -> Result<()> {
    let Some(lines) = lines(path)? else { return Ok(()) };
    for line in lines {
        let line = line.map_err(|e| unreadable(path, &e))?;
        if let Some(spoken) = prompt(&line) {
            if !each(spoken) {
                return Ok(());
            }
            continue;
        }
        let Some(reply) = Reply::claude(&line) else { continue };
        for text in reply.texts.into_iter().rev() {
            let Some(text) = joined([text]) else { continue };
            if !each(Spoken { role: Role::Reply, said: Said { text, written: reply.written.clone() } }) {
                return Ok(());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
