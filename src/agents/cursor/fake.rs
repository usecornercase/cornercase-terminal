use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};

use crate::agents::cursor::store::slug;
use crate::test_util::{Sleeper, TITLE_SIGNAL, TempDir, append, signal, write_executable};

pub(crate) const ID: &str = "22222222-2222-4222-8222-222222222222";
pub(crate) const BUCKET: &str = "0123456789abcdef0123456789abcdef";

fn varint(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let byte = u8::try_from(value & 0x7f).expect("seven bits");
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return out;
        }
        out.push(byte | 0x80);
    }
}

pub(crate) fn bytes(number: u64, data: &[u8]) -> Vec<u8> {
    [varint(number << 3 | 2), varint(data.len() as u64), data.to_vec()].concat()
}

pub(crate) fn number(number: u64, value: u64) -> Vec<u8> {
    [varint(number << 3), varint(value)].concat()
}

fn hash(data: &[u8]) -> Vec<u8> {
    Sha256::digest(data).to_vec()
}

pub(crate) fn hex(data: &[u8]) -> String {
    use std::fmt::Write;
    data.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

fn now_ms() -> u64 {
    u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_millis()).expect("milliseconds")
}

#[derive(Clone)]
struct Turn {
    user: Vec<u8>,
    message: String,
    steps: Vec<Vec<u8>>,
    routed: Option<String>,
    shell: bool,
}

struct State {
    turns: Vec<Turn>,
    tokens: Option<(u64, u64)>,
    model: Option<String>,
    created: u64,
}

pub(crate) struct FakeCursor {
    pub(crate) dir: TempDir,
    pub(crate) script: PathBuf,
    pub(crate) store: PathBuf,
    pub(crate) transcript: PathBuf,
    chat: PathBuf,
    state: Mutex<State>,
}

fn create_store(store: &Path) {
    Connection::open(store)
        .and_then(|db| {
            db.execute_batch(
                "CREATE TABLE blobs (id TEXT PRIMARY KEY, data BLOB); CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT);",
            )
        })
        .expect("create the store");
}

fn write_script(dir: &Path) -> PathBuf {
    let script = dir.join("cursor-agent");
    write_executable(
        &script,
        &format!(
            "#!/bin/sh\nexec 3< \"$1\"\nwhile [ -d \"$2\" ] && [ ! -e \"$2/quit\" ]; do\n  {TITLE_SIGNAL}\n  sleep 0.02\ndone\n"
        ),
    );
    script
}

impl FakeCursor {
    pub(crate) fn new(cwd: &Path) -> Self {
        let dir = TempDir::new();
        let chat = dir.path().join("config/chats").join(BUCKET).join(ID);
        let transcripts = dir.path().join("data/projects").join(slug(&cwd.to_string_lossy())).join("agent-transcripts");
        let transcript = transcripts.join(ID).join(format!("{ID}.jsonl"));
        for folder in [&chat, &transcript.parent().expect("transcript folder").to_path_buf()] {
            std::fs::create_dir_all(folder).expect("create the Cursor folders");
        }
        std::fs::write(&transcript, "").expect("write the transcript");
        std::fs::write(
            chat.join("meta.json"),
            serde_json::json!({"schemaVersion": 1, "cwd": cwd, "hasConversation": false}).to_string(),
        )
        .expect("write meta.json");
        let store = chat.join("store.db");
        create_store(&store);
        let script = write_script(dir.path());
        let fake = Self {
            dir,
            script,
            store,
            transcript,
            chat,
            state: Mutex::new(State {
                turns: Vec::new(),
                tokens: None,
                model: Some("default".into()),
                created: now_ms(),
            }),
        };
        fake.save();
        fake
    }

    pub(crate) fn command_line(&self) -> String {
        let args = [&self.script, &self.store, self.dir.path()].map(|p| p.to_string_lossy().into_owned());
        format!(
            "env CURSOR_DATA_DIR={} {}",
            crate::agents::quote(&self.dir.path().join("data").to_string_lossy()),
            crate::agents::join_args(&args)
        )
    }

    pub(crate) fn start(&self, cwd: &Path) -> Sleeper {
        let data = self.dir.path().join("data");
        Sleeper::holding(&self.script, cwd, &[("CURSOR_DATA_DIR", &data.to_string_lossy())], &self.store)
    }

    fn insert(db: &Connection, data: &[u8]) -> Vec<u8> {
        let id = hash(data);
        db.execute("INSERT OR IGNORE INTO blobs (id, data) VALUES (?1, ?2)", params![hex(&id), data])
            .expect("write a blob");
        id
    }

    fn save(&self) {
        let state = self.state.lock();
        let db = Connection::open(&self.store).expect("open the store");
        let mut root = Vec::new();
        for turn in &state.turns {
            let blob = Self::turn_blob(&db, turn);
            root.extend(bytes(8, &Self::insert(&db, &blob)));
        }
        if let Some((used, window)) = state.tokens {
            root.extend(bytes(5, &[number(1, used), number(2, window)].concat()));
        }
        let latest = if state.turns.is_empty() && state.tokens.is_none() {
            String::new()
        } else {
            hex(&Self::insert(&db, &root))
        };
        Self::put_meta(&db, &state, &latest);
        let has = !state.turns.is_empty();
        std::fs::write(
            self.chat.join("meta.json"),
            serde_json::json!({"schemaVersion": 1, "cwd": self.cwd(), "hasConversation": has}).to_string(),
        )
        .expect("write meta.json");
    }

    fn turn_blob(db: &Connection, turn: &Turn) -> Vec<u8> {
        if turn.shell {
            return bytes(2, &bytes(1, turn.message.as_bytes()));
        }
        let mut inner = bytes(1, &Self::insert(db, &turn.user));
        for step in &turn.steps {
            inner.extend(bytes(2, &Self::insert(db, step)));
        }
        if let Some(name) = &turn.routed {
            inner.extend(bytes(7, name.as_bytes()));
        }
        inner.extend(bytes(10, turn.message.as_bytes()));
        bytes(1, &inner)
    }

    fn cwd(&self) -> String {
        let text = std::fs::read_to_string(self.chat.join("meta.json")).expect("read meta.json");
        serde_json::from_str::<serde_json::Value>(&text).expect("meta.json")["cwd"].as_str().expect("cwd").to_string()
    }

    fn put_meta(db: &Connection, state: &State, latest: &str) {
        let mut meta = serde_json::json!({"latestRootBlobId": latest, "createdAt": state.created, "mode": "default"});
        if let Some(model) = &state.model {
            meta["lastUsedModel"] = model.clone().into();
        }
        db.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('0', ?1)",
            params![hex(meta.to_string().as_bytes())],
        )
        .expect("write the meta row");
    }

    pub(crate) fn turn(&self, text: &str) {
        self.turn_at(text, Some(now_ms()));
    }

    pub(crate) fn turn_at(&self, text: &str, started: Option<u64>) {
        let message = {
            let state = self.state.lock();
            format!("message-{}", state.turns.len())
        };
        let mut user = [bytes(1, text.as_bytes()), bytes(2, message.as_bytes())].concat();
        user.extend(started.map(|ms| number(25, ms)).unwrap_or_default());
        self.state.lock().turns.push(Turn { user, message, steps: Vec::new(), routed: None, shell: false });
        let line = serde_json::json!({"role": "user", "message": {"content": [{"type": "text", "text": text}]}});
        append(&self.transcript, &format!("{line}\n"));
        self.save();
    }

    pub(crate) fn turns(&self, count: usize) {
        {
            let mut state = self.state.lock();
            for n in 0..count {
                let message = format!("message-{}", state.turns.len());
                let user = [bytes(1, format!("prompt {n}").as_bytes()), bytes(2, message.as_bytes())].concat();
                state.turns.push(Turn { user, message, steps: Vec::new(), routed: None, shell: false });
            }
        }
        self.save();
    }

    pub(crate) fn shell_turn(&self, command: &str) {
        self.state.lock().turns.push(Turn {
            user: Vec::new(),
            message: command.to_string(),
            steps: Vec::new(),
            routed: None,
            shell: true,
        });
        self.save();
    }

    fn step(&self, step: Vec<u8>) {
        if let Some(turn) = self.state.lock().turns.last_mut() {
            turn.steps.push(step);
        }
        self.save();
    }

    pub(crate) fn reply(&self, text: &str) {
        let reply = [bytes(1, text.as_bytes()), number(2, 1_790_000_000_000), number(3, 1_790_000_001_000)].concat();
        self.step(bytes(1, &reply));
        let line = serde_json::json!({"role": "assistant", "message": {"content": [{"type": "text", "text": text}]}});
        append(&self.transcript, &format!("{line}\n"));
    }

    pub(crate) fn thinking(&self, text: &str) {
        self.step(bytes(3, &bytes(1, text.as_bytes())));
    }

    pub(crate) fn tool(&self, output: &str) {
        self.step(bytes(2, &bytes(1, output.as_bytes())));
    }

    pub(crate) fn end(&self, status: &str) {
        append(&self.transcript, &format!("{}\n", serde_json::json!({"type": "turn_ended", "status": status})));
    }

    pub(crate) fn routed(&self, name: &str) {
        if let Some(turn) = self.state.lock().turns.last_mut() {
            turn.routed = Some(name.to_string());
        }
        self.save();
    }

    pub(crate) fn context(&self, used: u64, window: u64) {
        self.state.lock().tokens = Some((used, window));
        self.save();
    }

    pub(crate) fn model(&self, id: &str) {
        self.state.lock().model = Some(id.to_string());
        self.save();
    }

    pub(crate) fn born(&self, millis: u64) {
        self.state.lock().created = millis;
        self.save();
    }

    pub(crate) fn root(&self, id: &str) {
        let db = Connection::open(&self.store).expect("open the store");
        let state = self.state.lock();
        Self::put_meta(&db, &state, id);
    }

    pub(crate) fn blob(&self, data: &[u8]) -> String {
        let db = Connection::open(&self.store).expect("open the store");
        hex(&Self::insert(&db, data))
    }

    pub(crate) fn title(&self, title: &str) {
        signal(self.dir.path(), "title", title);
    }

    pub(crate) fn quit(&self) {
        std::fs::write(self.dir.path().join("quit"), "").expect("signal fake Cursor");
    }
}

impl Drop for FakeCursor {
    fn drop(&mut self) {
        self.quit();
    }
}
