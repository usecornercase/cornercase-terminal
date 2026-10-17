use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use serde_json::{Value, json};

use crate::test_util::{Running, TempDir, append, signal, write_executable};
use crate::time::Stamp;

pub(crate) const ID: &str = "33333333-3333-4333-8333-333333333333";
pub(crate) const REQUEST: &str = "44444444-4444-4444-8444-444444444444";
pub(crate) const CALL: &str = "call_ask_1";

pub(crate) struct FakeCopilot {
    pub(crate) dir: TempDir,
    pub(crate) script: PathBuf,
    pub(crate) home: PathBuf,
    signals: PathBuf,
    cwd: PathBuf,
    counter: AtomicU64,
}

impl FakeCopilot {
    pub(crate) fn new(cwd: &Path) -> Self {
        let dir = TempDir::new();
        let home = dir.path().join("home");
        let signals = dir.path().join("signals");
        std::fs::create_dir_all(home.join("session-state").join(ID)).expect("create the Copilot session");
        std::fs::create_dir_all(&signals).expect("create the signal folder");
        let script = dir.path().join("copilot");
        write_executable(
            &script,
            &format!(
                "#!/bin/sh
[ -n \"$COPILOT_HOME\" ] || exec env COPILOT_HOME={home} \"$0\" \"$@\"
signals={signals}
id={ID}
resumed=
for arg in \"$@\"; do case $arg in --resume=*) id=${{arg#--resume=}}; resumed=1 ;; esac; done
dir=\"$COPILOT_HOME/session-state/$id\"
mkdir -p \"$dir\"
hold() {{ printf '%s' \"$$\" > \"$dir/inuse.$$.lock\"; exec 3>\"$dir/inuse.$$.hold\"; }}
[ -n \"$resumed\" ] || hold
while [ ! -e \"$signals/quit\" ]; do
  if [ -n \"$resumed\" ] && [ -e \"$signals/prompted\" ]; then hold; resumed=; fi
  if mv \"$signals/clear\" \"$signals/.cleared\" 2>/dev/null; then
    exec 3>&-
    rm -f \"$dir/inuse.$$.lock\" \"$dir/inuse.$$.hold\"
    dir=\"$COPILOT_HOME/session-state/$(cat \"$signals/.cleared\")\"
    mkdir -p \"$dir\"
    hold
  fi
  sleep 0.02
done
",
                home = crate::agents::quote(&home.to_string_lossy()),
                signals = crate::agents::quote(&signals.to_string_lossy()),
            ),
        );
        let fake = Self { dir, script, home, signals, cwd: cwd.to_path_buf(), counter: AtomicU64::new(0) };
        fake.working(false);
        fake
    }

    pub(crate) fn command_line(&self) -> String {
        crate::agents::quote(&self.script.to_string_lossy())
    }

    pub(crate) fn spawn(&self, program: &Path, args: &[&str]) -> Running {
        let mut command = std::process::Command::new(program);
        command
            .args(args)
            .env_remove("COPILOT_HOME")
            .current_dir(&self.cwd)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        Running::spawn(&mut command)
    }

    pub(crate) fn hold(&self, id: &str, pid: i32) -> PathBuf {
        self.session(id).join(format!("inuse.{pid}.hold"))
    }

    pub(crate) fn loader(&self) -> PathBuf {
        let path = self.dir.path().join("node_modules/@github/copilot/npm-loader.js");
        std::fs::create_dir_all(path.parent().expect("loader folder")).expect("create the loader folder");
        write_executable(
            &path,
            &format!("#!/bin/sh\n{} \"$@\"\nexit $?\n", crate::agents::quote(&self.script.to_string_lossy())),
        );
        path
    }

    pub(crate) fn session(&self, id: &str) -> PathBuf {
        self.home.join("session-state").join(id)
    }

    pub(crate) fn events(&self, id: &str) -> PathBuf {
        self.session(id).join("events.jsonl")
    }

    pub(crate) fn working(&self, working: bool) {
        self.presence(ID, working);
    }

    pub(crate) fn presence(&self, id: &str, working: bool) {
        let state = json!({id: {"schemaVersion": 1, "openedAt": Stamp(SystemTime::now()).to_string(), "refreshedAt": Stamp(SystemTime::now()).to_string(), "working": working}});
        let file = self.home.join("open-sessions-state.json");
        let next = self.home.join(".open-sessions-state.json");
        std::fs::write(&next, state.to_string()).expect("write the presence file");
        std::fs::rename(next, file).expect("put the presence file in place");
    }

    pub(crate) fn event_in(&self, id: &str, kind: &str, data: &Value, at: SystemTime) {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let line = json!({
            "type": kind,
            "data": data,
            "id": format!("00000000-0000-4000-8000-{n:012}"),
            "timestamp": Stamp(at).to_string(),
            "parentId": null,
        });
        let file = self.events(id);
        if !file.exists() {
            std::fs::File::create(&file).expect("create events.jsonl");
        }
        append(&file, &format!("{line}\n"));
    }

    pub(crate) fn event(&self, kind: &str, data: &Value) {
        self.event_in(ID, kind, data, SystemTime::now());
    }

    pub(crate) fn started(&self, model: &str) {
        let data = json!({"sessionId": ID, "selectedModel": model, "context": {"cwd": self.cwd}});
        self.event("session.start", &data);
    }

    pub(crate) fn prompt(&self, text: &str) {
        self.prompt_at(text, SystemTime::now());
    }

    pub(crate) fn prompt_at(&self, text: &str, at: SystemTime) {
        let data = json!({"content": text, "messageId": format!("message-{text}"), "delivery": "idle"});
        self.event_in(ID, "user.message", &data, at);
    }

    pub(crate) fn queued(&self, text: &str) {
        let data = json!({"content": text, "messageId": format!("queued-{text}"), "delivery": "queued", "agentMode": "interactive"});
        self.event("user.message", &data);
    }

    pub(crate) fn turn(&self, open: bool) {
        self.event(if open { "assistant.turn_start" } else { "assistant.turn_end" }, &json!({"turnId": "0"}));
    }

    pub(crate) fn reply(&self, text: &str, model: &str) {
        self.event("assistant.message", &json!({"content": text, "model": model, "apiCallId": format!("call-{text}")}));
    }

    pub(crate) fn subagent_reply(&self, text: &str) {
        self.event("assistant.message", &json!({"content": text, "parentToolCallId": "agent-1"}));
    }

    pub(crate) fn usage(&self, input: u64, max: u64, model: &str) {
        let usage = json!({
            "model": model, "inputTokens": input, "outputTokens": 10, "cacheReadTokens": 0, "cacheWriteTokens": 0,
            "maxPromptTokens": max, "interactionType": "conversation-agent", "initiator": "user",
        });
        self.event("session.usage_record", &json!({"usage": usage}));
    }

    pub(crate) fn subagent_usage(&self, input: u64, max: u64) {
        let usage = json!({
            "model": "gpt-4.1", "inputTokens": input, "outputTokens": 5, "cacheReadTokens": 0, "cacheWriteTokens": 0,
            "maxPromptTokens": max, "initiator": "sub-agent", "parentToolCallId": "agent-1",
        });
        self.event("session.usage_record", &json!({"usage": usage}));
    }

    pub(crate) fn permission(&self, open: bool) {
        if open {
            self.event("permission.requested", &json!({"requestId": REQUEST}));
        } else {
            self.event("permission.completed", &json!({"requestId": REQUEST}));
        }
    }

    pub(crate) fn ask(&self, open: bool) {
        if open {
            self.event("tool.execution_start", &json!({"toolCallId": CALL, "toolName": "ask_user"}));
        } else {
            self.event("human_response.recorded", &json!({"requestId": REQUEST, "toolCallId": CALL}));
            self.event("tool.execution_complete", &json!({"toolCallId": CALL, "success": true}));
        }
    }

    pub(crate) fn plan_review(&self) {
        self.event("tool.execution_start", &json!({"toolCallId": "call_plan", "toolName": "exit_plan_mode"}));
    }

    pub(crate) fn abort(&self) {
        self.event("abort", &json!({"reason": "user_initiated"}));
    }

    pub(crate) fn compacted(&self) {
        self.event("session.compaction_complete", &json!({"success": true}));
    }

    pub(crate) fn model_change(&self, model: &str) {
        self.event("session.model_change", &json!({"newModel": model, "source": "model_command"}));
    }

    pub(crate) fn auto_resolved(&self, model: &str) {
        self.event("session.auto_mode_resolved", &json!({"chosenModel": model}));
    }

    pub(crate) fn shell_started(&self, shell: &str) {
        let data = json!({
            "toolCallId": format!("call-{shell}"),
            "success": true,
            "result": {"content": format!("<command started in background with shellId: {shell}>")},
            "toolTelemetry": {"properties": {"executionMode": "async"}},
        });
        self.event("tool.execution_complete", &data);
    }

    pub(crate) fn shell_done(&self, shell: &str) {
        let kind = json!({"type": "shell_completed", "shellId": shell, "exitCode": 0});
        self.event("system.notification", &json!({"kind": kind}));
    }

    pub(crate) fn clear(&self, id: &str) {
        self.event("session.shutdown", &json!({"shutdownType": "routine"}));
        std::fs::create_dir_all(self.session(id)).expect("create the new session");
        signal(&self.signals, "clear", id);
    }

    pub(crate) fn prompted(&self) {
        std::fs::write(self.signals.join("prompted"), "").expect("signal fake Copilot");
    }

    pub(crate) fn quit(&self) {
        std::fs::write(self.signals.join("quit"), "").expect("signal fake Copilot");
    }
}

impl Drop for FakeCopilot {
    fn drop(&mut self) {
        self.quit();
    }
}
