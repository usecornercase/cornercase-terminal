use super::*;
use serde_json::json;

use crate::agents::context::counting::Ledger;
use crate::agents::context::message::Role;
use crate::agents::stats::Tokens;
use crate::agents::vibe::fake::{FakeVibe, ID, Usage};

fn parse(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_string()).collect()
}

mod store {
    use super::*;

    #[test]
    fn reads_the_model_and_the_shared_context_percentage() {
        let fake = FakeVibe::new();
        let session = look(fake.path(), ID).expect("the session");

        assert_eq!(session.id, ID);
        assert_eq!(session.context, Some(Context { model: "glm-5-3".into(), percent: Some(0) }));

        fake.turn(
            "completed",
            "mistral-medium-latest",
            Usage { input: 130_300, cached: 100_000, output: 240 },
            millis(SystemTime::now()),
        );
        let session = look(fake.path(), ID).expect("the session");
        assert_eq!(session.context, Some(Context { model: "mistral-medium-latest".into(), percent: Some(16) }));
        assert!(session.resumable);
    }

    #[test]
    fn a_missing_or_broken_store_gives_no_session() {
        let fake = FakeVibe::new();
        std::fs::write(fake.session().join("meta.json"), "{ not json").expect("break the meta");

        assert!(look(fake.path(), ID).is_none());
    }

    #[test]
    fn a_generation_that_leaves_its_folder_is_not_read() {
        let fake = FakeVibe::new();
        let current = json!({"generation": "../generations/0000000000000001", "session_id": ID});
        std::fs::write(fake.session().join("CURRENT"), current.to_string()).expect("write CURRENT");

        let session = look(fake.path(), ID).expect("the session");

        assert_eq!(session.context, Some(Context { model: "glm-5-3".into(), percent: None }));
        assert!(!session.resumable);
    }

    #[test]
    fn hides_the_percentage_after_a_compaction_until_a_newer_turn() {
        let fake = FakeVibe::new();
        fake.turn(
            "completed",
            "glm-5-3",
            Usage { input: 130_300, cached: 100_000, output: 240 },
            millis(SystemTime::now()),
        );
        let before = look(fake.path(), ID).expect("the session");
        assert_eq!(before.context.clone().expect("context").percent, Some(16));

        fake.compactions(1);
        let compacted = look_after(fake.path(), ID, before).expect("the session");
        assert!(compacted.compaction.hides);
        assert_eq!(compacted.context.clone().expect("context"), Context { model: "glm-5-3".into(), percent: None });

        fake.turn(
            "completed",
            "glm-5-3",
            Usage { input: 130_300, cached: 100_000, output: 240 },
            millis(SystemTime::now()) + 5_000,
        );
        let after = look_after(fake.path(), ID, compacted).expect("the session");
        assert!(!after.compaction.hides);
        assert_eq!(after.context.expect("context").percent, Some(16));
    }

    #[test]
    fn an_old_compaction_does_not_hide_the_percentage_when_the_pane_is_first_seen() {
        let fake = FakeVibe::new();
        fake.compactions(2);

        let session = look(fake.path(), ID).expect("the session");
        assert!(!session.compaction.hides);
        assert_eq!(session.compaction.at, None);
    }

    #[test]
    fn counts_prompts_and_the_token_split_once() {
        let fake = FakeVibe::new();
        let first = look(fake.path(), ID).expect("the session");
        let mut ledger = Ledger::default();
        ledger.apply(ID, SystemTime::UNIX_EPOCH, std::mem::take(&mut first.counted.clone()));
        let counted = ledger.take();
        assert_eq!(counted.prompts.len(), 1);
        assert!(counted.tokens.is_empty(), "the first read only sets the baseline");

        fake.turn(
            "completed",
            "glm-5-3",
            Usage { input: 130_300, cached: 100_000, output: 240 },
            millis(SystemTime::now()) + 5_000,
        );
        let second = look_after(fake.path(), ID, first).expect("the session");
        ledger.apply(ID, SystemTime::UNIX_EPOCH, second.counted);
        let counted = ledger.take();
        assert_eq!(counted.prompts.len(), 1);
        assert_eq!(counted.tokens.len(), 1);
        assert_eq!(counted.tokens[0].1, Tokens { input: 29_300, out: 40, cache_read: 100_000, cache_write: 0 });
    }

    #[test]
    fn a_compaction_counts_no_prompt_or_tokens_of_its_own() {
        let fake = FakeVibe::new();
        let first = look(fake.path(), ID).expect("the session");
        let mut ledger = Ledger::default();
        ledger.apply(ID, SystemTime::UNIX_EPOCH, first.counted.clone());
        ledger.take();

        fake.compactions(1);
        fake.turn("completed", "glm-5-3", Usage::default(), millis(SystemTime::now()) + 5_000);
        let compacted = look_after(fake.path(), ID, first).expect("the session");
        ledger.apply(ID, SystemTime::UNIX_EPOCH, compacted.counted);
        let counted = ledger.take();
        assert!(counted.prompts.is_empty(), "a compaction is not a prompt");
        assert!(counted.tokens.is_empty(), "a reset total counts nothing");
    }

    #[test]
    fn reads_the_older_token_threshold_when_model_facts_are_absent() {
        let fake = FakeVibe::new();
        fake.write_runtime(&json!({
            "session_metadata": {"active_model": "glm-5-3"},
            "core_settings": {"context": {"compaction": {"token_threshold": 800_000}}},
        }));

        let session = look(fake.path(), ID).expect("the session");
        assert_eq!(session.context, Some(Context { model: "glm-5-3".into(), percent: Some(0) }));
    }

    #[test]
    fn a_store_without_a_window_shows_the_model_alone() {
        let fake = FakeVibe::new();
        fake.write_runtime(&json!({
            "session_metadata": {"active_model": "glm-5-3"},
            "core_settings": {},
        }));

        let session = look(fake.path(), ID).expect("the session");
        assert_eq!(session.context, Some(Context { model: "glm-5-3".into(), percent: None }));
    }

    #[test]
    fn counts_the_subagents_that_still_run() {
        let fake = FakeVibe::new();
        fake.subagents(&[("explore", "running"), ("review", "reserved"), ("done", "idle"), ("gone", "tombstone")]);

        assert_eq!(look(fake.path(), ID).expect("the session").subagents, 2);
    }
}

mod fixtures {
    use super::*;
    use crate::test_util::TempDir;

    const SUCCESS: &str = "dadb83b5-a40b-ec1e-b1ff-c4c8ef5ce074";
    const COMPACT: &str = "727dea63-ebcc-6824-09d5-4b65f46e1ae4";

    fn store(id: &str, generation: &str, files: &[(&str, &str)], journal: &str) -> TempDir {
        let home = TempDir::new();
        let session = home.path().join("logs/session/unified").join(id);
        let generation = session.join("generations").join(generation);
        std::fs::create_dir_all(session.join("journal")).expect("create the journal folder");
        std::fs::create_dir_all(&generation).expect("create the generation folder");
        for (name, text) in files {
            let path =
                if *name == "CURRENT" || *name == "meta.json" { session.join(name) } else { generation.join(name) };
            std::fs::write(path, text).expect("write a fixture file");
        }
        std::fs::write(session.join("journal/0000000000000001.jsonl"), journal).expect("write the journal");
        home
    }

    #[test]
    fn the_3_0_3_success_fixture_reads_the_model_the_window_the_last_message_and_the_prompt() {
        let home = store(
            SUCCESS,
            "0000000000000008",
            &[
                ("CURRENT", crate::test_util::fixture!("vibe/3.0.3/success/CURRENT")),
                ("meta.json", crate::test_util::fixture!("vibe/3.0.3/success/meta.json")),
                ("projection-state.json", crate::test_util::fixture!("vibe/3.0.3/success/projection-state.json")),
                ("runtime-state.json", crate::test_util::fixture!("vibe/3.0.3/success/runtime-state.json")),
                ("checkpoint.json", crate::test_util::fixture!("vibe/3.0.3/success/checkpoint.json")),
            ],
            crate::test_util::fixture!("vibe/3.0.3/success/journal.jsonl"),
        );

        let session = look(home.path(), SUCCESS).expect("the fixture session");
        assert_eq!(session.context, Some(Context { model: "probe".into(), percent: Some(1) }));
        assert!(session.resumable);
        let message = crate::agents::vibe::message::last_message(&session.path).expect("read").expect("a reply");
        assert_eq!(message.text, "done");
        let mut said = Vec::new();
        crate::agents::vibe::message::said_all(&session.path, &mut |spoken| {
            said.push((spoken.role, spoken.said.text));
            true
        })
        .expect("search");
        assert_eq!(said, [(Role::Prompt, "Reply with the word done".to_string()), (Role::Reply, "done".to_string())]);
    }

    #[test]
    fn the_3_0_3_compaction_fixture_reads_the_reset_context() {
        let home = store(
            COMPACT,
            "0000000000000017",
            &[
                ("CURRENT", crate::test_util::fixture!("vibe/3.0.3/compact/CURRENT")),
                ("meta.json", crate::test_util::fixture!("vibe/3.0.3/compact/meta.json")),
                ("projection-state.json", crate::test_util::fixture!("vibe/3.0.3/compact/projection-state.json")),
                ("runtime-state.json", crate::test_util::fixture!("vibe/3.0.3/compact/runtime-state.json")),
                ("checkpoint.json", crate::test_util::fixture!("vibe/3.0.3/compact/checkpoint.json")),
            ],
            crate::test_util::fixture!("vibe/3.0.3/compact/journal.jsonl"),
        );

        let session = look(home.path(), COMPACT).expect("the fixture session");
        assert_eq!(session.context, Some(Context { model: "probe".into(), percent: Some(0) }));
        assert!(!session.compaction.hides);
    }
}

mod leases {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_lease_names_the_session_of_the_process() {
        let fake = FakeVibe::new();
        fake.lease(41);

        assert_eq!(lease(fake.path(), 41).as_deref(), Some(ID));
        assert_eq!(lease(fake.path(), 42), None);
    }

    #[test]
    fn two_leases_of_one_process_name_nothing() {
        let fake = FakeVibe::new();
        fake.lease(41);
        let active = fake.path().join("logs/session/active");
        std::fs::write(active.join("other.lock.json"), json!({"process_id": 41, "session_id": "other"}).to_string())
            .expect("write the second lease");

        assert_eq!(lease(fake.path(), 41), None);
    }

    #[test]
    fn resumes_only_with_an_id_that_could_name_a_session() {
        assert_eq!(resumed(&parse(&["--resume", "8c7d-4a3b"])).as_deref(), Some("8c7d-4a3b"));
        assert_eq!(resumed(&parse(&["--resume=8c7d-4a3b"])).as_deref(), Some("8c7d-4a3b"));
        assert_eq!(resumed(&parse(&["--resume"])), None);
        assert_eq!(resumed(&parse(&["--resume", "../escape"])), None);
    }
}
