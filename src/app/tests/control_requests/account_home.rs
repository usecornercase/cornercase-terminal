use super::*;
use crate::agents::Known;

fn in_its_own_home() -> (Agent, [TempDir; 2], u64) {
    let mut agent = Agent::new();
    let (elsewhere, bin) = (TempDir::new(), TempDir::new());
    let wrapper = bin.path().join("claude");
    let script = format!(
        "#!/bin/sh\nexport {}='{}'\nexec '{}' \"$@\"\n",
        crate::agents::claude::CLAUDE_DIR_ENV,
        agent.fake.dir().display(),
        agent.fake.script().display()
    );
    write_executable(&wrapper, &script);
    agent.app.config.agent_commands.insert(Known::Claude.name().into(), wrapper.display().to_string());
    agent.app.agents.claude_dir = Some(elsewhere.path().to_path_buf());
    let id = agent.start(None);
    (agent, [elsewhere, bin], id)
}

fn with_an_account() -> (Agent, TempDir, u64) {
    let mut agent = Agent::new();
    let elsewhere = TempDir::new();
    let root = agent.config_dir.path().join("accounts");
    std::fs::create_dir_all(root.join(Known::Claude.name())).expect("the accounts folder");
    std::os::unix::fs::symlink(agent.fake.dir(), root.join(Known::Claude.name()).join("work")).expect("the account");
    agent.app.set_accounts_dir(&root);
    agent.app.config.profiles.insert("Work".into(), [(Known::Claude.name().to_string(), "work".to_string())].into());
    agent.app.config.profile = "Work".into();
    agent.app.agents.claude_dir = Some(elsewhere.path().to_path_buf());
    let id = agent.start(None);
    (agent, elsewhere, id)
}

#[test]
fn an_agent_with_its_own_config_folder_is_read_from_that_folder() {
    let (agent, _dirs, id) = in_its_own_home();
    reads_its_own_folder(agent, id);
}

#[test]
fn an_agent_in_a_pane_with_an_account_is_read_from_the_account_s_folder() {
    let (agent, _elsewhere, id) = with_an_account();
    reads_its_own_folder(agent, id);
}

#[test]
fn a_pane_with_an_account_names_it_in_status() {
    let (mut agent, _elsewhere, id) = with_an_account();
    agent.refreshing("the account is seen", |a| pane(a, id).account().is_some());

    let info = agents::in_status(&mut agent.app, id);

    assert_eq!(info.account.as_deref(), Some("work"));
}

#[test]
fn a_pane_without_an_account_has_no_account_key() {
    let mut agent = Agent::new();
    let id = agent.start(None);
    agent.refreshing("the agent is seen", |a| pane(a, id).agent.status().is_some());

    let info = serde_json::to_value(agents::in_status(&mut agent.app, id)).expect("the pane's JSON");

    assert_eq!(info.get("id").and_then(Value::as_u64), Some(id));
    assert!(info.get("account").is_none(), "{info}");
}

fn reads_its_own_folder(mut agent: Agent, id: u64) {
    agent.transcript(id, &[
        r#"{"type":"user","message":{"role":"user","content":"fix the login"}}"#,
        r#"{"type":"assistant","timestamp":"2026-10-08T19:56:37.241Z","message":{"id":"m1","model":"claude-opus-5-5","content":[{"type":"text","text":"Fixed the login form."}],"usage":{"input_tokens":2,"cache_creation_input_tokens":15655,"cache_read_input_tokens":149954}}}"#,
    ]);
    agent.refreshing("the pane shows its context", |a| pane(a, id).context.context().is_some());

    ask(&mut agent.app, None, send_text(id, "next step", true, false));
    done(agent.answered("the prompt is confirmed"));
    agent.refreshing("the agent works", |a| pane(a, id).agent.status() == Some(activity::Status::Working));
    ask(&mut agent.app, None, Command::LastMessage(wire::LastMessage { pane: Some(id), tab: None }));
    let read = done(agent.answered("the transcript is read"));

    let context = pane(&agent.app, id).context.context().cloned().expect("the context line");
    assert_eq!(context.model, "Opus 5.5");
    assert_eq!(read.text.as_deref(), Some("Fixed the login form."));
}

#[test]
fn a_restored_agent_resumes_with_its_account() {
    let (mut agent, _elsewhere, id) = with_an_account();
    agent.transcript(id, &[r#"{"type":"user","message":{"role":"user","content":"fix the login"}}"#]);
    agent.refreshing("the account is remembered", |a| {
        pane(a, id).resume.as_ref().is_some_and(|resume| resume.account.as_deref() == Some("work"))
    });
    let saved = agent.app.state();
    let panes = saved.projects.iter().flat_map(|p| &p.workspaces).flat_map(|w| &w.tabs).flat_map(|t| &t.panes);
    let pane_state = panes.into_iter().find(|pane| pane.agent.is_some()).expect("the agent's pane");
    assert_eq!(pane_state.accounts.get(Known::Claude.name()).map(String::as_str), Some("work"));
    let root = agent.config_dir.path().join("accounts");
    let home = root.join(Known::Claude.name()).join("work").canonicalize().expect("the account's folder");
    let bin = TempDir::new();
    let out = bin.path().join("env.txt");
    let script = format!("#!/bin/sh\nenv > '{0}.tmp' && mv '{0}.tmp' '{0}'\necho \"resumed: $*\"\n", out.display());
    let fake = bin.path().join("claude");
    write_executable(&fake, &script);
    let (mut back, rx) = empty_app();
    back.set_accounts_dir(&root);
    back.config.agent_commands.insert(Known::Claude.name().into(), fake.display().to_string());
    back.config.resume_agents = true;

    back.restore(&saved, AREA);

    wait_until("the resume line ran", || {
        drain(&mut back, &rx);
        back.refresh(Instant::now());
        out.exists()
    });
    let env = std::fs::read_to_string(&out).expect("read the environment");
    let dir = format!("{}={}", crate::agents::claude::CLAUDE_DIR_ENV, home.display());
    assert!(env.lines().any(|line| line == dir), "{env}");
    assert!(back.projects.terms().any(|term| term.accounts().name(Known::Claude) == Some("work")));
}

#[test]
fn a_pane_with_an_account_names_it_on_its_tab_row_unless_the_mark_is_off() {
    let (mut agent, _elsewhere, id) = with_an_account();
    agent.app.relist_accounts();
    agent.refreshing("the account is seen", |a| pane(a, id).account().is_some());
    let shown = |app: &App| {
        let (_, tab) = app.projects.tab_with_pane(id).expect("the agent's tab");
        let agents = app.agents_view().entries.into_iter().map(|entry| entry.details.account).collect::<Vec<_>>();
        (app.tab_entry(tab).details.account, agents)
    };
    assert_eq!(shown(&agent.app), (None, vec![None]), "no account folder is listed yet");

    let accounts = agent.config_dir.path().join("accounts").join(Known::Claude.name());
    std::fs::create_dir(accounts.join("home")).expect("a listed account");
    agent.app.relist_accounts();
    assert_eq!(shown(&agent.app), (Some("work".into()), vec![Some("work".into())]));

    let labels = [("work".to_string(), "Office".to_string())].into();
    agent.app.config.account_names.insert(Known::Claude.name().into(), labels);
    assert_eq!(shown(&agent.app), (Some("Office".into()), vec![Some("Office".into())]));

    agent.app.config.account_mark = false;
    assert_eq!(shown(&agent.app), (None, vec![None]));
}

#[test]
fn the_system_login_is_named_only_while_its_agent_has_accounts() {
    let (mut app, _rx) = empty_app();
    let root = TempDir::new();
    app.set_accounts_dir(root.path());
    app.relist_accounts();
    assert_eq!(app.account_label(Known::Claude, None), None);
    assert_eq!(app.account_label(Known::Claude, Some("work")).as_deref(), Some("work"));

    crate::accounts::add(root.path(), Known::Claude, "home", None).expect("an account");
    app.relist_accounts();
    assert_eq!(app.account_label(Known::Claude, None).as_deref(), Some(crate::config::SYSTEM_ACCOUNT));
    assert_eq!(app.account_label(Known::Codex, None), None);

    let labels = [(String::new(), "work".to_string()), ("home".to_string(), "Personal".to_string())].into();
    app.config.account_names.insert(Known::Claude.name().into(), labels);
    assert_eq!(app.account_label(Known::Claude, None).as_deref(), Some("work"));
    assert_eq!(app.account_label(Known::Claude, Some("home")).as_deref(), Some("Personal"));
    assert_eq!(app.account_label(Known::Claude, Some(crate::accounts::CUSTOM)).as_deref(), Some("custom"));
}
