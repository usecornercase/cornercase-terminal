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

const QUITS: &str = "#!/bin/sh\necho \"resumed: $*\"\n";

fn restored(saved: &State, kind: &str, on: bool) -> (App, Receiver<AppEvent>, TempDir) {
    restored_running(saved, kind, on, QUITS)
}

fn restored_running(saved: &State, kind: &str, on: bool, script: &str) -> (App, Receiver<AppEvent>, TempDir) {
    let (mut app, rx) = empty_app();
    let bin = TempDir::new();
    let fake = bin.path().join(kind);
    write_executable(&fake, script);
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
    back.agents.set_claude_dir(claude.dir());
    assert!(back.restore(&saved, AREA));
    refreshing(&mut back, &rx, "resumed conversation", |a| a.term().and_then(|t| t.context.conversation()).is_some());
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

    let expected =
        AgentState { kind: "claude".into(), conversation: "s1".into(), mode: Some("plan".into()), account: None };
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
fn cursor_comes_back_in_its_conversation_and_mode() {
    let (mut app, rx, _dirs) = app_with(1);
    let cursor = FakeCursor::new(&app.projects[0].path);
    type_line(&mut app, &format!("{} --mode plan", cursor.command_line()));
    pump_until(&mut app, &rx, "cursor runs", |a| runs(a, agents::Known::Cursor.name()));
    watch_until(&mut app, &rx, "cursor holds its store", |a| {
        a.term().is_some_and(|t| t.context.record_for(agents::Known::Cursor).is_some())
    });
    cursor.turn("hello");
    cursor.reply("hi");
    let saved = remembered(&mut app, &rx);
    let (mut back, rx, _bin) = restored(&saved, agents::Known::Cursor.name(), true);

    wait_resumed(&mut back, &rx, &format!("--mode plan --resume {}", agents::cursor::fake::ID));

    assert_eq!(saved_agent(&saved).and_then(|s| s.mode.as_deref()), Some("plan"));
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

    assert_eq!((back.agents.launches.list().len(), saved_agent(&back.state())), (0, None));
}

mod input_while_resuming {
    use super::*;
    use crate::control::{Command, Request, Response, SendText};

    const STAYS: &str = "#!/bin/sh\necho \"resumed: $*\"\nexec sleep 30\n";

    fn resuming(script: &str) -> (App, Receiver<AppEvent>, TempDir, u64) {
        let (_app, _rx, _dirs, claude, saved) = remembered_claude();
        claude.signal("quit");
        let (back, rx, bin) = restored_running(&saved, agents::Known::Claude.name(), true, script);
        let id = back.term().expect("the restored pane").id;
        (back, rx, bin, id)
    }

    fn type_text(app: &mut App, text: &str) {
        text.chars().for_each(|c| app.forward_key(KeyEvent::from(KeyCode::Char(c))));
    }

    fn send(app: &mut App, pane: u64, force: bool) -> Response {
        let send = SendText { pane: Some(pane), text: Some("hello".into()), force, ..SendText::default() };
        let request = Request {
            caller: None,
            server: None,
            contract: None,
            boot: None,
            view: None,
            command: Command::Send(send),
        };
        let text = serde_json::to_string(&request).expect("json");
        app.request(1, &text, Some(AREA), Instant::now());
        let (_, answer) = app.take_answers().pop().expect("an answer");
        serde_json::from_str(&answer).expect("a response")
    }

    fn ready(app: &mut App, rx: &Receiver<AppEvent>, pane: u64) {
        wait_until_within("the pane takes input again", Duration::from_secs(15), || {
            drain(app, rx);
            app.refresh(Instant::now());
            !app.resume_holds(pane)
        });
    }

    #[test]
    fn keys_and_a_paste_are_dropped_and_say_why() {
        let (mut back, rx, _bin, id) = resuming(STAYS);
        assert!(back.resume_holds(id));

        type_text(&mut back, "zz");
        back.handle_paste("pasted-text", AREA);

        assert!(
            toast(&back).is_some_and(|t| t.starts_with("claude is resuming its conversation")),
            "{:?}",
            toast(&back)
        );
        ready(&mut back, &rx, id);
        assert!(!squeezed(&screen(&mut back)).contains("pasted-text"));
        assert!(!squeezed(&screen(&mut back)).contains("zzclaude"));
    }

    #[test]
    fn the_pane_takes_input_again_once_the_agent_has_drawn() {
        let (mut back, rx, _bin, id) = resuming(STAYS);

        ready(&mut back, &rx, id);

        assert!(back.term().is_some_and(|t| !t.shell_in_foreground()), "the agent still runs");
    }

    #[test]
    fn a_resume_whose_agent_exits_gives_the_pane_back() {
        let (mut back, rx, _bin, id) = resuming(QUITS);

        ready(&mut back, &rx, id);

        assert!(back.term().is_some_and(Term::shell_in_foreground));
    }

    #[test]
    fn the_hold_ends_with_the_grace_even_while_the_agent_still_starts() {
        let (mut back, _rx, _bin, id) = resuming(STAYS);
        let held = back.resume_holds(id);

        if let Some(launch) = back.agents.launches.of_pane_mut(id).next() {
            launch.resume = Some(Instant::now());
        }

        assert_eq!((held, back.resume_holds(id), back.agents.launches.list().len()), (true, false, 1));
    }

    #[test]
    fn cancel_before_the_line_is_typed_leaves_a_shell_that_takes_keys() {
        let (mut back, rx, _bin, id) = resuming(STAYS);
        let pane = back.tab().and_then(|t| t.shown(back.layout(AREA).pane).first().map(|(_, r)| *r)).expect("a pane");
        let (_, button) = ui::resume_notice(pane, "claude").expect("the notice fits");

        click(&mut back, button.as_position());
        type_text(&mut back, "echo ok-$((1+1))");
        back.forward_key(KeyEvent::from(KeyCode::Enter));

        wait_until("the shell answers", || {
            drain(&mut back, &rx);
            back.refresh(Instant::now());
            screen(&mut back).contains("ok-2")
        });
        let (held, resumed) = (back.resume_holds(id), screen(&mut back).contains("resumed:"));
        assert_eq!((held, resumed, saved_agent(&back.state())), (false, false, None));
    }

    #[test]
    fn send_is_refused_unless_forced() {
        let (mut back, _rx, _bin, id) = resuming(STAYS);

        let refused = send(&mut back, id, false);
        let forced = send(&mut back, id, true);

        assert!(
            matches!(&refused, Response::Error(e) if e.contains("is resuming its agent's conversation")),
            "{refused:?}"
        );
        assert!(matches!(forced, Response::Ok(_)), "{forced:?}");
    }
}

#[test]
fn copilot_comes_back_in_its_conversation_and_mode() {
    let (mut app, rx, _dirs) = app_with(1);
    let copilot = FakeCopilot::new(&app.projects[0].path);
    type_line(&mut app, &format!("{} --mode plan", copilot.command_line()));
    pump_until(&mut app, &rx, "copilot runs", |a| runs(a, agents::Known::Copilot.name()));
    watch_until(&mut app, &rx, "copilot holds its session", |a| {
        a.term().is_some_and(|t| t.context.record_for(agents::Known::Copilot).is_some())
    });
    copilot.started("gpt-4.1");
    copilot.prompt("hello");
    copilot.reply("hi", "gpt-4.1");
    let saved = remembered(&mut app, &rx);
    let (mut back, rx, _bin) = restored(&saved, agents::Known::Copilot.name(), true);

    wait_resumed(&mut back, &rx, &format!("--mode plan --resume={}", agents::copilot::fake::ID));

    assert_eq!(saved_agent(&saved).and_then(|s| s.mode.as_deref()), Some("plan"));
}

#[test]
fn a_restored_copilot_keeps_its_conversation_before_its_first_prompt() {
    let (mut app, rx, _dirs) = app_with(1);
    let copilot = FakeCopilot::new(&app.projects[0].path);
    type_line(&mut app, &copilot.command_line());
    watch_until(&mut app, &rx, "copilot holds its session", |a| {
        a.term().is_some_and(|t| t.context.record_for(agents::Known::Copilot).is_some())
    });
    copilot.started("gpt-4.1");
    copilot.prompt("hello");
    let saved = remembered(&mut app, &rx);
    copilot.quit();
    watch_until(&mut app, &rx, "copilot exits", |a| !runs(a, agents::Known::Copilot.name()));
    let (mut back, rx, _dirs) = app_with(0);
    back.config.agent_commands.insert(agents::Known::Copilot.name().into(), copilot.script.display().to_string());
    std::fs::remove_file(copilot.dir.path().join("signals/quit")).expect("clear quit");
    assert!(back.restore(&saved, AREA));
    refreshing(&mut back, &rx, "the conversation is known from the arguments", |a| {
        a.term().and_then(|t| t.context.conversation()) == Some(agents::copilot::fake::ID)
    });
    back.agents.launches.clear();
    let now = Instant::now();

    back.watch_agents(now + RESUME_GRACE + WATCH_AGENTS_EVERY);

    assert_eq!(saved_agent(&back.state()), saved_agent(&saved));
    copilot.quit();
}
