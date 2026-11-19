#![cfg(unix)]

use std::path::PathBuf;

use super::*;
use crate::agents::activity::Activity;
use crate::agents::headless::Access;
use crate::app::control::boundary::fronts_agent;
use crate::app::control::boundary::{HEAD, NO_KEYS, OWN_ONLY};
use crate::app::tests::bots::{install_bot, root_pid};
use crate::test_util::fake_program;

const OTHER: u64 = CLIENT + 1;

struct Bounded {
    app: App,
    rx: Receiver<AppEvent>,
    shell: u64,
    agent: u64,
    helper: PathBuf,
    claude: PathBuf,
    _folders: [TempDir; 3],
}

fn bounded() -> Bounded {
    let (mut app, rx) = app();
    let shell = first(&app);
    let (bots, bin, claude) = install_bot(&mut app, "", Access::Cornercase);
    let pasted = TempDir::new();
    app.set_pasted_dir(&pasted.path().join("pasted"));
    let helper = fake_program(bin.path(), "helper");
    std::fs::write(helper.with_extension("hold"), "").expect("hold");
    app.config.agent_commands.insert("helper".into(), helper.display().to_string());
    ask(&mut app, None, new_tab(Some(&helper.display().to_string())));
    let agent = done(answered(&mut app, &rx, "the agent's tab")).ids.pane.expect("the agent's pane");
    refreshing(&mut app, &rx, "the helper is in front", |app| in_front(app, agent));
    let mut bounded = Bounded { app, rx, shell, agent, helper, claude, _folders: [bots, bin, pasted] };
    bounded.connect_as_the_bot();
    bounded
}

fn in_front(app: &App, pane: u64) -> bool {
    fronts_agent(&app.config, app.projects.pane_by(pane).expect("the pane"))
}

impl Bounded {
    fn connect_as_the_bot(&mut self) {
        std::fs::write(self.claude.with_extension("hold"), "").expect("hold");
        self.app.bot_say("prs", "go", false).expect("say");
        wait_until("the bot's agent runs", || root_pid(&self.claude).is_some());
        self.app.client_connected(CLIENT, root_pid(&self.claude));
    }

    fn as_you(&mut self) {
        self.app.clients.bots.forget(CLIENT, std::iter::empty());
    }

    fn refused(&mut self, command: Command) -> String {
        let message = error(now(&mut self.app, None, command));
        assert!(message.starts_with(HEAD), "{message}");
        message
    }

    fn closed(&mut self, command: Command) {
        assert_eq!(error(now(&mut self.app, None, command)), NO_KEYS);
    }

    fn refused_for(&mut self, command: Command, why: &str) {
        let message = error(now(&mut self.app, None, command));
        assert!(message.starts_with(why), "{message}");
    }

    fn waits(&mut self) {
        let term = self.app.projects.pane_by_mut(self.agent).expect("the pane");
        term.agent.update(Some(Activity::Waiting), false, Instant::now());
        assert_eq!(term.agent.status(), Some(activity::Status::Waiting));
    }

    fn screen(&self, pane: u64) -> String {
        pane_screen(&self.app, pane)
    }

    fn release_helper(&self) {
        std::fs::remove_file(self.helper.with_extension("hold")).expect("release the helper");
    }

    fn gone_from_front(&mut self) {
        self.release_helper();
        let (app, agent) = (&self.app, self.agent);
        wait_until("the agent's shell is in front", || !in_front(app, agent));
    }

    fn leaves_the_agent(&mut self) {
        self.release_helper();
        let shell_in_front = |app: &App| !in_front(app, self.agent);
        let (app, rx) = (&mut self.app, &self.rx);
        refreshing(app, rx, "the agent's shell is in front", |app| shell_in_front(app));
    }
}

fn text(pane: u64, text: &str, enter: bool) -> Command {
    send_text(pane, text, enter, false)
}

fn send_text_forced(pane: u64, text: &str) -> Command {
    Command::Send(wire::SendText {
        pane: Some(pane),
        text: Some(text.into()),
        enter: true,
        force: true,
        ..wire::SendText::default()
    })
}

fn input(pane: u64) -> Command {
    let events = vec![crate::control::terminal::InputEvent::Text("typed-by-bot".into())];
    Command::Input(wire::Input { pane: Some(pane), events, ..wire::Input::default() })
}

#[test]
fn a_bot_cannot_type_into_a_shell_pane() {
    let mut b = bounded();
    let (shell, agent) = (b.shell, b.agent);

    let sent = b.refused(text(shell, "echo typed-by-bot", true));
    assert!(sent.contains(&format!("pane {shell} runs none")), "{sent}");
    b.closed(press_keys(shell, &["enter"]));
    b.closed(input(shell));
    b.refused(Command::PasteFile(wire::PasteFile { pane: Some(shell), name: "a.txt".into(), data: String::new() }));
    let todo = wire::TodoSend { id: 1, pane: Some(shell), tab: None, force: false };
    b.refused(Command::TodoSend(todo));
    b.refused(new_tab(Some("echo x")));
    b.refused(Command::Split(wire::Split { pane: Some(agent), command: Some("ls".into()), ..wire::Split::default() }));

    let tab = b.app.projects.tab(b.app.projects.tab_of_pane(shell).expect("the shell's tab")).id;
    let by_tab = wire::SendText { tab: Some(tab), text: Some("typed-by-bot".into()), ..wire::SendText::default() };
    b.refused(Command::Send(by_tab));
    assert!(!b.screen(shell).contains("typed-by-bot"), "{}", b.screen(shell));
}

#[test]
fn a_bot_can_write_a_prompt_to_an_agent_pane_and_start_a_plain_shell() {
    let mut b = bounded();
    let agent = b.agent;

    value(&mut b.app, None, text(agent, "hello", false));
    value(&mut b.app, None, new_tab(None));
}

#[test]
fn a_bot_cannot_press_keys_or_type_into_an_agent_pane() {
    let mut b = bounded();
    let agent = b.agent;

    b.closed(press_keys(agent, &["esc"]));
    b.closed(input(agent));
    assert!(!b.screen(agent).contains("typed-by-bot"), "{}", b.screen(agent));

    b.as_you();
    value(&mut b.app, None, press_keys(agent, &["esc"]));
}

#[test]
fn a_bot_cannot_send_an_agents_command_or_its_shell() {
    let mut b = bounded();
    let agent = b.agent;
    let command = "a bot writes prompts to an agent, not its commands";

    b.refused_for(text(agent, "/exit", true), command);
    b.refused_for(text(agent, "  !echo typed-by-bot", true), command);
    b.refused_for(
        Command::Send(wire::SendText { pane: Some(agent), enter: true, ..wire::SendText::default() }),
        "a bot's send needs a text",
    );

    assert!(!b.screen(agent).contains("typed-by-bot"), "{}", b.screen(agent));
    assert!(!b.screen(agent).contains("/exit"), "{}", b.screen(agent));
}

#[test]
fn a_bot_cannot_answer_a_waiting_agent_even_with_force() {
    let mut b = bounded();
    let agent = b.agent;
    b.waits();
    let asks = format!("{HEAD}, and the agent in pane {agent} asks something");

    b.refused_for(text(agent, "yes", true), &asks);
    b.refused_for(send_text_forced(agent, "yes"), &asks);
    b.refused_for(Command::TodoSend(wire::TodoSend { id: 1, pane: Some(agent), tab: None, force: true }), &asks);
    b.refused_for(
        Command::Held(wire::HeldRequest { pane: Some(agent), tab: None, action: wire::HeldAction::SendNow }),
        &asks,
    );
    b.refused_for(
        Command::PasteFile(wire::PasteFile { pane: Some(agent), name: "a.txt".into(), data: "eA==".into() }),
        &asks,
    );

    b.as_you();
    value(&mut b.app, None, press_keys(agent, &["esc"]));
}

#[test]
fn a_bots_enter_is_not_pressed_once_its_agent_asks_something() {
    let mut b = bounded();
    let agent = b.agent;
    ask(&mut b.app, None, text(agent, "go on", true));
    b.waits();

    let message = error(answered(&mut b.app, &b.rx, "the bot is answered"));

    assert!(message.starts_with(&format!("{HEAD}, and the agent in pane {agent} asks something")), "{message}");
    assert!(pane(&b.app, agent).submitted.is_none(), "no Enter was pressed");
}

#[test]
fn a_bot_closes_only_the_panes_and_tabs_it_started() {
    let mut b = bounded();
    let (shell, agent) = (b.shell, b.agent);
    let shells_tab = b.app.projects.tab(b.app.projects.tab_of_pane(shell).expect("the shell's tab")).id;
    let close = |item| Command::Close(wire::Close { item, remove_worktree: false, force: false, unlock: false });

    b.refused_for(close(Item::Pane(shell)), &format!("{OWN_ONLY}; pane-{shell} was not started by this bot"));
    b.refused_for(close(Item::Tab(shells_tab)), OWN_ONLY);
    let workspace = b.app.report(None).panes()[0].workspace;
    b.refused_for(close(Item::Workspace(workspace)), OWN_ONLY);
    assert!(b.app.projects.pane_by(shell).is_some());

    b.app.projects.pane_by_mut(agent).expect("the agent's pane").bot = Some("prs".into());
    value(&mut b.app, None, close(Item::Pane(agent)));
}

#[test]
fn a_bot_cannot_type_into_an_agent_that_went_back_to_its_shell() {
    let mut b = bounded();
    let agent = b.agent;
    b.leaves_the_agent();

    b.refused(text(agent, "echo typed-by-bot", true));
    assert!(!b.screen(agent).contains("typed-by-bot"), "{}", b.screen(agent));
}

#[test]
fn you_still_type_into_a_shell_pane() {
    let mut b = bounded();
    let shell = b.shell;
    b.as_you();

    ask(&mut b.app, None, text(shell, "echo typed-by-you", true));
    done(answered(&mut b.app, &b.rx, "the send is answered"));

    let (app, rx) = (&mut b.app, &b.rx);
    refreshing(app, rx, "the command runs", |app| pane_screen(app, shell).contains("typed-by-you"));
}

#[test]
fn another_connection_is_not_a_bots() {
    let mut b = bounded();
    let shell = b.shell;
    let json = json!({"contract": 1, "command": "send", "args": {"pane": shell, "text": "typed-by-other"}}).to_string();
    b.app.request(OTHER, &json, Some(AREA), Instant::now());
    let answers = b.app.take_answers();
    let (_, reply) = answers.into_iter().find(|(client, _)| *client == OTHER).expect("an answer");
    assert!(reply_of(&reply).is_ok(), "{reply}");
}

#[test]
fn a_bots_held_send_is_checked_again_when_it_goes_out() {
    let mut b = bounded();
    let agent = b.agent;
    b.app.clients.set_typing_holds(Duration::from_secs(600));
    b.app.projects.pane_by_mut(agent).expect("the pane").typed_text("mine");
    ask(&mut b.app, None, text(agent, "echo typed-by-bot", true));
    assert_eq!(b.app.take_answers(), Vec::new(), "the send is held");
    b.leaves_the_agent();

    let (_, answered) = held::asked_by_another_client(&mut b.app, &json!({"pane": agent, "action": "send-now"}));
    let reply = answered.into_iter().next().expect("the bot's answer");

    let refusal = error(serde_json::from_str(&reply).expect("a response"));
    assert!(refusal.starts_with(HEAD), "{refusal}");
    assert!(!b.screen(agent).contains("typed-by-bot"), "{}", b.screen(agent));
}

#[test]
fn a_bots_enter_is_not_pressed_once_its_agent_is_gone() {
    let mut b = bounded();
    let agent = b.agent;
    ask(&mut b.app, None, text(agent, "echo pwned", true));
    b.gone_from_front();

    let message = error(answered(&mut b.app, &b.rx, "the bot is answered"));

    assert!(message.starts_with(HEAD), "{message}");
    assert!(b.screen(agent).lines().all(|line| !line.trim_end().ends_with("pwned") || line.contains("echo pwned")));
    assert!(!b.screen(agent).lines().any(|line| line.trim() == "pwned"), "{}", b.screen(agent));
}

#[test]
fn a_bot_cannot_type_into_a_pane_whose_agent_went_back_to_its_shell_while_status_still_shows_it() {
    let mut b = bounded();
    let agent = b.agent;
    b.leaves_the_agent();
    b.app.projects.pane_by_mut(agent).expect("the pane").agent.update(Some(Activity::Done), false, Instant::now());
    assert!(pane(&b.app, agent).agent.status().is_some(), "the status outlives the agent");

    b.refused(text(agent, "echo typed-by-bot", true));
}

#[test]
fn a_bots_pasted_file_is_checked_again_when_it_is_stored() {
    let mut b = bounded();
    let agent = b.agent;
    ask(
        &mut b.app,
        None,
        Command::PasteFile(wire::PasteFile { pane: Some(agent), name: "a.txt".into(), data: "eA==".into() }),
    );
    b.gone_from_front();

    let message = error(answered(&mut b.app, &b.rx, "the paste is answered"));

    assert!(message.starts_with(HEAD), "{message}");
    assert!(!b.screen(agent).contains("pasted"), "{}", b.screen(agent));
}

#[test]
fn a_bots_enter_is_not_pressed_when_its_client_left_before_it() {
    let mut b = bounded();
    let agent = b.agent;
    ask(&mut b.app, None, text(agent, "echo pwned", true));
    b.app.forget(CLIENT);
    b.gone_from_front();

    let (app, rx) = (&mut b.app, &b.rx);
    refreshing(app, rx, "the launch is over", |app| app.agents.launches.of_pane(agent).next().is_none());
    value(&mut b.app, None, press_keys(agent, &["ctrl+u"]));
    ask(&mut b.app, None, text(agent, "echo after", true));
    done(answered(&mut b.app, &b.rx, "the send is answered"));
    let (app, rx) = (&mut b.app, &b.rx);
    refreshing(app, rx, "the shell ran what came after", |app| {
        pane_screen(app, agent).lines().any(|l| l.trim() == "after")
    });

    assert!(!b.screen(agent).lines().any(|line| line.trim() == "pwned"), "{}", b.screen(agent));
}
