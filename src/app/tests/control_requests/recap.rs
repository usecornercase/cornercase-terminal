#![cfg(unix)]

use std::time::Duration;

use super::*;
use crate::app::tests::recap_panel::recaps;

const PROMPTED: Duration = Duration::from_secs(600);

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
