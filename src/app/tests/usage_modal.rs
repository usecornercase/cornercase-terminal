use super::*;
use crate::test_util::write_executable;

const ANSWER: &str = r#"{"type":"control_response","response":{"subtype":"success","request_id":"usage","response":{"subscription_type":"max","rate_limits_available":true,"rate_limits":{"limits":[{"kind":"session","percent":42,"severity":"normal","resets_at":null}]}}}}"#;
const CODEX_ANSWER: &str = r#"{"id":2,"result":{"rateLimits":{"limitId":"codex","planType":"plus","primary":{"usedPercent":17,"windowDurationMins":300,"resetsAt":null}}}}"#;

fn with_agents(scripts: &[(&str, &str)]) -> (App, Receiver<AppEvent>, TempDir) {
    let dir = TempDir::new();
    let (mut app, rx) = empty_app();
    for kind in [agents::CLAUDE, agents::CODEX] {
        let command = match scripts.iter().find(|(k, _)| *k == kind) {
            Some((_, script)) => {
                let path = dir.path().join(kind);
                write_executable(&path, &format!("#!/bin/sh\n{script}\n"));
                path.display().to_string()
            }
            None => format!("/nonexistent/{kind}"),
        };
        app.config.agent_commands.insert(kind.into(), command);
    }
    (app, rx, dir)
}

fn with_claude(script: &str) -> (App, Receiver<AppEvent>, TempDir) {
    with_agents(&[(agents::CLAUDE, script)])
}

fn shown(app: &App) -> Option<ui::Usage> {
    match app.overlay_view(app.overlay.as_ref()?, AREA)? {
        ui::Overlay::Usage(usage) => Some(usage),
        _ => None,
    }
}

fn sections(app: &App) -> Vec<ui::UsageSection> {
    shown(app).map(|u| u.sections).unwrap_or_default()
}

fn windows(app: &App) -> Vec<(String, u16)> {
    sections(app).into_iter().flat_map(|s| s.windows).map(|w| (w.label, w.percent)).collect()
}

fn errors(app: &App) -> Vec<String> {
    sections(app).into_iter().filter_map(|s| s.error).collect()
}

fn titles(app: &App) -> Vec<String> {
    sections(app).into_iter().map(|s| s.title).collect()
}

#[test]
fn stats_opens_without_a_probe_and_tab_hits_switch_to_limits() {
    let (mut app, _rx, dir) = with_claude("echo run > \"$(dirname \"$0\")/runs\"\nexit 1");
    click(&mut app, areas().usage.as_position());
    assert_eq!(shown(&app).expect("dialog").page, usage::Page::Stats);
    assert!(sections(&app).is_empty());
    assert!(!dir.path().join("runs").exists());
    let view = shown(&app).expect("dialog");
    click(&mut app, ui::usage_tabs(AREA, &view)[0].as_position());
    assert_eq!(shown(&app).expect("dialog").page, usage::Page::Limits);
    assert_eq!(sections(&app).len(), 1);
    send_key(&mut app, KeyCode::Right, KeyModifiers::NONE);
    assert_eq!(shown(&app).expect("dialog").page, usage::Page::Stats);
    send_key(&mut app, KeyCode::BackTab, KeyModifiers::SHIFT);
    assert_eq!(shown(&app).expect("dialog").page, usage::Page::Limits);
    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    app.open_usage();
    assert_eq!(shown(&app).expect("dialog").page, usage::Page::Limits);
}

#[test]
fn sharing_copies_only_after_something_was_counted() {
    let (mut app, _rx, _dir) = with_agents(&[]);
    app.open_usage();
    let view = shown(&app).expect("dialog");
    click(&mut app, ui::usage_share(AREA, &view).as_position());
    assert_eq!(app.take_host_writes(), Vec::<Vec<u8>>::new());
    let now = SystemTime::now();
    app.agents
        .stats
        .counted(agents::stats::Kind::Claude, agents::stats::Counted { prompts: vec![now], tokens: Vec::new() });
    let expected = agents::stats::share_link(&app.agents.stats, now);
    let view = shown(&app).expect("dialog");
    click(&mut app, ui::usage_share(AREA, &view).as_position());
    assert_eq!(app.take_host_writes(), [clipboard::osc52(&expected)]);
    assert_eq!(toast(&app), Some("copied a link to your stats"));
}

#[test]
fn the_button_shows_loading_then_the_windows() {
    let (mut app, rx, _dir) = with_claude(&format!("read -r a\nread -r b\necho '{ANSWER}'"));
    click(&mut app, areas().usage.as_position());
    app.switch_usage(usage::Page::Limits);
    let loading: Vec<(String, String)> = sections(&app).into_iter().map(|s| (s.title, s.status)).collect();
    assert_eq!((loading, windows(&app)), (vec![("Claude Code".into(), "loading…".into())], Vec::new()));

    pump_until(&mut app, &rx, "the usage arrives", |a| !windows(a).is_empty());
    assert_eq!((windows(&app), errors(&app)), (vec![("session (5h)".into(), 42)], Vec::new()));
}

#[test]
fn every_installed_agent_gets_a_section() {
    let (mut app, rx, _dir) = with_agents(&[
        (agents::CLAUDE, &format!("read -r a\nread -r b\necho '{ANSWER}'")),
        (agents::CODEX, &format!("read -r a\nread -r b\nread -r c\necho '{CODEX_ANSWER}'")),
    ]);
    click(&mut app, areas().usage.as_position());
    app.switch_usage(usage::Page::Limits);
    pump_until(&mut app, &rx, "both answers arrive", |a| windows(a).len() == 2);
    assert_eq!(
        (titles(&app), windows(&app)),
        (
            vec!["Claude Code · max plan".into(), "Codex · plus plan".into()],
            vec![("session (5h)".into(), 42), ("session (5h)".into(), 17)]
        )
    );
}

#[test]
fn an_agent_that_is_not_installed_has_no_section() {
    let (mut app, _rx, _dir) = with_agents(&[(agents::CODEX, "exec sleep 30")]);
    app.usage_timeout = Duration::from_millis(200);
    click(&mut app, areas().usage.as_position());
    app.switch_usage(usage::Page::Limits);
    assert_eq!(titles(&app), ["Codex"]);
}

#[test]
fn without_any_agent_installed_each_one_says_why() {
    let (mut app, rx, _dir) = with_agents(&[]);
    click(&mut app, areas().usage.as_position());
    app.switch_usage(usage::Page::Limits);
    pump_until(&mut app, &rx, "both probes fail", |a| errors(a).len() == 2);
    let errors = errors(&app);
    assert!(errors[0].starts_with("usage unavailable: could not run /nonexistent/claude"), "{errors:?}");
    assert!(errors[1].starts_with("usage unavailable: could not run /nonexistent/codex"), "{errors:?}");
}

#[test]
fn a_claude_that_hangs_shows_usage_unavailable() {
    let (mut app, rx, _dir) = with_claude("exec sleep 30");
    app.usage_timeout = Duration::from_millis(200);
    click(&mut app, areas().usage.as_position());
    app.switch_usage(usage::Page::Limits);
    pump_until(&mut app, &rx, "the probe times out", |a| !errors(a).is_empty());
    assert_eq!(errors(&app), ["usage unavailable: claude did not answer in time"]);
}

#[test]
fn reopening_while_loading_starts_no_second_probe() {
    let (mut app, rx, dir) = with_claude("echo run >> \"$(dirname \"$0\")/runs\"\nexec sleep 30");
    app.usage_timeout = Duration::from_millis(300);
    click(&mut app, areas().usage.as_position());
    app.switch_usage(usage::Page::Limits);
    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    click(&mut app, areas().usage.as_position());
    app.switch_usage(usage::Page::Limits);
    pump_until(&mut app, &rx, "the probe times out", |a| !errors(a).is_empty());
    let runs = std::fs::read_to_string(dir.path().join("runs")).expect("the probe ran");
    assert_eq!(runs.lines().count(), 1);
}

#[rstest::rstest]
#[case::done_button(true)]
#[case::escape(false)]
fn done_and_escape_close_it(#[case] button: bool) {
    let (mut app, _rx, _dir) = with_claude("exit 1");
    click(&mut app, areas().usage.as_position());
    app.switch_usage(usage::Page::Limits);
    if button {
        let usage = shown(&app).expect("the modal is open");
        click(&mut app, ui::usage_done(AREA, &usage).as_position());
    } else {
        send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    }
    assert!(app.overlay.is_none());
}
