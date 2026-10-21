use std::os::unix::fs::MetadataExt;

use super::*;
use crate::accounts;
use crate::agents::Known;
use crate::agents::claude::fake::FakeClaude;
use crate::agents::codex::fake::FakeCodex;
use crate::control::{self as wire, Command, Until};
use crate::test_util::write_executable;

mod menu;
mod rows;
mod shells;

struct Setup {
    app: App,
    rx: Receiver<AppEvent>,
    config: TempDir,
    project: TempDir,
    _bin: TempDir,
    claude: FakeClaude,
}

impl Setup {
    fn new(script: &str) -> Self {
        let (config, project, bin) = (TempDir::new(), TempDir::new(), TempDir::new());
        let (mut app, rx) = new_app(config.path().join("config.json"));
        app.set_accounts_dir(&config.path().join("accounts"));
        app.set_default_homes(&config.path().join("homes"));
        let resumed = bin.path().join("claude");
        write_executable(&resumed, "#!/bin/sh\necho \"restarted: $*\"\n");
        app.config.agent_commands.insert("claude".into(), resumed.display().to_string());
        Self { app, rx, config, project, _bin: bin, claude: FakeClaude::running(script) }
    }

    fn add(&self, name: &str) -> PathBuf {
        let home = accounts::add(&self.config.path().join("accounts"), Known::Claude, name, None).expect("add");
        std::fs::create_dir_all(home.join("sessions")).expect("sessions folder");
        home
    }

    fn wait_on(&mut self, pane: u64, until: Until) {
        let command = Command::Wait(wire::Wait { pane: Some(pane), until, ..wire::Wait::default() });
        let request = wire::Request { caller: None, server: None, contract: None, boot: None, view: None, command };
        self.app.request(7, &serde_json::to_string(&request).expect("json"), Some(AREA), Instant::now());
        assert_eq!(self.app.take_answers(), Vec::new());
    }

    fn start_on(&mut self, name: &str) -> u64 {
        let home = self.add(name);
        self.app.config.active_accounts.insert("claude".into(), name.into());
        self.app.open_project(self.project.path().to_path_buf(), AREA).expect("open the project");
        let line = format!("env -u CLAUDE_CONFIG_DIR {} {}", self.claude.script().display(), home.display());
        type_line(&mut self.app, &line);
        self.app.term().expect("the pane").id
    }

    fn shell_on(&mut self, name: &str) -> u64 {
        self.add(name);
        self.app.config.active_accounts.insert("claude".into(), name.into());
        self.app.open_project(self.project.path().to_path_buf(), AREA).expect("open the project");
        wait_until("the shell is at its prompt", || self.app.term().is_some_and(Term::shell_in_foreground));
        self.app.term().expect("the pane").id
    }

    fn seat_coder(&mut self, pane: u64) {
        let workspace = self.app.projects[0].workspaces[0].id;
        let mut run = crate::flows::run::Run::new(workspace, crate::flows::Flow::default(), "fix it".into());
        run.members.push(crate::flows::run::Member { role: "coder".into(), pane });
        self.app.runs.push(run);
    }

    fn set_custom(&mut self, pane: u64) {
        let root = self.config.path().join("accounts");
        let outside = self.project.path().join("elsewhere");
        let env = move |_| vec![("CLAUDE_CONFIG_DIR".to_string(), outside.display().to_string())];
        let term = self.app.projects.pane_by_mut(pane).expect("the pane");
        term.set_accounts(accounts::Accounts::default());
        term.see_account(&root, Some((i32::MAX, Known::Claude, &[])), env);
        assert_eq!(term.agent_account(), Some((Known::Claude, Some(accounts::CUSTOM))));
    }

    fn shell_beside_personal() -> (Self, u64) {
        let mut setup = Self::new(FakeClaude::SILENT);
        let pane = setup.shell_on("work");
        setup.add("personal");
        (setup, pane)
    }

    fn runs_sleep(&mut self, pane: u64) {
        type_line(&mut self.app, "sleep 30");
        pump_until(&mut self.app, &self.rx, "sleep runs", |app| {
            app.projects.pane_by(pane).is_some_and(|term| !term.shell_in_foreground())
        });
    }

    fn watch_until(&mut self, what: &str, cond: impl Fn(&App) -> bool) {
        let mut now = Instant::now();
        wait_until(what, || {
            drain(&mut self.app, &self.rx);
            now += Duration::from_secs(1);
            self.app.watch_agents(now);
            cond(&self.app)
        });
    }

    fn runs(&mut self) {
        self.watch_until("claude runs", |app| app.term().is_some_and(|term| term.agent_account().is_some()));
    }

    fn known(&mut self) {
        self.watch_until("the conversation is known", |app| {
            app.term().is_some_and(|term| term.resume.is_some() && term.agent_account().is_some())
        });
    }

    fn untouched(&mut self, pane: u64) {
        self.app.projects.pane_by_mut(pane).expect("the pane").input_at = None;
    }

    fn restarted(&mut self, pane: u64) {
        pump_until(&mut self.app, &self.rx, "the pane is restarted", |app| {
            app.projects.pane_by(pane).is_some_and(|term| term.generation() == 1)
        });
    }

    fn typed(&mut self, line: &str) {
        wait_until(line, || {
            drain(&mut self.app, &self.rx);
            self.app.refresh(Instant::now());
            screen(&mut self.app).contains(line)
        });
    }

    fn transcript(&self, home: &Path) -> PathBuf {
        let dir = self.project.path().canonicalize().expect("canonical project");
        let encoded: String =
            dir.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
        home.join("projects").join(encoded).join("s1.jsonl")
    }
}

fn inode(path: &Path) -> u64 {
    std::fs::metadata(path).expect("metadata").ino()
}

#[test]
fn claude_takes_its_conversation_to_the_new_account_and_resumes_it() {
    let mut setup = Setup::new(FakeClaude::ANSWERING);
    let pane = setup.start_on("work");
    let personal = setup.add("personal");
    setup.known();
    let work = setup.config.path().join("accounts/claude/work").canonicalize().expect("work");
    let source = setup.transcript(&work);
    let subagent = source.with_extension("").join("subagents/agent-a1.jsonl");
    std::fs::create_dir_all(subagent.parent().expect("subagents")).expect("subagents folder");
    std::fs::write(&subagent, "{}\n").expect("subagent transcript");
    setup.app.projects.pane_by_mut(pane).expect("the pane").agent.restore(true, true);
    let old_shell = setup.app.term().and_then(Term::shell_pid);

    assert_eq!(setup.app.panes_on_other_accounts(Known::Claude, "personal"), (vec![pane], Vec::new()));
    assert_eq!(setup.app.panes_on_other_accounts(Known::Claude, "work"), (Vec::new(), Vec::new()));
    setup.app.restart_pane(pane, Known::Claude, "personal").expect("the restart starts");
    setup.restarted(pane);

    let personal = personal.canonicalize().expect("personal");
    let moved = setup.transcript(&personal);
    assert_eq!(inode(&moved), inode(&source));
    assert_eq!(inode(&moved.with_extension("").join("subagents/agent-a1.jsonl")), inode(&subagent));
    assert!(source.exists());
    let bands = setup.app.pane_bands();
    assert_eq!(bands.get(&pane), Some(&ui::PaneBand::Resuming("restarting claude on personal".into())));
    resumes_on_personal(setup.app.projects.pane_by(pane).expect("the same pane"), old_shell);
    setup.typed("restarted: --resume s1");
    saves_the_new_account(&setup);
    setup.claude.signal("quit");
}

fn resumes_on_personal(term: &Term, old_shell: Option<i32>) {
    assert_ne!(term.shell_pid(), old_shell);
    assert_eq!(term.accounts().name(Known::Claude), Some("personal"));
    assert_eq!(term.resume.as_ref().and_then(|state| state.account.as_deref()), Some("personal"));
    assert!(term.agent.unseen() && term.agent.pending());
}

fn saves_the_new_account(setup: &Setup) {
    let saved = setup.app.state();
    let saved = &saved.projects[0].workspaces[0].tabs[0].panes[0];
    assert_eq!(saved.accounts.get("claude").map(String::as_str), Some("personal"));
    assert_eq!(saved.agent.as_ref().map(|agent| agent.conversation.as_str()), Some("s1"));
}

#[test]
fn the_agent_s_own_login_takes_it_into_the_default_home() {
    let mut setup = Setup::new(FakeClaude::ANSWERING);
    let pane = setup.start_on("work");
    setup.known();
    let work = setup.config.path().join("accounts/claude/work").canonicalize().expect("work");

    setup.app.restart_pane(pane, Known::Claude, "").expect("the restart starts");
    setup.restarted(pane);

    let home = setup.config.path().join("homes/claude");
    assert_eq!(inode(&setup.transcript(&home)), inode(&setup.transcript(&work)));
    let term = setup.app.projects.pane_by(pane).expect("the same pane");
    assert!(term.accounts().is_empty());
    assert_eq!(term.resume.as_ref().and_then(|state| state.account.clone()), None);
    setup.claude.signal("quit");
}

#[test]
fn another_conversation_in_the_way_keeps_the_pane_as_it_was() {
    let mut setup = Setup::new(FakeClaude::ANSWERING);
    let pane = setup.start_on("work");
    let personal = setup.add("personal").canonicalize().expect("personal");
    setup.known();
    let theirs = setup.transcript(&personal);
    std::fs::create_dir_all(theirs.parent().expect("project folder")).expect("project folder");
    std::fs::write(&theirs, "theirs\n").expect("their transcript");
    let old_shell = setup.app.term().and_then(Term::shell_pid);

    setup.app.restart_pane(pane, Known::Claude, "personal").expect("the restart starts");
    pump_until(&mut setup.app, &setup.rx, "the restart fails", |app| toast(app).is_some());

    let message = toast(&setup.app).unwrap_or_default();
    assert!(message.starts_with("could not restart claude in pane"), "{message}");
    assert!(message.contains("other contents"), "{message}");
    assert_eq!(std::fs::read_to_string(&theirs).expect("theirs"), "theirs\n");
    let term = setup.app.projects.pane_by(pane).expect("the pane");
    assert_eq!((term.generation(), term.shell_pid()), (0, old_shell));
    assert_eq!(term.accounts().name(Known::Claude), Some("work"));
    setup.claude.signal("quit");
}

#[test]
fn an_agent_without_a_known_conversation_starts_a_new_one() {
    let mut setup = Setup::new(FakeClaude::SILENT);
    let pane = setup.start_on("work");
    setup.add("personal");
    setup.runs();
    setup.app.projects.pane_by_mut(pane).expect("the pane").agent.restore(true, false);

    setup.app.restart_pane(pane, Known::Claude, "personal").expect("the restart starts");

    let term = setup.app.projects.pane_by(pane).expect("the same pane");
    assert_eq!(term.generation(), 1);
    assert!(term.resume.is_none() && !term.agent.unseen());
    setup.typed("restarted:");
    assert!(!screen(&mut setup.app).contains("--resume"));
    setup.claude.signal("quit");
}

#[test]
fn requests_on_the_pane_end_with_a_reason() {
    let mut setup = Setup::new(FakeClaude::SILENT);
    let pane = setup.start_on("work");
    setup.add("personal");
    setup.runs();
    setup.wait_on(pane, Until::Shell);

    setup.app.restart_pane(pane, Known::Claude, "personal").expect("the restart starts");

    let answers = setup.app.take_answers();
    assert_eq!(answers.len(), 1);
    assert!(answers[0].1.contains("restarted its claude on personal"), "{answers:?}");
    setup.claude.signal("quit");
}

#[test]
fn a_pane_without_that_agent_is_refused() {
    let mut setup = Setup::new(FakeClaude::SILENT);
    setup.app.open_project(setup.project.path().to_path_buf(), AREA).expect("open the project");
    let pane = setup.app.term().expect("the pane").id;

    let refused = setup.app.restart_pane(pane, Known::Claude, "personal").expect_err("refused");

    assert_eq!(refused, format!("pane {pane} does not run claude"));
    assert_eq!(
        setup.app.restart_pane(u64::MAX, Known::Claude, "").expect_err("gone"),
        format!("pane {} is gone", u64::MAX)
    );
}

#[test]
fn a_flow_member_and_a_custom_account_stay_as_they_are() {
    let mut setup = Setup::new(FakeClaude::SILENT);
    let pane = setup.start_on("work");
    setup.add("personal");
    setup.runs();
    setup.seat_coder(pane);
    let old_shell = setup.app.term().and_then(Term::shell_pid);

    let skipped = vec![(pane, "it is the coder of a flow run".to_string())];
    assert_eq!(setup.app.panes_on_other_accounts(Known::Claude, "personal"), (Vec::new(), skipped));
    let refused = setup.app.restart_pane(pane, Known::Claude, "personal").expect_err("refused");
    assert_eq!(refused, "it is the coder of a flow run");
    assert_eq!(setup.app.ask_restart(pane, Known::Claude, "personal"), Err(refused));
    assert_eq!(setup.app.term().and_then(Term::shell_pid), old_shell);

    setup.app.runs.clear();
    setup.set_custom(pane);

    let skipped = vec![(pane, "its account was not set by cornercase".to_string())];
    assert_eq!(setup.app.panes_on_other_accounts(Known::Claude, "personal"), (Vec::new(), skipped));
    setup.app.restart_pane(pane, Known::Claude, "personal").expect_err("custom is refused");
    assert_eq!(setup.app.projects.pane_by(pane).map(Term::generation), Some(0));
    setup.claude.signal("quit");
}

#[test]
fn codex_takes_its_rollout_to_the_new_account_and_resumes_it() {
    let id = "019a1234-5678-7000-8000-000000000001";
    let (config, project, bin) = (TempDir::new(), TempDir::new(), TempDir::new());
    let (mut app, rx) = new_app(config.path().join("config.json"));
    let root = config.path().join("accounts");
    app.set_accounts_dir(&root);
    accounts::add(&root, Known::Codex, "work", None).expect("work");
    let personal = accounts::add(&root, Known::Codex, "personal", None).expect("personal");
    app.config.active_accounts.insert("codex".into(), "work".into());
    let resumed = bin.path().join("codex");
    let fake = FakeCodex::new(id, "gpt-5.4", false);
    app.open_project(project.path().to_path_buf(), AREA).expect("open the project");
    type_line(&mut app, &fake.command_line());
    let mut now = Instant::now();
    wait_until("codex is known", || {
        drain(&mut app, &rx);
        now += Duration::from_secs(1);
        app.watch_agents(now);
        app.term().is_some_and(|term| term.resume.is_some())
    });
    write_executable(&resumed, "#!/bin/sh\necho \"restarted: $*\"\n");
    app.config.agent_commands.insert("codex".into(), resumed.display().to_string());
    let pane = app.term().expect("the pane").id;

    app.restart_pane(pane, Known::Codex, "personal").expect("the restart starts");
    pump_until(&mut app, &rx, "the pane is restarted", |app| {
        app.projects.pane_by(pane).is_some_and(|term| term.generation() == 1)
    });

    let moved = personal.join("sessions/2026/10/05").join(fake.rollout.file_name().expect("rollout name"));
    assert_eq!(inode(&moved), inode(&fake.rollout));
    wait_until("the restart line is typed", || {
        drain(&mut app, &rx);
        app.refresh(Instant::now());
        screen(&mut app).replace('\n', "").contains(&format!("restarted: resume {id}"))
    });
    fake.signal("quit", "");
}

mod waiting {
    use super::*;
    use crate::app::account_restart::Restart;

    pub(super) fn working() -> (Setup, u64, PathBuf) {
        let mut setup = Setup::new(FakeClaude::CONTROL);
        let pane = setup.start_on("work");
        setup.add("personal");
        let home = setup.config.path().join("accounts/claude/work");
        setup.runs();
        type_line(&mut setup.app, "go");
        setup.watch_until("claude works", |app| app.term().is_some_and(|term| term.agent.state() == Some("working")));
        (setup, pane, home)
    }

    fn turn_ends(setup: &mut Setup, home: &Path) {
        std::fs::write(home.join("finish"), "").expect("finish the turn");
        setup.watch_until("the turn ends", |app| {
            app.projects.pane_by(app.term().map_or(0, |t| t.id)).is_some_and(|t| t.agent.state() == Some("idle"))
        });
    }

    fn drive(setup: &mut Setup, now: Instant) {
        setup.app.agents.set_watched(Some(now));
        setup.app.drive_restarts(now);
    }

    fn later() -> Instant {
        Instant::now() + Duration::from_secs(10)
    }

    fn notice(setup: &App, pane: u64) -> Option<String> {
        match setup.pane_bands().get(&pane) {
            Some(ui::PaneBand::Restarts(text)) => Some(text.clone()),
            _ => None,
        }
    }

    #[test]
    fn a_working_agent_restarts_once_its_turn_is_over() {
        let (mut setup, pane, home) = working();

        assert_eq!(setup.app.ask_restart(pane, Known::Claude, "personal"), Ok(Restart::Waiting));
        drive(&mut setup, Instant::now());

        assert_eq!(setup.app.restart_waiting(pane), Some((Known::Claude, "personal")));
        assert_eq!(notice(&setup.app, pane).as_deref(), Some("restarts on personal after this turn"));
        assert_eq!(setup.app.projects.pane_by(pane).map(Term::generation), Some(0));
        turn_ends(&mut setup, &home);
        setup.app.drive_restarts(later());
        assert_eq!(setup.app.restart_waiting(pane), Some((Known::Claude, "personal")), "no fresh read");
        drive(&mut setup, later());
        setup.restarted(pane);
        assert_eq!(setup.app.restart_waiting(pane), None);
        let term = setup.app.projects.pane_by(pane).expect("the same pane");
        assert_eq!(term.accounts().name(Known::Claude), Some("personal"));
    }

    #[test]
    fn a_prompt_sent_as_the_turn_ends_holds_the_restart_until_it_is_answered() {
        let (mut setup, pane, home) = working();
        setup.app.ask_restart(pane, Known::Claude, "personal").expect("it waits");
        turn_ends(&mut setup, &home);

        let send = serde_json::json!({"command": "send", "args": {"pane": pane, "text": "again", "enter": true}});
        setup.app.request(9, &send.to_string(), None, Instant::now());
        drive(&mut setup, Instant::now());
        assert_eq!(setup.app.restart_waiting(pane), Some((Known::Claude, "personal")), "while it types");
        wait_until("enter is pressed", || {
            drain(&mut setup.app, &setup.rx);
            setup.app.refresh(Instant::now());
            setup.app.projects.pane_by(pane).is_some_and(|term| term.submitted.is_some())
        });
        drive(&mut setup, Instant::now());

        assert_eq!(setup.app.restart_waiting(pane), Some((Known::Claude, "personal")), "before it reacts");
        assert_eq!(setup.app.projects.pane_by(pane).map(Term::generation), Some(0));
        setup.watch_until("claude works again", |app| {
            app.projects.pane_by(pane).is_some_and(|t| t.agent.state() == Some("working"))
        });
        turn_ends(&mut setup, &home);
        drive(&mut setup, later());
        setup.restarted(pane);
    }

    #[test]
    fn a_draft_the_person_types_holds_the_restart() {
        let (mut setup, pane, home) = working();
        setup.app.ask_restart(pane, Known::Claude, "personal").expect("it waits");
        type_text(&mut setup.app, "half a th");
        assert!(setup.app.projects.pane_by(pane).is_some_and(|term| term.draft(Instant::now())));

        turn_ends(&mut setup, &home);
        drive(&mut setup, later());

        assert_eq!(setup.app.restart_waiting(pane), Some((Known::Claude, "personal")));
        assert_eq!(setup.app.projects.pane_by(pane).map(Term::generation), Some(0));
    }

    #[test]
    fn asking_for_the_account_it_uses_cancels_a_waiting_restart() {
        let (mut setup, pane, home) = working();
        setup.app.ask_restart(pane, Known::Claude, "personal").expect("it waits");

        let refused = setup.app.ask_restart(pane, Known::Claude, "work").expect_err("already on work");

        assert_eq!(refused, format!("pane {pane} already uses that account, so its waiting restart was cancelled"));
        assert_eq!(setup.app.restart_waiting(pane), None);
        turn_ends(&mut setup, &home);
        drive(&mut setup, later());
        let term = setup.app.projects.pane_by(pane).expect("the pane");
        assert_eq!((term.generation(), term.accounts().name(Known::Claude)), (0, Some("work")));
    }

    #[test]
    fn cancel_leaves_the_agent_where_it_was() {
        let (mut setup, pane, home) = working();
        setup.app.ask_restart(pane, Known::Claude, "personal").expect("it waits");
        let wide = Rect::new(0, 0, 160, 40);
        let rect = setup.app.tab().and_then(|t| t.shown(setup.app.layout(wide).pane).first().map(|(_, r)| *r));
        let text = notice(&setup.app, pane).expect("the notice");
        let (_, button) = ui::restart_notice(rect.expect("a pane"), &text).expect("the notice fits");

        click_in(&mut setup.app, button.as_position(), wide);

        assert_eq!(setup.app.restart_waiting(pane), None);
        assert_eq!(notice(&setup.app, pane), None);
        turn_ends(&mut setup, &home);
        drive(&mut setup, later());
        let term = setup.app.projects.pane_by(pane).expect("the pane");
        assert_eq!((term.generation(), term.accounts().name(Known::Claude)), (0, Some("work")));
    }

    #[test]
    fn choosing_accounts_or_profiles_leaves_a_waiting_restart_as_it_is() {
        let (mut setup, pane, _home) = working();
        setup.add("home");
        setup.app.ask_restart(pane, Known::Claude, "personal").expect("it waits");
        let commands = [
            serde_json::json!({"command": "account-use", "args": {"provider": "claude", "account": "home"}}),
            serde_json::json!({"command": "account-use", "args": {"provider": "claude", "account": "work"}}),
            serde_json::json!({"command": "profile-new", "args": {"name": "p", "accounts": {"claude": "home"}}}),
            serde_json::json!({"command": "profile-use", "args": {"name": "p"}}),
            serde_json::json!({"command": "profile-set", "args": {"name": "p", "provider": "claude", "account": "work"}}),
            serde_json::json!({"command": "profile-delete", "args": {"name": "p"}}),
        ];

        for command in commands {
            ask_ok(&mut setup.app, &command);
            assert_eq!(setup.app.restart_waiting(pane), Some((Known::Claude, "personal")), "{command}");
        }
    }

    #[test]
    fn removing_the_account_a_restart_waits_for_drops_it() {
        let (mut setup, pane, _home) = working();
        setup.app.ask_restart(pane, Known::Claude, "personal").expect("it waits");

        let remove =
            serde_json::json!({"command": "account-remove", "args": {"provider": "claude", "name": "personal"}});
        ask_ok(&mut setup.app, &remove);

        assert_eq!(setup.app.restart_waiting(pane), None);
    }

    fn ask_ok(app: &mut App, command: &serde_json::Value) {
        app.request(9, &command.to_string(), None, Instant::now());
        let answers = app.take_answers();
        assert!(answers.iter().all(|(_, text)| text.contains("\"ok\"")), "{answers:?}");
    }

    #[test]
    fn a_closed_pane_drops_its_waiting_restart() {
        let (mut setup, pane, _home) = working();
        setup.app.ask_restart(pane, Known::Claude, "personal").expect("it waits");

        setup.app.remove(pane);
        drive(&mut setup, later());

        assert_eq!(setup.app.restart_waiting(pane), None);
    }

    #[test]
    fn an_idle_agent_restarts_at_once() {
        let mut setup = Setup::new(FakeClaude::SILENT);
        let pane = setup.start_on("work");
        setup.add("personal");
        setup.runs();
        assert_eq!(setup.app.ask_restart(pane, Known::Claude, "personal"), Ok(Restart::Waiting), "just typed");
        setup.app.cancel_restart(pane);
        setup.untouched(pane);

        assert_eq!(setup.app.ask_restart(pane, Known::Claude, "personal"), Ok(Restart::Now));
        assert_eq!(setup.app.projects.pane_by(pane).map(Term::generation), Some(1));
        setup.claude.signal("quit");
    }
}

mod requests {
    use serde_json::{Value, json};

    use super::*;

    fn ask(app: &mut App, command: &Value) {
        let request = json!({"contract": 1, "command": command["command"], "args": command["args"]});
        app.request(9, &request.to_string(), None, Instant::now());
    }

    fn answer(app: &mut App) -> Value {
        let answers = app.take_answers();
        assert_eq!(answers.len(), 1, "{answers:?}");
        serde_json::from_str(&answers[0].1).expect("json")
    }

    fn restart(account: Option<&str>, panes: &[u64]) -> Value {
        json!({"command": "account-restart", "args": {"provider": "claude", "account": account, "panes": panes}})
    }

    fn pane_of(app: &mut App, pane: u64) -> Value {
        ask(app, &json!({"command": "status", "args": {}}));
        let report = answer(app);
        let panes = &report["ok"]["projects"][0]["workspaces"][0]["tabs"][0]["panes"];
        panes.as_array().expect("panes").iter().find(|p| p["id"] == pane).cloned().expect("the pane")
    }

    #[test]
    fn a_second_request_while_the_conversation_moves_joins_the_first() {
        let mut setup = Setup::new(FakeClaude::ANSWERING);
        let pane = setup.start_on("work");
        setup.add("personal");
        setup.known();
        setup.untouched(pane);

        ask(&mut setup.app, &restart(Some("personal"), &[pane]));
        let band = setup.app.pane_bands().get(&pane).cloned();
        assert_eq!(band, Some(ui::PaneBand::Resuming("restarting claude on personal".into())));
        ask(&mut setup.app, &restart(Some("personal"), &[pane]));
        setup.restarted(pane);

        let answers = setup.app.take_answers();
        assert_eq!(answers.len(), 2, "{answers:?}");
        for (_, text) in answers {
            let done: Value = serde_json::from_str(&text).expect("json");
            assert_eq!(done["ok"]["restarted"], json!([pane]), "{done}");
        }
        assert_eq!(setup.app.projects.pane_by(pane).map(Term::generation), Some(1));
        setup.claude.signal("quit");
    }

    #[test]
    fn a_request_to_another_account_while_the_conversation_moves_is_skipped() {
        let mut setup = Setup::new(FakeClaude::ANSWERING);
        let pane = setup.start_on("work");
        setup.add("personal");
        setup.add("home");
        setup.known();
        setup.untouched(pane);

        ask(&mut setup.app, &restart(Some("personal"), &[pane]));
        ask(&mut setup.app, &restart(Some("home"), &[pane]));
        let skipped = answer(&mut setup.app);
        assert_eq!(skipped["ok"]["skipped"][0]["pane"], json!(pane), "{skipped}");
        assert!(skipped["ok"]["skipped"][0]["why"].as_str().is_some_and(|why| why.contains("personal")), "{skipped}");
        setup.restarted(pane);

        assert_eq!(answer(&mut setup.app)["ok"]["restarted"], json!([pane]));
        let term = setup.app.projects.pane_by(pane).expect("the pane");
        assert_eq!(term.accounts().name(Known::Claude), Some("personal"));
        setup.claude.signal("quit");
    }

    #[test]
    fn the_panes_on_another_account_restart_and_the_answer_comes_once_their_conversation_moved() {
        let mut setup = Setup::new(FakeClaude::ANSWERING);
        let pane = setup.start_on("work");
        setup.add("personal");
        setup.known();
        setup.untouched(pane);
        setup.app.config.active_accounts.insert("claude".into(), "personal".into());
        assert_eq!(pane_of(&mut setup.app, pane).get("account_restart"), None);

        ask(&mut setup.app, &restart(None, &[]));
        assert_eq!(setup.app.take_answers(), Vec::new());
        setup.restarted(pane);

        let done = answer(&mut setup.app);
        let expected = json!({"provider": "claude", "account": "personal", "restarted": [pane], "waiting": [],
            "skipped": []});
        assert_eq!(done["ok"], expected);
        let term = setup.app.projects.pane_by(pane).expect("the pane");
        assert_eq!(term.accounts().name(Known::Claude), Some("personal"));
        setup.claude.signal("quit");
    }

    #[test]
    fn a_working_agent_is_answered_as_waiting_and_shows_it_in_status() {
        let (mut setup, pane, _home) = waiting::working();

        ask(&mut setup.app, &restart(Some("personal"), &[pane]));

        assert_eq!(answer(&mut setup.app)["ok"]["waiting"], json!([pane]));
        assert_eq!(pane_of(&mut setup.app, pane)["account_restart"], json!("waiting"));
    }

    #[test]
    fn a_named_shell_at_its_prompt_starts_again_and_one_running_a_program_is_skipped_with_why() {
        let (mut setup, pane) = Setup::shell_beside_personal();

        ask(&mut setup.app, &restart(Some("personal"), &[pane, pane]));

        let done = answer(&mut setup.app);
        assert_eq!(done["ok"]["restarted"], json!([pane]), "{done}");
        assert_eq!(done["ok"]["skipped"], json!([]), "{done}");
        let term = setup.app.projects.pane_by(pane).expect("the pane");
        assert_eq!((term.generation(), term.accounts().name(Known::Claude)), (1, Some("personal")));

        setup.runs_sleep(pane);
        ask(&mut setup.app, &restart(Some("work"), &[pane]));

        let done = answer(&mut setup.app);
        assert_eq!(done["ok"]["skipped"], json!([{"pane": pane, "why": "stop sleep first"}]), "{done}");
        assert_eq!(setup.app.projects.pane_by(pane).map(Term::generation), Some(1));
    }

    #[test]
    fn without_panes_an_agent_already_on_the_account_drops_its_waiting_restart() {
        let (mut setup, pane, _home) = waiting::working();
        setup.app.ask_restart(pane, Known::Claude, "personal").expect("it waits");

        ask(&mut setup.app, &restart(Some("work"), &[]));

        let done = answer(&mut setup.app);
        let why = format!("pane {pane} already uses that account, so its waiting restart was cancelled");
        assert_eq!(done["ok"]["skipped"], json!([{"pane": pane, "why": why}]), "{done}");
        assert_eq!(setup.app.restart_waiting(pane), None);
    }

    #[test]
    fn without_panes_a_shell_is_left_alone() {
        let (mut setup, pane) = Setup::shell_beside_personal();

        ask(&mut setup.app, &restart(Some("personal"), &[]));

        assert_eq!(answer(&mut setup.app)["ok"]["restarted"], json!([]));
        assert_eq!(setup.app.projects.pane_by(pane).map(Term::generation), Some(0));
    }

    #[test]
    fn unknown_panes_agents_and_accounts_are_refused() {
        let mut setup = Setup::new(FakeClaude::SILENT);
        setup.app.open_project(setup.project.path().to_path_buf(), AREA).expect("open the project");
        let code = |setup: &mut Setup, command: Value| {
            ask(&mut setup.app, &command);
            answer(&mut setup.app)["error"]["code"].clone()
        };

        assert_eq!(code(&mut setup, restart(None, &[u64::MAX])), json!("not-found"));
        assert_eq!(code(&mut setup, restart(Some("nobody"), &[])), json!("not-found"));
        let opencode = json!({"command": "account-restart", "args": {"provider": "opencode", "account": null,
            "panes": []}});
        assert_eq!(code(&mut setup, opencode), json!("malformed"));
    }
}
