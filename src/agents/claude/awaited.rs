use std::collections::HashSet;
use std::fmt;
use std::path::Path;

use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::agents::claude::transcript::{ASSISTANT, USER};
use crate::agents::context::message::{lines, unreadable};
use crate::agents::context::{Awaited, Choice, Question};
use crate::error::Result;

const ASK: &str = "AskUserQuestion";
const SPAWNS: [&str; 2] = ["Task", "Agent"];

#[derive(Deserialize)]
struct Entry {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(rename = "isSidechain", default)]
    sidechain: bool,
    #[serde(rename = "isMeta", default)]
    meta: bool,
    message: Option<Body>,
}

#[derive(Deserialize)]
struct Body {
    id: Option<String>,
    #[serde(default, deserialize_with = "blocks")]
    content: Vec<Block>,
}

#[derive(Deserialize)]
struct Block {
    #[serde(rename = "type")]
    kind: Option<String>,
    id: Option<String>,
    name: Option<String>,
    tool_use_id: Option<String>,
    input: Option<Value>,
}

impl Block {
    fn is(&self, kind: &str) -> bool {
        self.kind.as_deref() == Some(kind)
    }
}

fn blocks<'de, D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Vec<Block>, D::Error> {
    struct Blocks;

    impl<'de> Visitor<'de> for Blocks {
        type Value = Vec<Block>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a text or a list of blocks")
        }

        fn visit_str<E>(self, _: &str) -> std::result::Result<Vec<Block>, E> {
            Ok(Vec::new())
        }

        fn visit_unit<E>(self) -> std::result::Result<Vec<Block>, E> {
            Ok(Vec::new())
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<Vec<Block>, A::Error> {
            let mut blocks = Vec::new();
            while let Some(block) = seq.next_element()? {
                blocks.push(block);
            }
            Ok(blocks)
        }
    }

    deserializer.deserialize_any(Blocks)
}

enum Said {
    Asked { id: Option<String>, blocks: Vec<Block> },
    Answered(Vec<Block>),
}

impl Said {
    fn read(line: &[u8]) -> Option<Self> {
        let line = std::str::from_utf8(line).ok().filter(|line| line.contains(ASSISTANT) || line.contains(USER))?;
        let entry: Entry = serde_json::from_str(line).ok()?;
        let body = entry.message.filter(|_| !entry.sidechain)?;
        match entry.kind.as_deref()? {
            "assistant" => Some(Self::Asked { id: body.id, blocks: body.content }),
            "user" if !entry.meta => Some(Self::Answered(body.content)),
            _ => None,
        }
    }
}

struct Call {
    name: String,
    input: Value,
}

#[derive(Default)]
struct Turn {
    started: bool,
    id: Option<String>,
    resolved: HashSet<String>,
    calls: Vec<Call>,
}

impl Turn {
    fn take(&mut self, said: Said) -> bool {
        match said {
            Said::Answered(blocks) => {
                let results: Vec<String> = blocks
                    .into_iter()
                    .filter(|block| block.is("tool_result"))
                    .filter_map(|block| block.tool_use_id)
                    .collect();
                let answered = !results.is_empty();
                self.resolved.extend(results);
                answered
            }
            Said::Asked { id, blocks } => {
                if self.started && (id.is_none() || id != self.id) {
                    return false;
                }
                self.started = true;
                self.id = id;
                self.called(blocks);
                true
            }
        }
    }

    fn called(&mut self, blocks: Vec<Block>) {
        let calls = blocks
            .into_iter()
            .filter(|block| block.is("tool_use"))
            .filter(|block| block.id.as_ref().is_none_or(|id| !self.resolved.contains(id)))
            .filter_map(|block| Some(Call { name: block.name?, input: block.input.unwrap_or_default() }));
        let calls: Vec<Call> = calls.collect();
        self.calls.extend(calls.into_iter().rev());
    }

    fn awaited(mut self) -> Option<Awaited> {
        self.calls.reverse();
        if let Some(ask) = self.calls.iter().find(|call| call.name == ASK) {
            return Some(Awaited::questions(questions(&ask.input)));
        }
        let call = self.calls.into_iter().find(|call| !SPAWNS.contains(&call.name.as_str()))?;
        let command = call.input["command"].as_str().map(str::to_string);
        Some(Awaited::permission(call.name, command))
    }
}

fn text(value: &Value) -> Option<String> {
    value.as_str().filter(|text| !text.is_empty()).map(str::to_string)
}

fn listed(value: &Value) -> impl Iterator<Item = &Value> {
    value.as_array().into_iter().flatten()
}

fn questions(input: &Value) -> Vec<Question> {
    listed(&input["questions"])
        .map(|asked| Question {
            header: text(&asked["header"]),
            text: asked["question"].as_str().unwrap_or_default().to_string(),
            multi_select: asked["multiSelect"].as_bool().unwrap_or_default(),
            free_text: true,
            options: listed(&asked["options"])
                .enumerate()
                .map(|(at, option)| Choice {
                    number: at + 1,
                    label: option["label"].as_str().unwrap_or_default().to_string(),
                    description: text(&option["description"]),
                    selected: None,
                })
                .collect(),
        })
        .collect()
}

pub(in crate::agents) fn awaited(path: &Path) -> Result<Option<Awaited>> {
    let Some(lines) = lines(path)? else { return Ok(None) };
    let mut turn = Turn::default();
    for line in lines {
        let line = line.map_err(|e| unreadable(path, &e))?;
        let Some(said) = Said::read(&line) else { continue };
        if !turn.take(said) {
            break;
        }
    }
    Ok(turn.awaited())
}

#[cfg(test)]
mod tests;
