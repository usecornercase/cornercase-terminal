use std::sync::atomic::AtomicBool;

use super::*;
use crate::test_util::TempDir;

fn place(pane: u64) -> Place {
    Place { project: 1, workspace: 2, tab: 3, pane, label: format!("api › main › tab{pane}"), who: "zsh".into() }
}

fn screen(pane: u64, text: &str) -> Source {
    Source { place: place(pane), screen: text.into(), record: None }
}

fn run(sources: &[Source], query: &str) -> Found {
    search(sources, query, &AtomicBool::new(false))
}

fn claude(dir: &TempDir, lines: &[&str]) -> Record {
    let path = dir.path().join("session.jsonl");
    std::fs::write(&path, lines.join("\n") + "\n").expect("write the transcript");
    Record::Claude(path)
}

fn human(text: &str) -> String {
    serde_json::json!({
        "type": "user",
        "origin": { "kind": "human" },
        "timestamp": "2026-10-10T12:00:00.000Z",
        "message": { "content": text },
    })
    .to_string()
}

fn reply(id: &str, text: &str) -> String {
    serde_json::json!({
        "type": "assistant",
        "timestamp": "2026-10-10T12:00:05.000Z",
        "message": { "id": id, "model": "claude-opus-5-5", "content": [{ "type": "text", "text": text }] },
    })
    .to_string()
}

mod matching {
    use super::*;

    #[test]
    fn ignores_case_and_folds_non_ascii_letters() {
        let found = run(&[screen(1, "Migration FAILED\nsalió ÁRBOL")], "migration failed");
        assert_eq!(found.matches.len(), 1);
        assert_eq!(run(&[screen(1, "salió ÁRBOL")], "árbol").matches.len(), 1);
    }

    #[test]
    fn an_empty_query_finds_nothing() {
        assert_eq!(run(&[screen(1, "text")], "  "), Found::default());
    }

    #[test]
    fn a_screen_match_keeps_its_line_and_where_it_hit() {
        let found = run(&[screen(1, "one\ntwo needle here\nthree")], "needle");
        let hit = &found.matches[0];
        assert_eq!((hit.origin, hit.line), (Origin::Screen, Some(1)));
        assert_eq!(&hit.snippet[hit.hit.clone()], "needle");
    }

    #[test]
    fn the_snippet_shifts_to_show_a_match_far_along_the_line() {
        let line = format!("{}needle{}", "a".repeat(300), "b".repeat(300));
        let found = run(&[screen(1, &line)], "needle");
        let hit = &found.matches[0];
        assert!(hit.snippet.starts_with('…') && hit.snippet.ends_with('…'));
        assert!(hit.snippet.chars().count() <= WIDTH + 2);
        assert_eq!(hit.snippet.chars().skip(hit.hit.start).take(hit.hit.len()).collect::<String>(), "needle");
    }

    #[test]
    fn control_characters_become_spaces_and_the_indent_goes() {
        let (shown, hit) = snippet("   \tneedle\u{1b}[0m", &(4..10));
        assert_eq!(shown, "needle [0m");
        assert_eq!(hit, 0..6);
    }
}

mod order {
    use super::*;

    #[test]
    fn panes_come_in_the_order_given_and_a_screen_from_the_bottom_up() {
        let found = run(&[screen(2, "hit a\nhit b"), screen(1, "hit c")], "hit");
        let seen: Vec<(u64, Option<usize>)> = found.matches.iter().map(|m| (m.place.pane, m.line)).collect();
        assert_eq!(seen, [(2, Some(1)), (2, Some(0)), (1, Some(0))]);
    }

    #[test]
    fn the_transcript_comes_first_newest_first() {
        let dir = TempDir::new();
        let record = claude(&dir, &[&human("fix the needle bug"), &reply("m1", "found the needle in src/lib.rs")]);
        let source = Source { place: place(1), screen: "a needle on screen".into(), record: Some(record) };
        let found = run(&[source], "needle");
        let origins: Vec<Origin> = found.matches.iter().map(|m| m.origin).collect();
        assert_eq!(origins, [Origin::Reply, Origin::Prompt, Origin::Screen]);
        assert_eq!(found.matches[0].written.as_deref(), Some("2026-10-10T12:00:05.000Z"));
    }

    #[test]
    fn a_screen_line_the_transcript_already_holds_is_left_out() {
        let dir = TempDir::new();
        let record = claude(&dir, &[&reply("m1", "I fixed the needle bug.\nAnother line.")]);
        let screen = "● I fixed the   needle bug.\nother needle";
        let found = run(&[Source { place: place(1), screen: screen.into(), record: Some(record) }], "needle");
        let seen: Vec<(Origin, &str)> = found.matches.iter().map(|m| (m.origin, m.snippet.as_str())).collect();
        assert_eq!(seen, [(Origin::Reply, "I fixed the needle bug."), (Origin::Screen, "other needle")]);
    }
}

mod limits {
    use super::*;

    #[test]
    fn a_pane_keeps_its_first_matches_and_says_it_left_some_out() {
        let text = "hit\n".repeat(MAX_PER_PANE + 1);
        let found = run(&[screen(1, &text)], "hit");
        assert_eq!((found.matches.len(), found.capped), (MAX_PER_PANE, true));
    }

    #[test]
    fn a_pane_with_exactly_the_cap_is_not_capped() {
        let found = run(&[screen(1, &"hit\n".repeat(MAX_PER_PANE))], "hit");
        assert_eq!((found.matches.len(), found.capped), (MAX_PER_PANE, false));
    }

    #[test]
    fn the_total_is_capped_too() {
        let sources: Vec<Source> = (0..40).map(|pane| screen(pane, &"hit\n".repeat(MAX_PER_PANE))).collect();
        let found = run(&sources, "hit");
        assert_eq!((found.matches.len(), found.capped), (MAX_MATCHES, true));
    }

    #[test]
    fn a_cancelled_search_stops_early() {
        let sources: Vec<Source> = (0..40).map(|pane| screen(pane, "hit")).collect();
        let found = search(&sources, "hit", &AtomicBool::new(true));
        assert_eq!(found, Found::default());
    }
}

mod state {
    use std::sync::atomic::Ordering;
    use std::time::Instant;

    use super::*;

    #[test]
    fn a_newer_request_cancels_the_older_job_and_its_answer_is_dropped() {
        let (mut sessions, now) = (Sessions::default(), Instant::now());
        let (first, cancel) = sessions.request("a", now).expect("a job");
        let (second, _) = sessions.request("ab", now).expect("another job");
        assert!(cancel.load(Ordering::Relaxed));
        assert!(!sessions.answered(first, "a", Found::default()));
        assert!(sessions.answered(second, "ab", Found::default()));
        assert!(sessions.answer("ab").is_some() && sessions.answer("a").is_none());
    }

    #[test]
    fn a_query_with_an_answer_or_a_job_asks_nothing() {
        let (mut sessions, now) = (Sessions::default(), Instant::now());
        let (generation, _) = sessions.request("a", now).expect("a job");
        assert!(sessions.request("a", now).is_none());
        sessions.answered(generation, "a", Found::default());
        assert!(sessions.request(" a ", now).is_none());
    }

    #[test]
    fn a_job_that_never_answers_is_given_up_on() {
        let (mut sessions, now) = (Sessions::default(), Instant::now());
        sessions.request("a", now);
        assert!(sessions.request("a", now + GIVE_UP_AFTER).is_some());
    }

    #[test]
    fn a_screen_is_read_again_only_when_the_pane_changed() {
        let (mut sessions, at) = (Sessions::default(), Instant::now());
        let reads = std::cell::Cell::new(0);
        let mut read = |text: &'static str, at, size| {
            sessions.screen(1, at, size, || {
                reads.set(reads.get() + 1);
                text.into()
            })
        };
        assert_eq!(&*read("one", at, (24, 80)), "one");
        assert_eq!(&*read("two", at, (24, 80)), "one");
        assert_eq!(&*read("two", at, (30, 80)), "two");
        assert_eq!(&*read("three", at + Duration::from_secs(1), (30, 80)), "three");
        assert_eq!(reads.get(), 3);
    }

    #[test]
    fn forgetting_drops_everything() {
        let (mut sessions, now) = (Sessions::default(), Instant::now());
        sessions.request("a", now);
        sessions.built(now, vec![screen(1, "x")]);
        sessions.screen(1, now, (1, 1), || "x".into());
        assert!(!sessions.idle());
        sessions.forget();
        assert!(sessions.idle());
    }

    #[test]
    fn the_corpus_is_rebuilt_only_after_its_time() {
        let (mut sessions, now) = (Sessions::default(), Instant::now());
        assert!(sessions.corpus(now).is_none());
        sessions.built(now, vec![screen(1, "x")]);
        assert!(sessions.corpus(now + Duration::from_secs(1)).is_some());
        assert!(sessions.corpus(now + CORPUS_FOR).is_none());
    }
}
