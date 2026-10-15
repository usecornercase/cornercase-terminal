use super::*;

fn remembered_claude() -> (App, Receiver<AppEvent>, Vec<TempDir>, FakeClaude, State) {
    let (mut app, rx, dirs) = app_with(1);
    let claude = FakeClaude::running(FakeClaude::ANSWERING);
    claude.start(&mut app);
    let saved = remembered(&mut app, &rx);
    (app, rx, dirs, claude, saved)
}

const CODEX_ID: &str = "019a1234-5678-7000-8000-000000000001";

fn saved_agent(saved: &State) -> Option<&AgentState> {
    saved.projects[0].workspaces[0].tabs[0].panes[0].agent.as_ref()
}

fn remembered(app: &mut App, rx: &Receiver<AppEvent>) -> State {
    watch_until(app, rx, "the conversation is known", |a| term(a, 0).resume.is_some());
    app.state()
}

fn restored(saved: &State, kind: &str, on: bool) -> (App, Receiver<AppEvent>, TempDir) {
    let (mut app, rx) = empty_app();
    let bin = TempDir::new();
    let fake = bin.path().join(kind);
    write_executable(&fake, "#!/bin/sh\necho \"resumed: $*\"\n");
    app.config.agent_commands.insert(kind.into(), fake.display().to_string());
    app.config.resume_agents = on;
    app.restore(saved, AREA);
    (app, rx, bin)
}

fn squeezed(text: &str) -> String {
    text.split_whitespace().collect()
}

fn wait_resumed(app: &mut App, rx: &Receiver<AppEvent>, line: &str) {
    wait_until("the conversation is resumed", || {
        drain(app, rx);
        app.refresh(Instant::now());
        squeezed(&screen(app)).contains(&squeezed(&format!("resumed: {line}")))
    });
}

#[test]
fn an_agent_restored_in_its_conversation_is_not_a_new_start() {
    let (mut app, rx, _dirs, claude, saved) = remembered_claude();
    assert_eq!(app.stats_view(SystemTime::now()).windows["total"].counts.agents, 1);
    claude.signal("quit");
    watch_until(&mut app, &rx, "old agent exits", |a| !runs(a, agents::Known::Claude.name()));
    std::fs::remove_file(claude.dir().join("quit")).expect("clear quit");
    let (mut back, rx, _dirs) = app_with(0);
    back.config.agent_commands.insert(agents::Known::Claude.name().into(), claude.script().display().to_string());
    back.config.agent_args.insert(agents::Known::Claude.name().into(), vec![claude.dir().display().to_string()]);
    back.agents.claude_dir = Some(claude.dir().to_path_buf());
    assert!(back.restore(&saved, AREA));
    refreshing(&mut back, &rx, "resumed conversation", |a| {
        a.term().and_then(|t| t.context.conversation()).is_some()
    });
    assert_eq!(back.stats_view(SystemTime::now()).windows["total"].counts.agents, 0);
    claude.signal("quit");
}

#[test]
fn claude_comes_back_in_its_conversation_and_mode() {
    let (mut app, rx, _dirs) = app_with(1);
    let claude = FakeClaude::running(FakeClaude::ANSWERING);
    claude.start_with(&mut app, "--permission-mode plan");
    let saved = remembered(&mut app, &rx);
    claude.signal("quit");
    let (mut back, rx, _bin) = restored(&saved, agents::Known::Claude.name(), true);

    wait_resumed(&mut back, &rx, "--permission-mode plan --resume s1");

    let expected = AgentState { kind: "claude".into(), conversation: "s1".into(), mode: Some("plan".into()) };
    assert_eq!(saved_agent(&saved), Some(&expected));
}

#[test]
fn codex_comes_back_in_its_conversation() {
    let (mut app, rx, _dirs) = app_with(1);
    let codex = FakeCodex::new(CODEX_ID, "gpt-5.4", false);
    type_line(&mut app, &codex.command_line());
    let saved = remembered(&mut app, &rx);
    codex.signal("quit", "");
    let (mut back, rx, _bin) = restored(&saved, agents::Known::Codex.name(), true);

    wait_resumed(&mut back, &rx, &format!("resume {CODEX_ID}"));
}

#[test]
fn gemini_comes_back_in_its_conversation_and_plan_mode() {
    let (mut app, rx, _dirs) = app_with(1);
    let gemini = FakeGemini::new(&app.projects[0].path);
    type_line(&mut app, &format!("{} --approval-mode plan", gemini.command_line()));
    watch_until(&mut app, &rx, "Gemini is ready", |a| status(a, 0, 0) == Some(Status::Idle));
    gemini.append(crate::test_util::fixture!("gemini/0.63.0/prompt.jsonl"));
    let saved = remembered(&mut app, &rx);
    let (mut back, rx, _bin) = restored(&saved, agents::Known::Gemini.name(), true);
    wait_resumed(&mut back, &rx, &format!("--approval-mode plan --resume {}", FakeGemini::ID));
    assert_eq!(saved_agent(&saved).and_then(|s| s.mode.as_deref()), Some("plan"));
}

#[test]
fn grok_comes_back_in_its_conversation_and_mode() {
    let (mut app, rx, _dirs) = app_with(1);
    let grok = FakeGrok::new(&app.projects[0].path);
    type_line(&mut app, &format!("{} --permission-mode acceptEdits", grok.command_line()));
    pump_until(&mut app, &rx, "grok runs", |a| runs(a, agents::Known::Grok.name()));
    let pid = app.term().and_then(Term::foreground_pid).expect("the pid of grok");
    grok.bind(pid);
    let saved = remembered(&mut app, &rx);
    let (mut back, rx, _bin) = restored(&saved, agents::Known::Grok.name(), true);
    wait_resumed(&mut back, &rx, &format!("--permission-mode acceptEdits --resume {}", FakeGrok::ID));
    assert_eq!(saved_agent(&saved).and_then(|s| s.mode.as_deref()), Some("accept edits"));
}

#[test]
fn the_conversation_is_kept_until_its_agent_is_back_or_the_grace_runs_out() {
    let (_app, _rx, _dirs, claude, saved) = remembered_claude();
    claude.signal("quit");
    let (mut back, _rx, _bin) = restored(&saved, agents::Known::Claude.name(), true);
    back.agents.launches.clear();
    let now = Instant::now();

    back.watch_agents(now + RESUME_GRACE / 2);
    let kept = saved_agent(&back.state()).cloned();
    back.watch_agents(now + RESUME_GRACE + WATCH_AGENTS_EVERY);

    assert_eq!((kept.as_ref(), saved_agent(&back.state())), (saved_agent(&saved), None));
}

#[test]
fn a_pane_whose_agent_quit_comes_back_as_a_shell() {
    let (mut app, rx, _dirs) = app_with(1);
    let claude = FakeClaude::running(FakeClaude::ANSWERING);
    claude.start(&mut app);
    remembered(&mut app, &rx);

    claude.signal("quit");

    watch_until(&mut app, &rx, "the conversation is forgotten", |a| saved_agent(&a.state()).is_none());
}

#[test]
fn nothing_resumes_when_it_is_turned_off() {
    let (_app, _rx, _dirs, claude, saved) = remembered_claude();
    claude.signal("quit");

    let (back, _rx, _bin) = restored(&saved, agents::Known::Claude.name(), false);

    assert_eq!((back.agents.launches.len(), saved_agent(&back.state())), (0, None));
}
