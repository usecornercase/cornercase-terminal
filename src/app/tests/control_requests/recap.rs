#![cfg(unix)]

use std::time::Duration;

use super::*;
use crate::app::tests::recap_panel::{Recaps, cached_now, recaps, waiting_place};
use crate::recap::Choice;

const PROMPTED: Duration = Duration::from_secs(600);

fn the_new_keys_are_empty(first: &Value) {
    assert_eq!(first["outdated"], false);
    assert_eq!(first["changed"], json!([]));
    assert!(first["places"]["c1"]["status"].is_null(), "{first}");
    assert!(first["places"]["c1"]["checks"].is_null(), "{first}");
}

#[test]
fn a_recap_is_written_for_the_first_ask_and_answered_from_the_cache_after() {
    let mut r = recaps(PROMPTED);
    let first = asked(&mut r.app, &r.rx, "recap", &json!({"range": "now"})).expect("a recap");
    assert_eq!(first["text"], "hello");
    assert_eq!(first["agent"], "claude");
    assert_eq!(first["title"], "Claude Code");
    assert_eq!(first["conversations"], 1);
    assert_eq!(first["loading"], false);
    assert!(first["places"]["c1"]["project"]["label"].is_string(), "{first}");
    assert_eq!(first["tokens"]["output"], 73);
    the_new_keys_are_empty(&first);
    let again = asked(&mut r.app, &r.rx, "recap", &json!({"range": "now"})).expect("a recap");
    assert_eq!(again["text"], "hello");
    assert_eq!(r.runs(), 1);
}

#[test]
fn fresh_and_an_override_write_it_again_and_reach_the_agent() {
    let mut r = recaps(PROMPTED);
    asked(&mut r.app, &r.rx, "recap", &json!({"range": "now"})).expect("a recap");
    asked(&mut r.app, &r.rx, "recap", &json!({"range": "now", "fresh": true})).expect("a recap");
    assert_eq!(r.runs(), 2);
    asked(&mut r.app, &r.rx, "recap", &json!({"range": "now", "model": "sonnet", "language": "Spanish"}))
        .expect("a recap");
    assert_eq!(r.runs(), 3);
    let argv = std::fs::read_to_string(r.claude.with_extension("argv")).expect("argv");
    let last = argv.lines().last().expect("a run");
    assert!(last.contains("\"--model\", \"sonnet\""), "{last}");
    assert!(last.contains("Write in Spanish."), "{last}");
}

#[test]
fn the_dossier_is_printed_without_running_an_agent() {
    let mut r = recaps(PROMPTED);
    let answer = asked(&mut r.app, &r.rx, "recap", &json!({"range": "now", "dossier": true})).expect("a dossier");
    assert!(answer["dossier"].as_str().is_some_and(|text| text.contains("add a rate limit")), "{answer}");
    assert!(answer["text"].is_null() && answer["agent"].is_null() && answer["tokens"].is_null());
    assert_eq!(r.runs(), 0);
}

#[test]
fn a_range_nobody_knows_is_malformed() {
    let mut r = recaps(PROMPTED);
    assert_eq!(refused(&mut r.app, "recap", &json!({"range": "tomorrow"})).code, "malformed");
}

#[test]
fn nothing_to_sum_up_says_so_and_runs_no_agent() {
    let mut r = recaps(PROMPTED);
    let answer = asked(&mut r.app, &r.rx, "recap", &json!({"range": "yesterday"})).expect("an answer");
    assert_eq!(answer["empty"], "No agent activity in the last 30 days.");
    assert!(answer["text"].is_null());
    assert_eq!(r.runs(), 0);
}

#[test]
fn another_set_of_options_while_one_runs_is_busy() {
    let mut r = recaps(PROMPTED);
    std::fs::write(r.claude.with_extension("hold"), "").expect("hold");
    r.app.toggle_recap();
    let busy = refused(&mut r.app, "recap", &json!({"range": "now", "model": "sonnet"}));
    assert_eq!(busy.code, "busy");
    std::fs::remove_file(r.claude.with_extension("hold")).expect("release");
}

#[test]
fn a_failed_run_answers_with_its_reason() {
    let mut r = recaps(PROMPTED);
    let failed = crate::test_util::fixture!("claude/2.1.318/recap-failed.jsonl");
    std::fs::write(r.claude.with_extension("out"), failed).expect("a failing agent");
    let answer = asked(&mut r.app, &r.rx, "recap", &json!({"range": "now"})).expect("an answer");
    assert_eq!(answer["error"], "Not logged in · Please run /login");
}

#[test]
fn a_run_tells_its_start_and_its_end_to_the_ones_following() {
    let mut r = recaps(PROMPTED);
    follow(&mut r.app, &[]);
    std::fs::write(r.claude.with_extension("hold"), "").expect("hold");
    r.app.toggle_recap();
    let Recaps { app, rx, .. } = &mut r;
    refreshing(app, rx, "the agent starts", |a| a.panels.recap.run(Choice::Now).announced);
    std::fs::remove_file(r.claude.with_extension("hold")).expect("release");
    let Recaps { app, rx, .. } = &mut r;
    refreshing(app, rx, "the run ends", |a| !a.panels.recap.run(Choice::Now).running);

    let states: Vec<String> = told(&streamed(&mut r.app))
        .into_iter()
        .filter_map(|(what, _)| match what {
            wire::What::Recap { state, range, agent, error } => {
                assert_eq!((range.as_str(), agent.as_str(), error), ("now", "claude", None));
                Some(state)
            }
            _ => None,
        })
        .collect();
    assert_eq!(states, ["started", "finished"]);
}

#[test]
fn a_cached_now_that_names_a_closed_conversation_is_asked_again_and_says_what_changed_when_that_fails() {
    let mut r = recaps(PROMPTED);
    let cached = cached_now([("c1".to_string(), waiting_place(Some("waiting")))].into());
    r.app.panels.recap.run_mut(Choice::Now).last = Some(cached.clone());
    let failed = crate::test_util::fixture!("claude/2.1.318/recap-failed.jsonl");
    std::fs::write(r.claude.with_extension("out"), failed).expect("a failing agent");
    let answer = asked(&mut r.app, &r.rx, "recap", &json!({"range": "now"})).expect("an answer");
    assert_eq!(r.runs(), 1);
    assert_eq!(answer["error"], "Not logged in · Please run /login");
    assert_eq!(answer["outdated"], true);
    assert_eq!(answer["changed"], json!(["c1"]));
    assert_eq!(answer["text"], cached.text);
}
