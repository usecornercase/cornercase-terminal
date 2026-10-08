use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags, params};
use serde::Deserialize;

use super::{Context, percent};
use crate::process;

const NAME: &str = "opencode";
const BUSY_FOR: Duration = Duration::from_millis(100);
const MESSAGES: i64 = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Session {
    pub turn: bool,
    pub context: Option<Context>,
}

#[derive(Debug, Default)]
pub(super) struct Models {
    last: Option<(Key, Option<Model>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Key {
    file: Option<(PathBuf, SystemTime)>,
    provider: String,
    model: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Model {
    name: String,
    window: Option<u64>,
}

impl Models {
    fn find(&mut self, file: Option<&Path>, provider: &str, model: &str) -> Option<Model> {
        let file = file.and_then(|path| Some((path.to_path_buf(), std::fs::metadata(path).ok()?.modified().ok()?)));
        let key = Key { file, provider: provider.to_string(), model: model.to_string() };
        if self.last.as_ref().is_none_or(|(last, _)| *last != key) {
            let found = key.file.as_ref().and_then(|(path, _)| read_model(path, provider, model));
            self.last = Some((key, found));
        }
        self.last.as_ref().and_then(|(_, found)| found.clone())
    }
}

#[derive(Deserialize)]
struct Provider {
    #[serde(default)]
    models: HashMap<String, Entry>,
}

#[derive(Deserialize)]
struct Entry {
    name: Option<String>,
    limit: Option<Limit>,
}

#[derive(Deserialize)]
struct Limit {
    context: Option<u64>,
}

fn read_model(path: &Path, provider: &str, model: &str) -> Option<Model> {
    let mut providers: HashMap<String, Provider> =
        serde_json::from_reader(BufReader::new(File::open(path).ok()?)).ok()?;
    let entry = providers.remove(provider)?.models.remove(model)?;
    Some(Model {
        name: entry.name.filter(|name| !name.trim().is_empty()).unwrap_or_else(|| model.to_string()),
        window: entry.limit.and_then(|limit| limit.context).filter(|window| *window > 0),
    })
}

pub(super) fn look(pid: i32, since: SystemTime, models: &mut Models) -> Option<Session> {
    let (holder, database) = database(pid)?;
    let cwd = folder(holder)?;
    if shared(holder, &database, &cwd) {
        return None;
    }
    let connection =
        Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
            .ok()?;
    connection.busy_timeout(BUSY_FOR).ok()?;
    let since = i64::try_from(since.duration_since(UNIX_EPOCH).ok()?.as_millis()).ok()?;
    let Some(id) = session(&connection, &cwd, since).ok()? else {
        return Some(Session { turn: false, context: None });
    };
    let messages = messages(&connection, &id)?;
    Some(read(&messages, models, models_file(holder).as_deref()))
}

fn database(pid: i32) -> Option<(i32, PathBuf)> {
    std::iter::once(pid).chain(process::descendants(pid).into_iter().filter(|&child| is_opencode(child))).find_map(
        |candidate| {
            let named = configured(candidate);
            let found = process::open_files(candidate).into_iter().find(|path| is_database(path, named.as_deref()))?;
            Some((candidate, found))
        },
    )
}

fn configured(pid: i32) -> Option<String> {
    let env = process::env(pid);
    let value = env.into_iter().find(|(key, value)| key == "OPENCODE_DB" && !value.is_empty())?.1;
    Path::new(&value).file_name()?.to_str().map(str::to_string)
}

fn is_database(path: &Path, named: Option<&str>) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else { return false };
    named.map_or_else(
        || name.starts_with(NAME) && Path::new(name).extension().is_some_and(|e| e == "db"),
        |named| name == named,
    )
}

fn is_opencode(pid: i32) -> bool {
    process::args(pid).iter().take(2).any(|arg| Path::new(arg).file_name().is_some_and(|name| name == NAME))
}

fn shared(holder: i32, database: &Path, cwd: &Path) -> bool {
    process::all().into_iter().any(|other| {
        other != holder
            && is_opencode(other)
            && folder(other).as_deref() == Some(cwd)
            && process::open_files(other).iter().any(|path| path == database)
    })
}

fn folder(pid: i32) -> Option<PathBuf> {
    process::cwd(pid)?.canonicalize().ok()
}

fn models_file(pid: i32) -> Option<PathBuf> {
    let env = process::env(pid);
    let var = |name| env.iter().find(|(key, value)| key == name && !value.is_empty()).map(|(_, v)| PathBuf::from(v));
    let path = var("OPENCODE_MODELS_PATH").or_else(|| {
        let cache = var("XDG_CACHE_HOME").or_else(|| var("HOME").map(|home| home.join(".cache")))?;
        Some(cache.join(NAME).join("models.json"))
    })?;
    if path.is_absolute() { Some(path) } else { Some(process::cwd(pid)?.join(path)) }
}

fn session(connection: &Connection, cwd: &Path, since: i64) -> rusqlite::Result<Option<String>> {
    let mut statement = connection.prepare(
        "SELECT id, directory FROM session WHERE parent_id IS NULL AND time_archived IS NULL \
         AND time_updated >= ?1 ORDER BY time_updated DESC",
    )?;
    let rows: Vec<(String, String)> =
        statement.query_map(params![since], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<Result<_, _>>()?;
    Ok(rows
        .into_iter()
        .find_map(|(id, directory)| (Path::new(&directory).canonicalize().ok().as_deref() == Some(cwd)).then_some(id)))
}

fn messages(connection: &Connection, id: &str) -> Option<Vec<String>> {
    let mut statement = connection
        .prepare("SELECT data FROM message WHERE session_id = ?1 ORDER BY time_created DESC, id DESC LIMIT ?2")
        .ok()?;
    let rows = statement.query_map(params![id, MESSAGES], |row| row.get::<_, String>(0)).ok()?;
    Some(rows.flatten().collect())
}

#[derive(Debug, Default, Deserialize)]
struct Message {
    role: Option<String>,
    #[serde(rename = "providerID")]
    provider: Option<String>,
    #[serde(rename = "modelID")]
    model: Option<String>,
    #[serde(rename = "model")]
    picked: Option<Picked>,
    time: Option<Times>,
    tokens: Option<Tokens>,
}

#[derive(Debug, Default, Deserialize)]
struct Picked {
    #[serde(rename = "providerID")]
    provider: Option<String>,
    #[serde(rename = "modelID")]
    model: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Times {
    completed: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Tokens {
    input: u64,
    output: u64,
    reasoning: u64,
    cache: Cache,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Cache {
    read: u64,
    write: u64,
}

impl Message {
    fn assistant(&self) -> bool {
        self.role.as_deref() == Some("assistant")
    }

    fn key(&self) -> Option<(&str, &str)> {
        let (provider, model) = if self.assistant() {
            (self.provider.as_deref(), self.model.as_deref())
        } else {
            let picked = self.picked.as_ref()?;
            (picked.provider.as_deref(), picked.model.as_deref())
        };
        Some((provider?, model.filter(|m| !m.trim().is_empty())?))
    }

    fn used(&self) -> Option<u64> {
        let tokens = self.tokens.as_ref().filter(|t| t.output > 0)?;
        let used = [tokens.input, tokens.output, tokens.reasoning, tokens.cache.read, tokens.cache.write]
            .into_iter()
            .fold(0_u64, u64::saturating_add);
        (used > 0).then_some(used)
    }
}

fn read(newest_first: &[String], models: &mut Models, file: Option<&Path>) -> Session {
    let messages: Vec<Message> = newest_first.iter().filter_map(|data| serde_json::from_str(data).ok()).collect();
    let turn = messages
        .iter()
        .find(|m| matches!(m.role.as_deref(), Some("user" | "assistant")))
        .is_some_and(|m| !m.assistant() || m.time.as_ref().is_none_or(|t| t.completed.is_none()));
    let context = messages.iter().find_map(Message::key).map(|(provider, model)| {
        let found = models.find(file, provider, model);
        let reply = messages.iter().find(|m| m.assistant() && m.used().is_some());
        let used = reply.filter(|reply| reply.key() == Some((provider, model))).and_then(Message::used);
        Context {
            model: found.as_ref().map_or_else(|| model.to_string(), |found| found.name.clone()),
            percent: used.zip(found.and_then(|found| found.window)).map(|(used, window)| percent(used, window)),
        }
    });
    Session { turn, context }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::context::Pane;
    use crate::test_util::{FakeOpencode, Sleeper, TempDir, wait_until};

    const REPLY: &str = include_str!("../../tests/fixtures/opencode/1.18.35/reply.jsonl");
    const WORKING: &str = include_str!("../../tests/fixtures/opencode/1.18.35/working.jsonl");
    const ABORTED: &str = include_str!("../../tests/fixtures/opencode/1.18.35/aborted.jsonl");
    const COMPACTION: &str = include_str!("../../tests/fixtures/opencode/1.18.35/compaction.jsonl");
    const MODELS: &str = include_str!("../../tests/fixtures/opencode/1.18.35/models.json");
    const FLASH: &str =
        r#"{"role":"user","time":{"created":1},"model":{"providerID":"deepseek","modelID":"deepseek-v4-flash"}}"#;

    struct Models {
        dir: TempDir,
        cache: super::Models,
    }

    impl Models {
        fn new() -> Self {
            let dir = TempDir::new();
            std::fs::write(dir.path().join("models.json"), MODELS).expect("write models.json");
            Self { dir, cache: super::Models::default() }
        }

        fn read(&mut self, oldest_first: &[&str]) -> Session {
            let mut rows: Vec<String> = oldest_first.iter().flat_map(|text| text.lines()).map(str::to_string).collect();
            rows.reverse();
            read(&rows, &mut self.cache, Some(&self.dir.path().join("models.json")))
        }
    }

    fn shown(model: &str, percent: Option<u16>) -> Context {
        Context { model: model.into(), percent }
    }

    #[rstest]
    #[case::a_reply(&[REPLY], false, Some(shown("DeepSeek V4 Pro", Some(12))))]
    #[case::a_turn_before_any_reply(&[WORKING], true, Some(shown("DeepSeek V4 Pro", None)))]
    #[case::a_turn_after_a_reply(&[REPLY, WORKING], true, Some(shown("DeepSeek V4 Pro", Some(12))))]
    #[case::an_interrupted_turn(&[REPLY, ABORTED], false, Some(shown("DeepSeek V4 Pro", Some(12))))]
    #[case::a_compaction(&[REPLY, COMPACTION], false, Some(shown("DeepSeek V4 Pro", Some(0))))]
    #[case::a_message_sent_before_its_reply_starts(&[REPLY, FLASH], true, Some(shown("DeepSeek V4 Flash", None)))]
    #[case::nothing_yet(&[], false, None)]
    fn the_versioned_messages_give_the_turn_and_the_line_opencode_shows(
        #[case] messages: &[&str],
        #[case] turn: bool,
        #[case] context: Option<Context>,
    ) {
        assert_eq!(Models::new().read(messages), Session { turn, context });
    }

    #[test]
    fn a_model_missing_from_models_dev_shows_its_id_alone() {
        let mut models = Models::new();
        let reply = REPLY.replace("deepseek-v4-pro", "my-local-model");

        assert_eq!(models.read(&[&reply]).context, Some(shown("my-local-model", None)));
        std::fs::remove_file(models.dir.path().join("models.json")).expect("remove models.json");
        assert_eq!(models.read(&[REPLY]).context, Some(shown("deepseek-v4-pro", None)));
    }

    #[test]
    fn messages_that_cannot_be_read_are_skipped() {
        assert_eq!(Models::new().read(&[REPLY, "{broken", "[]"]).context, Some(shown("DeepSeek V4 Pro", Some(12))));
    }

    #[rstest]
    #[case::default("/home/a/.local/share/opencode/opencode.db", None, true)]
    #[case::a_channel("/home/a/.local/share/opencode/opencode-beta-2.db", None, true)]
    #[case::its_write_ahead_log("/home/a/.local/share/opencode/opencode.db-wal", None, false)]
    #[case::another_database("/home/a/.local/share/other/app.db", None, false)]
    #[case::configured("/srv/state/agent.sqlite", Some("agent.sqlite"), true)]
    #[case::not_the_configured_one("/home/a/.local/share/opencode/opencode.db", Some("agent.sqlite"), false)]
    fn the_database_is_known_by_its_name(#[case] path: &str, #[case] named: Option<&str>, #[case] expected: bool) {
        assert_eq!(is_database(Path::new(path), named), expected);
    }

    struct Running {
        fake: FakeOpencode,
        project: TempDir,
        child: std::process::Child,
    }

    impl Running {
        fn start() -> Self {
            let fake = FakeOpencode::new();
            let project = TempDir::new();
            let child = fake.spawn(project.path());
            let running = Self { fake, project, child };
            wait_until("fake opencode opens its database", || {
                process::open_files(running.pid()).contains(&running.fake.database)
            });
            running
        }

        fn pid(&self) -> i32 {
            i32::try_from(self.child.id()).expect("pid")
        }

        fn look(&self, since: SystemTime) -> Option<Session> {
            look(self.pid(), since, &mut super::Models::default())
        }
    }

    impl Drop for Running {
        fn drop(&mut self) {
            self.fake.quit();
            let _ = self.child.wait();
        }
    }

    #[test]
    fn the_session_is_the_one_in_the_agents_folder_written_since_it_started() {
        let running = Running::start();
        let elsewhere = TempDir::new();
        running.fake.write("ses_old", running.project.path(), REPLY);
        let since = SystemTime::now() + Duration::from_millis(5);
        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(running.look(since), Some(Session { turn: false, context: None }));

        running.fake.write("ses_elsewhere", elsewhere.path(), WORKING);
        running.fake.write(FakeOpencode::SESSION, running.project.path(), REPLY);

        assert_eq!(
            running.look(since),
            Some(Session { turn: false, context: Some(shown("DeepSeek V4 Pro", Some(12))) })
        );
    }

    #[test]
    fn a_subagents_session_is_not_the_pane() {
        let running = Running::start();
        running.fake.write(FakeOpencode::SESSION, running.project.path(), REPLY);
        running.fake.write("ses_child", running.project.path(), WORKING);
        Connection::open(&running.fake.database)
            .and_then(|db| {
                db.execute("UPDATE session SET parent_id = ?1 WHERE id = 'ses_child'", [FakeOpencode::SESSION])
            })
            .expect("make it a subagent");

        assert_eq!(running.look(SystemTime::UNIX_EPOCH).map(|s| s.turn), Some(false));
    }

    #[test]
    fn two_opencode_in_one_folder_show_nothing() {
        let running = Running::start();
        running.fake.write(FakeOpencode::SESSION, running.project.path(), REPLY);
        let mut other = running.fake.spawn(running.project.path());
        let pid = i32::try_from(other.id()).expect("pid");
        wait_until("the second one opens the same database", || {
            process::open_files(pid).contains(&running.fake.database)
        });

        assert_eq!(running.look(SystemTime::UNIX_EPOCH), None);
        other.kill().expect("stop the second one");
        other.wait().expect("the second one exits");
        assert!(running.look(SystemTime::UNIX_EPOCH).is_some());
    }

    #[test]
    fn a_database_that_cannot_be_queried_gives_no_status() {
        let running = Running::start();
        Connection::open(&running.fake.database)
            .and_then(|db| db.execute_batch("DROP TABLE message; DROP TABLE session;"))
            .expect("break the schema");

        assert_eq!(running.look(SystemTime::UNIX_EPOCH), None);
    }

    #[test]
    fn a_process_without_the_database_open_has_no_session() {
        let project = TempDir::new();
        let sleeper = Sleeper::named(&project.path().join("opencode"), project.path(), &[]);

        assert_eq!(look(sleeper.pid(), SystemTime::UNIX_EPOCH, &mut super::Models::default()), None);
    }

    #[test]
    fn the_pane_follows_the_turn_and_clears_when_opencode_exits() {
        let mut running = Running::start();
        let mut pane = Pane::default();
        pane.update_opencode(running.pid());
        running.fake.write(FakeOpencode::SESSION, running.project.path(), WORKING);
        wait_until("the turn shows", || {
            pane.update_opencode(running.pid());
            pane.opencode_turn() == Some(true)
        });
        assert_eq!(pane.context().cloned(), Some(shown("DeepSeek V4 Pro", None)));

        running.fake.write(FakeOpencode::SESSION, running.project.path(), REPLY);
        wait_until("the reply shows", || {
            pane.update_opencode(running.pid());
            pane.opencode_turn() == Some(false) && pane.context().is_some_and(|c| c.percent == Some(12))
        });
        running.fake.quit();
        running.child.wait().expect("fake opencode exits");
        wait_until("the line clears", || {
            pane.update_opencode(running.pid());
            pane.context().is_none() && pane.opencode_turn().is_none()
        });
    }
}
