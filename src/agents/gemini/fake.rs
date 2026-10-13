use std::path::{Path, PathBuf};

use crate::test_util::{Sleeper, TITLE_SIGNAL, TempDir, append, fixture, signal, write_executable};

pub(crate) struct FakeGemini {
    pub(crate) dir: TempDir,
    pub(crate) script: PathBuf,
    pub(crate) session: PathBuf,
}

impl FakeGemini {
    pub(crate) const ID: &str = "00000000-0000-4000-8000-000000000001";

    pub(crate) fn new(cwd: &Path) -> Self {
        let dir = TempDir::new();
        let project = dir.path().join(".gemini/tmp/project");
        std::fs::create_dir_all(project.join("chats")).expect("create Gemini chats");
        let registry = serde_json::json!({"projects": {cwd.to_string_lossy(): "project"}});
        std::fs::write(dir.path().join(".gemini/projects.json"), registry.to_string()).expect("write Gemini projects");
        std::fs::write(project.join(".project_root"), cwd.to_string_lossy().as_bytes()).expect("write Gemini owner");
        let session = project.join("chats/session-2026-10-10T12-00-00000000.jsonl");
        std::fs::write(&session, fixture!("gemini/0.63.0/initial.jsonl")).expect("write Gemini metadata");
        let script = dir.path().join("gemini");
        let body = r#"#!/bin/sh
watch_titles() {
  while [ -d "$2" ] && [ ! -e "$2/quit" ]; do
    $TITLE
    sleep 0.02
  done
}
watch_titles "$@" & watcher=$!
trap 'kill "$watcher" 2>/dev/null; wait "$watcher" 2>/dev/null' EXIT
printf '\033]0;◇  Ready (project)\007'
while IFS= read -r line; do
  case "$line" in
    /*|\?*|!*) printf 'local command\n' ;;
    *) printf '{"id":"prompt-%s","timestamp":"%s.999Z","type":"user","content":[{"text":"prompt"}]}\n' \
         "$n" "$(date -u +%Y-%m-%dT%H:%M:%S)" >> "$1"
       n=$((n + 1))
       printf '\033]0;✦  Working… (project)\007' ;;
  esac
done
"#
        .replace("$TITLE", TITLE_SIGNAL);
        write_executable(&script, &body);
        Self { dir, script, session }
    }

    pub(crate) fn command_line(&self) -> String {
        let args = [&self.script, &self.session, self.dir.path()].map(|p| p.to_string_lossy().into_owned());
        format!(
            "env GEMINI_CLI_HOME={} {}",
            crate::agents::quote(&self.dir.path().to_string_lossy()),
            crate::agents::join_args(&args)
        )
    }

    pub(crate) fn signal(&self, title: &str) {
        signal(self.dir.path(), "title", title);
    }

    pub(crate) fn append(&self, text: &str) {
        let file = append(&self.session, text);
        file.set_modified(std::time::SystemTime::now()).expect("set the write time");
    }

    pub(crate) fn start(&self, cwd: &Path) -> Sleeper {
        Sleeper::named(&self.script, cwd, &[("GEMINI_CLI_HOME", &self.dir.path().to_string_lossy())])
    }
}

impl Drop for FakeGemini {
    fn drop(&mut self) {
        signal(self.dir.path(), "quit", "");
    }
}
