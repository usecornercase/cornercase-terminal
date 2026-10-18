use serde_json::json;

use super::super::usage_modal::{ANSWER, CODEX_ANSWER, with_agents};
use super::*;
use crate::control::Plans;
use crate::control::usage::PlanInfo;

const CLAUDE: &str = "claude";
const CODEX: &str = "codex";

fn script(body: &str) -> String {
    format!("echo run >> \"$0.runs\"\n{body}")
}

fn claude_script() -> String {
    script(&format!("read -r a\nread -r b\necho '{ANSWER}'"))
}

fn codex_script() -> String {
    script(&format!("read -r a\nread -r b\nread -r c\necho '{CODEX_ANSWER}'"))
}

fn runs(dir: &TempDir, agent: &str) -> usize {
    std::fs::read_to_string(dir.path().join(format!("{agent}.runs"))).map_or(0, |text| text.lines().count())
}

fn plans(value: Value) -> Plans {
    serde_json::from_value(value).expect("plans")
}

fn plan<'a>(plans: &'a Plans, agent: &str) -> &'a PlanInfo {
    plans.agents.iter().find(|p| p.agent == agent).expect("an agent")
}

fn fresh(app: &mut App, rx: &Receiver<AppEvent>) -> Plans {
    plans(asked(app, rx, "usage", &json!({"cached": false})).expect("an answer"))
}

#[test]
fn a_request_asks_each_installed_agent_and_answers_what_the_limits_tab_shows() {
    let (claude, codex) = (claude_script(), codex_script());
    let (mut app, rx, _dir) = with_agents(&[(CLAUDE, &claude), (CODEX, &codex)]);
    let answer = fresh(&mut app, &rx);
    assert_eq!(answer.agents.len(), 2);
    let claude = plan(&answer, CLAUDE);
    assert_eq!((claude.title.as_str(), claude.plan.as_deref()), ("Claude Code", Some("max")));
    assert_eq!((claude.limits[0].kind.as_str(), claude.limits[0].percent), ("session", 42));
    let codex = plan(&answer, CODEX);
    assert_eq!(codex.plan.as_deref(), Some("plus"));
    assert_eq!((codex.limits[0].kind.as_str(), codex.limits[0].percent), ("primary", 17));
    for p in &answer.agents {
        assert!(p.read_at.is_some() && p.age == Some(0), "{p:?}");
        assert!(p.error.is_none() && !p.loading, "{p:?}");
    }
}

#[test]
fn a_cached_request_answers_at_once_without_asking() {
    let claude = claude_script();
    let (mut app, rx, dir) = with_agents(&[(CLAUDE, &claude)]);
    let before = plans(ok(&mut app, "usage", &json!({"cached": true})));
    assert!(plan(&before, CLAUDE).read_at.is_none());
    assert_eq!(plan(&before, CLAUDE).limits.len(), 0);
    assert_eq!(runs(&dir, CLAUDE), 0);
    fresh(&mut app, &rx);
    assert_eq!(runs(&dir, CLAUDE), 1);
    let after = plans(ok(&mut app, "usage", &json!({"cached": true})));
    assert_eq!(plan(&after, CLAUDE).plan.as_deref(), Some("max"));
    assert_eq!(runs(&dir, CLAUDE), 1);
}

#[test]
fn a_running_probe_is_waited_for_not_started_again() {
    let claude = claude_script();
    let (mut app, rx, dir) = with_agents(&[(CLAUDE, &claude)]);
    app.switch_usage(crate::agents::usage::Page::Limits);
    let answer = fresh(&mut app, &rx);
    assert_eq!(plan(&answer, CLAUDE).plan.as_deref(), Some("max"));
    assert_eq!(runs(&dir, CLAUDE), 1);
}

#[test]
fn a_failed_probe_answers_its_error_and_the_reading_before_it() {
    let claude = claude_script();
    let (mut app, rx, dir) = with_agents(&[(CLAUDE, &claude)]);
    fresh(&mut app, &rx);
    write_executable(&dir.path().join(CLAUDE), "#!/bin/sh\nexec sleep 30\n");
    app.usage_timeout = Duration::from_millis(200);
    let answer = fresh(&mut app, &rx);
    let claude = plan(&answer, CLAUDE);
    assert_eq!(claude.error.as_deref(), Some("claude did not answer in time"));
    assert_eq!(claude.plan.as_deref(), Some("max"));
    assert_eq!(claude.limits.len(), 1);
}

#[test]
fn without_any_agent_installed_each_one_says_why() {
    let (mut app, rx, _dir) = with_agents(&[]);
    let answer = fresh(&mut app, &rx);
    assert_eq!(answer.agents.len(), 2);
    for (p, agent) in answer.agents.iter().zip([CLAUDE, CODEX]) {
        let error = p.error.as_deref().unwrap_or_default();
        assert!(error.starts_with(&format!("could not run /nonexistent/{agent}")), "{error}");
    }
}

#[test]
fn an_answer_does_not_wait_for_a_probe_started_after_it() {
    let claude =
        script(&format!("[ -e \"$0.ran\" ] && exec sleep 30\ntouch \"$0.ran\"\nread -r a\nread -r b\necho '{ANSWER}'"));
    let codex = script(&format!(
        "while [ ! -e \"$(dirname \"$0\")/go\" ]; do sleep 0.05; done\nread -r a\nread -r b\nread -r c\necho '{CODEX_ANSWER}'"
    ));
    let (mut app, rx, dir) = with_agents(&[(CLAUDE, &claude), (CODEX, &codex)]);
    app.usage_timeout = Duration::from_secs(3);
    let text = json!({"contract": 1, "command": "usage", "args": {}}).to_string();
    app.request(CLIENT, &text, Some(AREA), Instant::now());
    pump_until(&mut app, &rx, "claude answers", |a| a.usage.probe(crate::agents::Known::Claude).answers() == 1);
    app.request(10, &text, Some(AREA), Instant::now());
    assert!(app.usage.probe(crate::agents::Known::Claude).loading());
    std::fs::write(dir.path().join("go"), "").expect("go");
    let mut got = Vec::new();
    wait_until("the first request is answered", || {
        drain(&mut app, &rx);
        app.refresh(Instant::now());
        got.extend(app.take_answers());
        got.iter().any(|(client, _)| *client == CLIENT)
    });
    assert!(app.usage.probe(crate::agents::Known::Claude).loading());
    assert!(got.iter().all(|(client, _)| *client == CLIENT));
}

#[test]
fn a_client_that_leaves_drops_its_request_but_the_reading_is_kept() {
    let claude = claude_script();
    let (mut app, rx, _dir) = with_agents(&[(CLAUDE, &claude)]);
    let text = json!({"contract": 1, "command": "usage", "args": {}}).to_string();
    app.request(CLIENT, &text, Some(AREA), Instant::now());
    app.forget(CLIENT);
    pump_until(&mut app, &rx, "the probe answers", |a| a.usage.probe(crate::agents::Known::Claude).answers() == 1);
    app.refresh(Instant::now());
    assert_eq!(app.take_answers().len(), 0);
    let cached = plans(ok(&mut app, "usage", &json!({"cached": true})));
    assert_eq!(plan(&cached, CLAUDE).plan.as_deref(), Some("max"));
}
