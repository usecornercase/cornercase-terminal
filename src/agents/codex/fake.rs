use std::path::{Path, PathBuf};

use crate::test_util::{
    Running, Sleeper, TITLE_SIGNAL, TempDir, append, fixture, signal, wait_until, write_executable,
};

pub(crate) struct FakeCodex {
    pub(crate) dir: TempDir,
    pub(crate) home: PathBuf,
    pub(crate) script: PathBuf,
    pub(crate) rollout: PathBuf,
}

impl FakeCodex {
    pub(crate) fn new(id: &str, model: &str, wrapper: bool) -> Self {
        let dir = TempDir::new();
        let sessions = dir.path().join("sessions/2026/10/05");
        std::fs::create_dir_all(&sessions).expect("create sessions");
        let rollout = sessions.join(format!("rollout-2026-10-05T12-00-00-{id}.jsonl"));
        let fixture = fixture!("codex/0.160.0/context.jsonl")
            .replace("019a1234-5678-7000-8000-000000000001", id)
            .replace("gpt-5.4", model);
        std::fs::write(&rollout, fixture).expect("write rollout");
        let native = dir.path().join("codex");
        write_executable(
            &native,
            r#"#!/bin/sh
exec 3>> "$1"
while [ -d "$2" ] && [ ! -e "$2/quit" ]; do
  if mv "$2/switch" "$2/.switched" 2>/dev/null; then
    next=$(cat "$2/.switched")
    exec 3>&-
    exec 3>> "$next"
  fi
  $TITLE
  sleep 0.02
done
"#
            .replace("$TITLE", TITLE_SIGNAL)
            .as_str(),
        );
        let script = if wrapper {
            let wrapper = dir.path().join("node_modules/@openai/codex/bin/codex.js");
            std::fs::create_dir_all(wrapper.parent().expect("wrapper parent")).expect("create wrapper folder");
            write_executable(&wrapper, "#!/bin/sh\n\"$1\" \"$2\" \"$3\" &\nwait\n");
            wrapper
        } else {
            native
        };
        let home = dir.path().to_path_buf();
        Self { dir, home, script, rollout }
    }

    pub(crate) fn args(&self) -> Vec<PathBuf> {
        let mut args = Vec::new();
        if self.script.extension().is_some_and(|e| e == "js") {
            args.push(self.dir.path().join("codex"));
        }
        args.extend([self.rollout.clone(), self.dir.path().to_path_buf()]);
        args
    }

    pub(crate) fn command_line(&self) -> String {
        let args: Vec<String> =
            std::iter::once(self.script.clone()).chain(self.args()).map(|p| p.to_string_lossy().into_owned()).collect();
        format!(
            "env CODEX_HOME={} {}",
            crate::agents::quote(&self.home.to_string_lossy()),
            crate::agents::join_args(&args),
        )
    }

    pub(crate) fn signal(&self, name: &str, contents: &str) {
        signal(self.dir.path(), name, contents);
    }

    pub(crate) fn append(&self, text: &str) {
        append(&self.rollout, text);
    }

    pub(crate) fn serve(&self, cwd: &Path) -> Running {
        let text = std::fs::read_to_string(&self.rollout).expect("read rollout");
        std::fs::write(&self.rollout, text.replace("/tmp/project", &cwd.to_string_lossy())).expect("session folder");
        let script = self.dir.path().join("daemon/codex");
        std::fs::create_dir_all(script.parent().expect("daemon folder")).expect("create daemon folder");
        write_executable(
            &script,
            "#!/bin/sh\nexec 3>> \"$2\"\nwhile [ -d \"$3\" ] && [ ! -e \"$3/quit\" ]; do sleep 0.02; done\n",
        );
        let daemon = Running::spawn(
            std::process::Command::new(&script).arg("app-server").arg(&self.rollout).arg(self.dir.path()),
        );
        let pid = daemon.pid();
        let state = self.home.join("app-server-daemon");
        std::fs::create_dir_all(&state).expect("create daemon state");
        std::fs::write(state.join("daemon.pid"), format!("{{\"pid\":{pid}}}")).expect("write daemon pid");
        wait_until("the daemon opens the rollout", || crate::process::open_files(pid).contains(&self.rollout));
        daemon
    }

    pub(crate) fn agent(&self, cwd: &Path) -> Sleeper {
        Sleeper::named(&self.dir.path().join("agent/codex"), cwd, &[("CODEX_HOME", &self.home.to_string_lossy())])
    }
}

impl Drop for FakeCodex {
    fn drop(&mut self) {
        self.signal("quit", "");
    }
}
