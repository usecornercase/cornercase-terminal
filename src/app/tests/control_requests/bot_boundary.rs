#![cfg(unix)]

use std::path::PathBuf;

use super::*;
use crate::agents::headless::Access;
use crate::app::control::boundary::HEAD;
use crate::app::tests::bots::install_bot;
use crate::test_util::fake_program;

const OTHER: u64 = CLIENT + 1;

struct Bounded {
    app: App,
    rx: Receiver<AppEvent>,
    shell: u64,
    agent: u64,
    helper: PathBuf,
    claude: PathBuf,
    _folders: [TempDir; 2],
}

fn bounded() -> Bounded {
    let (mut app, rx) = app();
    let shell = first(&app);
    let (bots, bin, claude) = install_bot(&mut app, "", Access::Cornercase);
    let helper = fake_program(bin.path(), "helper");
    std::fs::write(helper.with_extension("hold"), "").expect("hold");
    app.config.agent_commands.insert("helper".into(), helper.display().to_string());
    ask(&mut app, None, new_tab(Some(&helper.display().to_string())));
    let agent = done(answered(&mut app, &rx, "the agent's tab")).ids.pane.expect("the agent's pane");
    refreshing(&mut app, &rx, "the helper is in front", |app| in_front(app, agent));
    let mut bounded = Bounded { app, rx, shell, agent, helper, claude, _folders: [bots, bin] };
    bounded.connect_as_the_bot();
    bounded
}

fn in_front(app: &App, pane: u64) -> bool {
    crate::agents::detect(&app.config, &app.projects.pane_by(pane).expect("the pane").foreground_args()).is_some()
}

impl Bounded {
    fn connect_as_the_bot(&mut self) {
        std::fs::write(self.claude.with_extension("hold"), "").expect("hold");
        self.app.bot_say("prs", "go", false).expect("say");
        let claude = self.claude.with_extension("pid");
        wait_until("the bot's agent runs", || std::fs::read_to_string(&claude).is_ok_and(|pids| !pids.is_empty()));
        let pids = std::fs::read_to_string(&claude).expect("the agent's pid");
        let root = pids.lines().next().and_then(|pid| pid.parse().ok()).expect("a pid");
        self.app.client_connected(CLIENT, Some(root));
    }

    fn as_you(&mut self) {
        self.app.clients.bots.remove(&CLIENT);
    }

    fn refused(&mut self, command: Command) -> String {
        let message = error(now(&mut self.app, None, command));
        assert!(message.starts_with(HEAD), "{message}");
        message
    }

    fn screen(&self, pane: u64) -> String {
        pane_screen(&self.app, pane)
    }

    fn leaves_the_agent(&mut self) {
        std::fs::remove_file(self.helper.with_extension("hold")).expect("release the helper");
        let shell_in_front = |app: &App| !in_front(app, self.agent);
        let (app, rx) = (&mut self.app, &self.rx);
        refreshing(app, rx, "the agent's shell is in front", |app| shell_in_front(app));
    }
}

fn text(pane: u64, text: &str, enter: bool) -> Command {
    send_text(pane, text, enter, false)
}

fn keys(pane: u64, keys: &[&str]) -> Command {
    Command::Keys(wire::Keys { pane: Some(pane), keys: keys.iter().map(|key| (*key).to_string()).collect(), tab: None })
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
    b.refused(keys(shell, &["enter"]));
    b.refused(input(shell));
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
fn a_bot_can_type_into_an_agent_pane_and_start_a_plain_shell() {
    let mut b = bounded();
    let agent = b.agent;

    value(&mut b.app, None, text(agent, "hello", false));
    value(&mut b.app, None, keys(agent, &["esc"]));
    value(&mut b.app, None, new_tab(None));
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

    let held = json!({"pane": agent, "action": "send-now"});
    let envelope = json!({"contract": 1, "command": "held", "args": held}).to_string();
    b.app.request(OTHER, &envelope, Some(AREA), Instant::now());
    let answers = b.app.take_answers();
    let (_, reply) = answers.into_iter().find(|(client, _)| *client == CLIENT).expect("the bot's answer");

    let refusal = error(serde_json::from_str(&reply).expect("a response"));
    assert!(refusal.starts_with(HEAD), "{refusal}");
    assert!(!b.screen(agent).contains("typed-by-bot"), "{}", b.screen(agent));
}

#[test]
fn a_bots_enter_is_not_pressed_once_its_agent_is_gone() {
    let mut b = bounded();
    let agent = b.agent;
    ask(&mut b.app, None, text(agent, "echo pwned", true));
    std::fs::remove_file(b.helper.with_extension("hold")).expect("release the helper");

    let message = error(answered(&mut b.app, &b.rx, "the bot is answered"));

    assert!(message.starts_with(HEAD), "{message}");
    assert!(b.screen(agent).lines().all(|line| !line.trim_end().ends_with("pwned") || line.contains("echo pwned")));
    assert!(!b.screen(agent).lines().any(|line| line.trim() == "pwned"), "{}", b.screen(agent));
}
