use std::io::BufRead;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::agents::context::message::{self, Each, Role, Said, Spoken};
use crate::error::Result;

fn journals(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir.join("journal"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "jsonl"))
        .collect();
    files.sort();
    files
}

fn replies(record: &Value) -> Vec<String> {
    let parts = &record["payload"]["result"]["result"]["parts"];
    let Some(parts) = parts.as_array() else { return Vec::new() };
    parts
        .iter()
        .filter(|part| part["type"].as_str() == Some("text"))
        .filter_map(|part| part["text"].as_str())
        .map(str::to_string)
        .collect()
}

fn prompt(record: &Value) -> Option<String> {
    let payload = &record["payload"];
    if record["type"].as_str() != Some("command_reserved") || payload["method"].as_str() != Some("turn/start") {
        return None;
    }
    let blocks = payload["params"]["message"].as_array()?;
    message::joined(
        blocks
            .iter()
            .filter(|block| block["type"].as_str() == Some("text"))
            .filter_map(|block| block["text"].as_str().map(str::to_string)),
    )
}

fn spoken(role: Role, text: &str) -> Option<Spoken> {
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(Spoken { role, said: Said { text, written: None } })
}

fn emit(role: Role, text: &str, each: Each) -> bool {
    spoken(role, text).is_none_or(each)
}

fn speak(record: &Value, each: Each) -> bool {
    prompt(record).is_none_or(|text| emit(Role::Prompt, &text, each))
        && replies(record).iter().all(|text| emit(Role::Reply, text, each))
}

pub(in crate::agents) fn said_all(dir: &Path, each: Each) -> Result<()> {
    for path in journals(dir) {
        let Ok(file) = std::fs::File::open(&path) else { continue };
        for line in std::io::BufReader::new(file).lines() {
            let line = line.map_err(|e| message::unreadable(&path, &e))?;
            let Ok(record) = serde_json::from_str::<Value>(&line) else { continue };
            if !speak(&record, each) {
                return Ok(());
            }
        }
    }
    Ok(())
}

pub(in crate::agents) fn last_message(dir: &Path) -> Result<Option<Said>> {
    let mut files = journals(dir);
    files.reverse();
    for file in files {
        let Some(lines) = message::lines(&file)? else { continue };
        for line in lines {
            let line = line.map_err(|e| message::unreadable(&file, &e))?;
            let Ok(record) = serde_json::from_slice::<Value>(&line) else { continue };
            if let Some(text) = replies(&record).into_iter().next_back() {
                return Ok(spoken(Role::Reply, &text).map(|spoken| spoken.said));
            }
        }
    }
    Ok(None)
}
