use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::agents::grok::session::encode_path;
use crate::test_util::{TempDir, append, write_executable};
use crate::time::Stamp;

pub(crate) struct FakeGrok {
    pub(crate) dir: TempDir,
    pub(crate) script: PathBuf,
    session: PathBuf,
    cwd: PathBuf,
}

impl FakeGrok {
    pub(crate) const ID: &str = "11111111-1111-4111-8111-111111111111";

    pub(crate) fn new(cwd: &Path) -> Self {
        let dir = TempDir::new();
        let session = dir.path().join("sessions").join(encode_path(&cwd.to_string_lossy())).join(Self::ID);
        std::fs::create_dir_all(&session).expect("create the Grok session");
        std::fs::write(
            session.join("summary.json"),
            r#"{"info":{"id":"11111111-1111-4111-8111-111111111111","cwd":"replaced"},"current_model_id":"grok-4.7","context_window":256000}"#
                .replace("replaced", &cwd.to_string_lossy()),
        )
        .expect("write the summary");
        std::fs::write(
            session.join("signals.json"),
            r#"{"contextWindowTokens":256000,"contextTokensUsed":0,"contextWindowUsage":0,"primaryModelId":"grok-4.7"}"#,
        )
        .expect("write the signals");
        std::fs::write(session.join("events.jsonl"), "").expect("write the events");
        std::fs::write(
            session.join("updates.jsonl"),
            r#"{"timestamp":1790000000,"method":"session/update","params":{"update":{"sessionUpdate":"turn_completed","prompt_id":"prompt-0","usage":{"inputTokens":20480,"outputTokens":12,"cachedReadTokens":1024,"cacheCreationTokens":0,"reasoningTokens":4,"modelCalls":1}}}}
"#,
        )
        .expect("write the updates");
        std::fs::write(
            session.join("chat_history.jsonl"),
            r#"{"type":"user","prompt_index":0,"content":[{"type":"text","text":"<user_query>\nsay hi\n</user_query>"}]}
{"type":"assistant","content":"Hi.","model_id":"grok-4.7"}
"#,
        )
        .expect("write the chat");
        let script = dir.path().join("grok");
        write_executable(&script, "#!/bin/sh\nwhile IFS= read -r line; do :; done\n");
        Self { dir, script, session, cwd: cwd.to_path_buf() }
    }

    pub(crate) fn command_line(&self) -> String {
        format!(
            "env GROK_HOME={} {}",
            crate::agents::quote(&self.dir.path().to_string_lossy()),
            crate::agents::quote(&self.script.to_string_lossy())
        )
    }

    pub(crate) fn bind(&self, pid: i32) {
        let start = crate::process::started(pid).unwrap_or(0);
        let row = serde_json::json!([{
            "session_id": Self::ID,
            "pid": pid,
            "pid_start": start,
            "cwd": self.cwd,
        }]);
        std::fs::write(self.dir.path().join("active_sessions.json"), row.to_string())
            .expect("write the active sessions");
    }

    pub(crate) fn add_prompt(&self, millis: u64) {
        let line = format!(
            "{{\"timestamp\":1791925800,\"params\":{{\"_meta\":{{\"agentTimestampMs\":{millis}}},\"update\":{{\"sessionUpdate\":\"user_message_chunk\",\"content\":{{\"type\":\"text\",\"text\":\"later\"}},\"_meta\":{{\"promptIndex\":0}}}}}}}}\n"
        );
        append(&self.session.join("updates.jsonl"), &line);
    }

    pub(crate) fn report(&self, status: &str) {
        let now = Stamp(SystemTime::now()).to_string();
        let line = match status {
            "busy" => format!(
                r#"{{"ts":"{now}","type":"turn_started","session_id":"{}","turn_number":1,"model_id":"grok-4.7","session_relationship":"primary"}}"#,
                Self::ID
            ),
            "waiting" => format!(r#"{{"ts":"{now}","type":"tool_started","tool_name":"ask_user_question"}}"#),
            _ => format!(r#"{{"ts":"{now}","type":"turn_ended","outcome":"completed"}}"#),
        };
        append(&self.session.join("events.jsonl"), &format!("{line}\n"));
    }
}
