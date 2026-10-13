use std::path::{Path, PathBuf};

use crate::test_util::{TempDir, write_executable};

pub(crate) struct FakeClaude {
    dir: TempDir,
    _bin: TempDir,
    script: PathBuf,
}

impl FakeClaude {
    pub(crate) const WORKING: &str = r#"#!/bin/sh
s="$1/sessions/$$.json"
printf '{"pid":%s,"status":"busy"}' $$ > "$s"
while [ -d "$1" ] && [ ! -e "$1/finish" ]; do sleep 0.02; done
printf '{"pid":%s,"status":"idle"}' $$ > "$s"
while [ -d "$1" ] && [ ! -e "$1/quit" ]; do sleep 0.02; done
rm -f "$s"
"#;

    pub(crate) const SILENT: &str = "#!/bin/sh\nwhile [ -d \"$1\" ] && [ ! -e \"$1/quit\" ]; do sleep 0.02; done\n";

    pub(crate) const ANSWERING: &str = r#"#!/bin/sh
p=$(printf '%s' "$PWD" | tr -c 'a-zA-Z0-9' '-')
mkdir -p "$1/projects/$p"
printf '{"type":"assistant","message":{"model":"claude-opus-5-5","usage":{"input_tokens":2,"cache_creation_input_tokens":15655,"cache_read_input_tokens":149954}}}\n' > "$1/projects/$p/s1.jsonl"
printf '{"pid":%s,"sessionId":"s1","cwd":"%s","status":"idle"}' $$ "$PWD" > "$1/sessions/$$.json"
while [ -d "$1" ] && [ ! -e "$1/quit" ]; do sleep 0.02; done
rm -f "$1/sessions/$$.json"
"#;

    pub(crate) const CONTROL: &str = r#"#!/bin/sh
d="$1"; s="$d/sessions/$$.json"; t="$d/projects/$(printf %s "$PWD" | tr -c 'A-Za-z0-9' '-')"
mkdir -p "$t"
state() { printf '{"pid":%s,"sessionId":"s%s","cwd":"%s","status":"%s"}' $$ $$ "$PWD" "$1" > "$s"; }
state idle
n=0
while printf '\033[H\033[2J%s\n────────────\n❯ ' "$said" && IFS= read -r line; do
  case "$line" in *panel*) printf '\033[H\033[2J────────────\n  Shell details\n  x to stop\n'; read -r line; continue ;; esac
  case "$line" in *survey*)
    printf '\033[H\033[2J● How is Claude doing this session? (optional)\n  1: Bad    2: Fine   3: Good   0: Dismiss\n\n'
    printf '────────────\n❯ '; IFS= read -r line; [ "$line" = 0 ] && continue ;;
  esac
  n=$((n + 1))
  printf '{"type":"user","uuid":"p%s","origin":{"kind":"human"},"timestamp":"%s.999Z"}\n' "$n" "$(date -u +%Y-%m-%dT%H:%M:%S)" >> "$t/s$$.jsonl"
  state busy
  while [ ! -e "$d/finish" ]; do sleep 0.02; done
  rm -f "$d/finish"
  printf '{"type":"assistant","timestamp":"%s.999Z","message":{"id":"m%s","model":"synthetic","usage":{"input_tokens":20,"output_tokens":50,"cache_read_input_tokens":200,"cache_creation_input_tokens":10}}}\n' "$(date -u +%Y-%m-%dT%H:%M:%S)" "$n" >> "$t/s$$.jsonl"
  case "$line" in
    *ask*) state waiting; read answer ;;
    *background*) state shell
      while [ ! -e "$d/finish" ]; do sleep 0.02; done; rm -f "$d/finish" ;;
  esac
  said="done: $line"
  state idle
done
rm -f "$s"
"#;

    pub(crate) fn new() -> Self {
        Self::running(Self::WORKING)
    }

    pub(crate) fn running(script: &str) -> Self {
        let (dir, bin) = (TempDir::new(), TempDir::new());
        std::fs::create_dir(dir.path().join("sessions")).expect("create the sessions folder");
        let path = bin.path().join("claude");
        write_executable(&path, script);
        Self { dir, _bin: bin, script: path }
    }

    pub(crate) fn dir(&self) -> &Path {
        self.dir.path()
    }

    pub(crate) fn script(&self) -> &Path {
        &self.script
    }

    pub(crate) fn command_line(&self, args: &str) -> String {
        let unset = format!(
            "env -u {} -u {}",
            crate::agents::claude::transcript::NO_LONG_ENV,
            crate::agents::claude::transcript::NO_COMPACT_ENV
        );
        format!("{unset} {} {} {args}", self.script().display(), self.dir().display())
    }

    pub(crate) fn signal(&self, name: &str) {
        std::fs::write(self.dir().join(name), "").expect("write the signal");
    }

    pub(crate) fn report(&self, pid: i32, status: &str) {
        let next = self.dir().join("next.json");
        std::fs::write(&next, format!(r#"{{"pid":{pid},"status":"{status}"}}"#)).expect("write the session");
        let session = self.dir().join("sessions").join(format!("{pid}.json"));
        std::fs::rename(next, session).expect("put the session in place");
    }
}
