use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::test_util::TempDir;

pub(crate) const ID: &str = "8c7d6e5f-4a3b-2c1d-0e9f-8a7b6c5d4e3f";
const GENERATION: &str = "0000000000000001";

#[derive(Clone, Copy, Default)]
pub(crate) struct Usage {
    pub(crate) input: u64,
    pub(crate) cached: u64,
    pub(crate) output: u64,
}

pub(crate) struct FakeVibe {
    pub(crate) home: TempDir,
    pub(crate) id: String,
}

impl FakeVibe {
    pub(crate) fn new() -> Self {
        Self::named(ID)
    }

    pub(crate) fn named(id: &str) -> Self {
        let home = TempDir::new();
        let fake = Self { home, id: id.to_string() };
        std::fs::create_dir_all(fake.session().join("journal")).expect("create the session");
        fake.turn("completed", "glm-5-3", Usage { input: 1_000, cached: 0, output: 200 }, now_ms());
        fake
    }

    pub(crate) fn session(&self) -> PathBuf {
        self.home.path().join("logs/session/unified").join(&self.id)
    }

    pub(crate) fn turn(&self, status: &str, model: &str, usage: Usage, completed: u64) {
        let session = self.session();
        write(
            &session.join("meta.json"),
            &json!({"session_id": self.id, "config": {"active_model": model, "model_name": model}}),
        );
        write(
            &session.join("CURRENT"),
            &json!({"generation": GENERATION, "session_id": self.id, "store_format": "mistral.vibe.unified-session-store/v1"}),
        );
        let generation = session.join("generations").join(GENERATION);
        write(
            &generation.join("projection-state.json"),
            &json!({"snapshot": {
                "latestTurn": {
                    "id": format!("turn-{completed}"),
                    "status": status,
                    "startedAt": completed.saturating_sub(1_000),
                    "completedAt": completed,
                },
                "session": {
                    "contextUsage": Self::usage(&usage),
                    "status": {"type": "idle"},
                    "tokenUsage": Self::usage(&usage),
                },
            }}),
        );
        let mut runtime = self.runtime();
        runtime["session_metadata"]["active_model"] = json!(model);
        write(&generation.join("runtime-state.json"), &runtime);
        write(&generation.join("checkpoint.json"), &json!({"compaction_count": self.compaction_count()}));
    }

    fn usage(usage: &Usage) -> Value {
        json!({
            "inputTokens": usage.input,
            "cachedInputTokens": usage.cached,
            "outputTokens": usage.output,
            "totalTokens": usage.input + usage.output,
        })
    }

    fn runtime(&self) -> Value {
        json!({
            "session_metadata": {"active_model": "glm-5-3", "cwd": "/work"},
            "core_settings": {"context": {
                "compaction": {"mode": "automatic"},
                "model_facts": {"compaction_threshold": 800_000, "context_window": 1_000_000},
            }},
            "subagents": {"children": self.children()},
        })
    }

    fn children(&self) -> Value {
        let file = self.session().join("runtime-state.json");
        serde_json::from_str::<Value>(&std::fs::read_to_string(file).unwrap_or_default())
            .ok()
            .map_or_else(|| json!({}), |value| value["subagents"]["children"].clone())
    }

    fn compaction_count(&self) -> u64 {
        let file = self.session().join("generations").join(GENERATION).join("checkpoint.json");
        serde_json::from_str::<Value>(&std::fs::read_to_string(file).unwrap_or_default())
            .ok()
            .and_then(|value| value["compaction_count"].as_u64())
            .unwrap_or_default()
    }

    pub(crate) fn write_runtime(&self, value: &Value) {
        write(&self.session().join("generations").join(GENERATION).join("runtime-state.json"), value);
    }

    pub(crate) fn subagents(&self, states: &[(&str, &str)]) {
        let children: serde_json::Map<String, Value> = states
            .iter()
            .map(|(name, state)| ((*name).to_string(), json!({"child_session_id": name, "state": {"type": state}})))
            .collect();
        let generation = self.session().join("generations").join(GENERATION);
        let mut runtime = self.runtime();
        runtime["subagents"]["children"] = Value::Object(children);
        write(&generation.join("runtime-state.json"), &runtime);
    }

    pub(crate) fn compactions(&self, count: u64) {
        let generation = self.session().join("generations").join(GENERATION);
        write(&generation.join("checkpoint.json"), &json!({"compaction_count": count}));
    }

    pub(crate) fn journal(&self, records: &[Value]) {
        let file = self.session().join("journal").join("0000000000000001.jsonl");
        let mut text = String::new();
        for record in records {
            text.push_str(&record.to_string());
            text.push('\n');
        }
        std::fs::write(file, text).expect("write the journal");
    }

    pub(crate) fn lease(&self, pid: i32) {
        let active = self.home.path().join("logs/session/active");
        std::fs::create_dir_all(&active).expect("create the lease folder");
        write(&active.join(format!("{}.lock.json", self.id)), &json!({"process_id": pid, "session_id": self.id}));
    }

    pub(crate) fn path(&self) -> &Path {
        self.home.path()
    }
}

fn now_ms() -> u64 {
    u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()).unwrap_or_default()
}

fn write(path: &Path, value: &Value) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create the folder");
    }
    std::fs::write(path, value.to_string()).expect("write the file");
}
