use rstest::rstest;

use super::*;
use crate::agents::Known;
use crate::agents::activity::Activity;
use crate::agents::context::tests::pane_look;
use crate::agents::context::{Pane, Role};
use crate::agents::opencode::fake::FakeOpencode;
use crate::test_util::{Sleeper, TempDir, wait_until};

const REPLY: &str = crate::test_util::fixture!("opencode/1.18.35/reply.jsonl");
const WORKING: &str = crate::test_util::fixture!("opencode/1.18.35/working.jsonl");
const ABORTED: &str = crate::test_util::fixture!("opencode/1.18.35/aborted.jsonl");
const COMPACTION: &str = crate::test_util::fixture!("opencode/1.18.35/compaction.jsonl");
const MODELS: &str = crate::test_util::fixture!("opencode/1.18.35/models.json");
const REPLY_PARTS: &str = crate::test_util::fixture!("opencode/1.18.35/reply-parts.jsonl");
const TOOL_PARTS: &str = crate::test_util::fixture!("opencode/1.18.35/tool-parts.jsonl");
const FLASH: &str =
    r#"{"role":"user","time":{"created":1},"model":{"providerID":"deepseek","modelID":"deepseek-v4-flash"}}"#;
const QUEUED: &str =
    r#"{"role":"user","time":{"created":1791469765000},"model":{"providerID":"deepseek","modelID":"deepseek-v4-pro"}}"#;

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
    let session = Models::new().read(messages);

    assert_eq!((session.turn, session.context), (turn, context));
}

#[rstest]
#[case::a_reply(&[REPLY], Some(1_791_469_400_000))]
#[case::a_message_after_it(&[REPLY, WORKING], Some(1_791_469_762_156))]
#[case::a_prompt_queued_while_it_works(&[WORKING, QUEUED], Some(1_791_469_765_000))]
#[case::nothing_yet(&[], None)]
fn the_newest_message_of_the_user_says_when_a_prompt_was_last_sent(
    #[case] messages: &[&str],
    #[case] millis: Option<u64>,
) {
    let prompted = millis.map(|ms| UNIX_EPOCH + Duration::from_millis(ms));

    assert_eq!(Models::new().read(messages).prompted, prompted);
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
fn the_catalogue_lists_every_model_as_provider_and_id() {
    let models = Models::new();
    let ids = ids_in(&models.dir.path().join("models.json"));
    assert_eq!(ids, ["deepseek/deepseek-v4-flash", "deepseek/deepseek-v4-pro"]);
}

#[test]
fn a_missing_catalogue_lists_nothing() {
    assert_eq!(ids_in(Path::new("/nonexistent/models.json")), Vec::<String>::new());
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
    opencode: Sleeper,
    fake: FakeOpencode,
    project: TempDir,
}

impl Running {
    fn start() -> Self {
        let fake = FakeOpencode::new();
        let project = TempDir::new();
        Self { opencode: fake.start(project.path()), fake, project }
    }

    fn pid(&self) -> i32 {
        self.opencode.pid()
    }

    fn look(&self, since: SystemTime) -> Option<Session> {
        look(self.pid(), since, &mut super::Models::default())
    }
}

#[test]
fn the_session_is_the_one_in_the_agents_folder_written_since_it_started() {
    let running = Running::start();
    let elsewhere = TempDir::new();
    running.fake.write("ses_old", running.project.path(), REPLY);
    let since = SystemTime::now() + Duration::from_millis(5);
    std::thread::sleep(Duration::from_millis(10));
    assert_eq!(running.look(since), Some(Session::default()));

    running.fake.write("ses_elsewhere", elsewhere.path(), WORKING);
    running.fake.write(FakeOpencode::SESSION, running.project.path(), REPLY);

    let place = Place { database: running.fake.database.clone(), id: FakeOpencode::SESSION.into() };
    assert_eq!(
        running.look(since).map(|session| (session.turn, session.context, session.place)),
        Some((false, Some(shown("DeepSeek V4 Pro", Some(12))), Some(place)))
    );
}

#[test]
fn a_subagents_session_is_not_the_pane() {
    let running = Running::start();
    running.fake.write(FakeOpencode::SESSION, running.project.path(), REPLY);
    running.fake.write("ses_child", running.project.path(), WORKING);
    Connection::open(&running.fake.database)
        .and_then(|db| db.execute("UPDATE session SET parent_id = ?1 WHERE id = 'ses_child'", [FakeOpencode::SESSION]))
        .expect("make it a subagent");

    assert_eq!(running.look(SystemTime::UNIX_EPOCH).map(|s| s.turn), Some(false));
}

#[test]
fn two_opencode_in_one_folder_show_nothing() {
    let running = Running::start();
    running.fake.write(FakeOpencode::SESSION, running.project.path(), REPLY);
    let other = running.fake.start(running.project.path());

    assert_eq!(running.look(SystemTime::UNIX_EPOCH), None);
    drop(other);
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
    pane.update(Some(Known::Opencode), &pane_look(running.pid()));
    running.fake.write(FakeOpencode::SESSION, running.project.path(), WORKING);
    wait_until("the turn shows", || {
        pane.update(Some(Known::Opencode), &pane_look(running.pid()));
        pane.activity("") == Some(Activity::Working)
    });
    assert_eq!(pane.context().cloned(), Some(shown("DeepSeek V4 Pro", None)));

    running.fake.write(FakeOpencode::SESSION, running.project.path(), REPLY);
    wait_until("the reply shows", || {
        pane.update(Some(Known::Opencode), &pane_look(running.pid()));
        pane.activity("") == Some(Activity::Idle) && pane.context().is_some_and(|c| c.percent == Some(12))
    });
    running.opencode.stop();
    wait_until("the line clears", || {
        pane.update(Some(Known::Opencode), &pane_look(running.pid()));
        pane.context().is_none() && pane.activity("").is_none()
    });
}

mod last_message {
    use super::*;

    struct Chat {
        fake: FakeOpencode,
        project: TempDir,
    }

    impl Chat {
        fn new() -> Self {
            Self { fake: FakeOpencode::new(), project: TempDir::new() }
        }

        fn messages(&self, messages: &str) {
            self.fake.write(FakeOpencode::SESSION, self.project.path(), messages);
        }

        fn parts(&self, message: &str, parts: &str) {
            self.fake.parts(FakeOpencode::SESSION, message, parts);
        }

        fn last(&self) -> error::Result<Option<Said>> {
            last_message(&self.fake.database, FakeOpencode::SESSION)
        }
    }

    #[test]
    fn is_the_text_of_the_newest_reply_with_any() {
        let said = Chat::new();
        said.messages(REPLY);
        said.parts("msg_000000", r#"{"type":"text","text":"Fix the login form"}"#);
        said.parts("msg_000001", REPLY_PARTS);
        said.messages(WORKING);
        said.parts("msg_000003", TOOL_PARTS);

        let found = said.last().expect("read the database");

        assert_eq!(
            found,
            Some(Said {
                text: "The login form now checks the password.\n\nTests pass.".into(),
                written: Some("2026-10-08T14:24:47.195Z".into())
            })
        );
    }

    #[test]
    fn leaves_out_text_written_for_the_model_alone() {
        let said = Chat::new();
        said.messages(REPLY);
        said.parts("msg_000001", r#"{"type":"text","text":"Continue if you have next steps.","synthetic":true}"#);

        assert_eq!(said.last().expect("read the database"), None);
    }

    #[test]
    fn a_compaction_summary_is_for_the_model_not_a_message() {
        let said = Chat::new();
        said.messages(REPLY);
        said.parts("msg_000001", REPLY_PARTS);
        said.messages(COMPACTION);
        said.parts("msg_000003", r#"{"type":"text","text":"Goal: fix the login form."}"#);

        let found = said.last().expect("read the database").map(|said| said.text);

        assert_eq!(found.as_deref(), Some("The login form now checks the password.\n\nTests pass."));
    }

    #[test]
    fn a_session_without_a_reply_has_none() {
        let said = Chat::new();
        said.messages(WORKING);
        said.parts("msg_000000", r#"{"type":"text","text":"Fix the login form"}"#);

        assert_eq!(said.last().expect("read the database"), None);
    }

    fn heard(said: &Chat) -> Vec<(Role, String)> {
        let mut heard = Vec::new();
        said_all(&said.fake.database, FakeOpencode::SESSION, &mut |spoken| {
            heard.push((spoken.role, spoken.said.text));
            true
        })
        .expect("read the database");
        heard
    }

    #[test]
    fn every_message_comes_newest_first_with_its_role() {
        let said = Chat::new();
        said.messages(REPLY);
        said.parts("msg_000000", r#"{"type":"text","text":"Fix the login form"}"#);
        said.parts("msg_000001", REPLY_PARTS);

        assert_eq!(
            heard(&said),
            [
                (Role::Reply, "The login form now checks the password.\n\nTests pass.".into()),
                (Role::Prompt, "Fix the login form".into())
            ]
        );
    }

    #[test]
    fn leaves_out_synthetic_parts_and_compaction_summaries() {
        let said = Chat::new();
        said.messages(REPLY);
        said.parts("msg_000001", r#"{"type":"text","text":"Continue if you have next steps.","synthetic":true}"#);
        said.messages(COMPACTION);
        said.parts("msg_000003", r#"{"type":"text","text":"Goal: fix the login form."}"#);

        assert_eq!(heard(&said), []);
    }

    #[test]
    fn a_database_that_cannot_be_queried_says_which() {
        let said = Chat::new();
        Connection::open(&said.fake.database).and_then(|db| db.execute_batch("DROP TABLE part;")).expect("drop parts");
        said.messages(REPLY);

        let error = said.last().expect_err("no parts to read");

        assert!(error.to_string().contains(&said.fake.database.display().to_string()), "{error}");
    }
}

mod counting_records {
    use super::*;
    use crate::agents::context::counting::{
        Ledger,
        tests::{since, sum},
    };

    #[test]
    fn only_completed_messages_count_and_each_id_counts_once() {
        let records: Vec<_> =
            crate::test_util::fixture!("opencode/1.18.35/usage.jsonl").lines().filter_map(counted).collect();
        let mut ledger = Ledger::default();
        ledger.apply("a", since(), records.clone());
        assert_eq!(
            sum(ledger.take()),
            (1, crate::agents::stats::Tokens { input: 20, out: 65, cache_read: 200, cache_write: 10 })
        );
        ledger.apply("a", since(), records);
        assert_eq!(sum(ledger.take()).1.out, 0);
    }
}
