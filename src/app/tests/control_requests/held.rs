use rstest::rstest;

use super::*;
use crate::agents::launch::Launch;

const SHELL: &str = if cfg!(windows) { "cmd.exe" } else { "/bin/sh" };
const OTHER: u64 = CLIENT + 1;

pub(super) fn asked_by_another_client(
    app: &mut App,
    args: &Value,
) -> (std::result::Result<Value, Refused>, Vec<String>) {
    let text = json!({"contract": 1, "command": "held", "args": args}).to_string();
    app.request(OTHER, &text, Some(AREA), Instant::now());
    let mut answers = app.take_answers();
    let mine = answers.iter().position(|(client, _)| *client == OTHER).expect("an answer at once");
    let (_, mine) = answers.remove(mine);
    (reply_of(&mine), answers.into_iter().map(|(_, text)| text).collect())
}

struct Bar {
    app: App,
    _rx: Receiver<AppEvent>,
    _project: TempDir,
    pane: u64,
    answered: Vec<String>,
}

impl Bar {
    fn new() -> Self {
        let project = TempDir::new();
        let (mut app, rx) = new_app_with_shell(SHELL, no_config());
        app.open_project(project.path().to_path_buf(), AREA).expect("open project");
        app.clients.set_typing_holds(Duration::from_secs(600));
        let pane = app.report(None).panes()[0].pane;
        app.projects.pane_by_mut(pane).expect("the pane").typed_text("mine");
        Self { app, _rx: rx, _project: project, pane, answered: Vec::new() }
    }

    fn hold(&mut self, from: Option<u64>, text: &str) {
        let send = wire::SendText { pane: Some(self.pane), text: Some(text.into()), ..wire::SendText::default() };
        self.hold_send(from, send);
    }

    fn hold_entered(&mut self, from: Option<u64>, text: &str) {
        let send =
            wire::SendText { pane: Some(self.pane), text: Some(text.into()), enter: true, ..wire::SendText::default() };
        self.hold_send(from, send);
    }

    fn hold_send(&mut self, from: Option<u64>, send: wire::SendText) {
        ask(&mut self.app, from, Command::Send(send));
        assert_eq!(self.app.take_answers(), Vec::new(), "the send is held");
    }

    fn held(&mut self, args: &Value) -> std::result::Result<Value, Refused> {
        let (reply, answered) = asked_by_another_client(&mut self.app, args);
        self.answered.extend(answered);
        reply
    }

    fn press(&mut self, action: &str) -> std::result::Result<Value, Refused> {
        self.held(&json!({"pane": self.pane, "action": action}))
    }

    fn fields(&self) -> (u32, Option<u64>, bool) {
        let report = self.app.report(None);
        let pane = &report.projects[0].workspaces[0].tabs[0].panes[0];
        (pane.held_sends, pane.held_from, pane.held_queued)
    }

    fn draft(&self) -> bool {
        self.app.report(None).panes()[0].draft
    }

    fn words(&self) -> Option<String> {
        self.app.held_label(self.pane).map(|(words, _)| words)
    }
}

#[test]
fn send_now_sends_the_first_message_and_says_what_is_left() {
    let mut bar = Bar::new();
    bar.hold(Some(41), "first");
    bar.hold(Some(42), "second");

    let said = bar.press("send-now").expect("sent");

    assert_eq!(
        said,
        json!({"pane": bar.pane, "held_sends": 1, "held_from": 42, "held_queued": false, "after_draft": true})
    );
    assert_eq!(bar.answered.len(), 1, "the first message's sender has its answer");
    assert!(reply_of(&bar.answered[0]).is_ok(), "a send without Enter gets its usual answer: {:?}", bar.answered);
    assert_eq!(bar.fields(), (1, Some(42), false));
}

#[test]
fn send_now_of_the_last_message_leaves_nothing_held() {
    let mut bar = Bar::new();
    bar.hold(None, "theirs");

    let said = bar.press("send-now").expect("sent");

    assert_eq!(
        said,
        json!({"pane": bar.pane, "held_sends": 0, "held_from": null, "held_queued": false, "after_draft": true})
    );
    assert_eq!(bar.fields(), (0, None, false));
    assert_eq!(bar.words(), None);
}

#[test]
fn send_now_over_a_draft_pastes_the_message_and_leaves_its_enter_to_the_person() {
    let mut bar = Bar::new();
    bar.hold_entered(Some(41), "theirs");
    bar.hold_entered(Some(42), "later");
    let before = bar.app.projects.pane_by(bar.pane).expect("the pane").typing.at.expect("typed");
    wait_until("time moves on", || Instant::now() > before);

    let said = bar.press("send-now").expect("pasted");

    let expected =
        json!({"pane": bar.pane, "held_sends": 1, "held_from": 42, "held_queued": false, "after_draft": true});
    assert_eq!(said, expected);
    assert_eq!(bar.answered.len(), 1);
    let told = &bar.answered[0];
    assert!(told.contains(&format!("the person in pane {} had a draft", bar.pane)), "{told}");
    assert!(!bar.app.sending(bar.pane), "no Enter is queued");
    assert!(bar.draft(), "the draft holds");
    let after = bar.app.projects.pane_by(bar.pane).expect("the pane").typing.at.expect("typed");
    assert!(after > before, "the paste counts as the person's input");
    assert_eq!(bar.fields().0, 1);
}

#[test]
fn send_now_without_a_draft_still_presses_enter() {
    let mut bar = Bar::new();
    bar.hold_entered(None, "theirs");
    let ctrl_u = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
    bar.app.projects.pane_by_mut(bar.pane).expect("the pane").typed_key(ctrl_u);

    let said = bar.press("send-now").expect("sent");

    assert_eq!(said["after_draft"], json!(false));
    assert!(bar.app.sending(bar.pane), "its Enter is queued");
}

#[test]
fn the_enter_of_a_send_ends_the_draft_it_took_with_it() {
    let mut bar = Bar::new();
    let send = wire::SendText {
        pane: Some(bar.pane),
        text: Some("now".into()),
        enter: true,
        force: true,
        ..wire::SendText::default()
    };
    ask(&mut bar.app, None, Command::Send(send));
    assert!(bar.draft());

    bar.app.refresh(Instant::now() + Duration::from_secs(4));

    let answers = bar.app.take_answers();
    assert_eq!(answers.len(), 1, "{answers:?}");
    assert!(reply_of(&answers[0].1).is_ok(), "{answers:?}");
    assert!(!bar.draft(), "the Enter took the line");
}

#[test]
fn wait_turn_only_says_the_person_saw_it() {
    let mut bar = Bar::new();
    bar.hold(None, "theirs");

    let said = bar.press("wait-turn").expect("acknowledged");
    let again = bar.press("wait-turn").expect("acknowledged again");

    assert_eq!(
        said,
        json!({"pane": bar.pane, "held_sends": 1, "held_from": null, "held_queued": true, "after_draft": false})
    );
    assert_eq!(again, said);
    assert!(bar.answered.is_empty(), "nothing went out: {:?}", bar.answered);
    assert_eq!(bar.fields(), (1, None, true));
}

#[test]
fn a_message_held_after_wait_turn_is_not_covered_by_it() {
    let mut bar = Bar::new();
    bar.hold(None, "first");
    bar.press("wait-turn").expect("acknowledged");

    bar.hold(None, "second");

    assert_eq!(bar.fields(), (2, None, false));
    bar.press("wait-turn").expect("acknowledged");
    assert_eq!(bar.fields(), (2, None, true));
}

#[test]
fn the_fields_say_what_the_bar_of_the_window_says() {
    let mut bar = Bar::new();
    bar.hold(Some(41), "first");
    assert_eq!(bar.fields(), (1, Some(41), false));
    assert_eq!(bar.words().as_deref(), Some("1 message waiting from pane 41 · goes after yours"));

    bar.hold(None, "second");
    assert_eq!(bar.fields(), (2, Some(41), false));
    assert_eq!(bar.words().as_deref(), Some("2 messages waiting, the first from pane 41 · goes after yours"));

    bar.press("wait-turn").expect("acknowledged");
    assert_eq!(bar.fields(), (2, Some(41), true));
    assert_eq!(bar.words().as_deref(), Some("2 queued: the first goes right after yours"));

    bar.press("send-now").expect("sent");
    assert_eq!(bar.fields(), (1, None, true));
    assert_eq!(bar.words().as_deref(), Some("queued: goes right after yours"));
}

#[test]
fn a_tab_stands_for_its_pane() {
    let mut bar = Bar::new();
    bar.hold(None, "theirs");
    let tab = bar.app.report(None).panes()[0].tab;

    let said = bar.held(&json!({"tab": tab, "action": "wait-turn"})).expect("acknowledged");

    assert_eq!(
        said,
        json!({"pane": bar.pane, "held_sends": 1, "held_from": null, "held_queued": true, "after_draft": false})
    );
}

#[rstest]
#[case::send_now("send-now")]
#[case::wait_turn("wait-turn")]
fn a_pane_with_nothing_held_is_refused_in_words(#[case] action: &str) {
    let mut bar = Bar::new();

    let refusal = bar.press(action).expect_err("nothing is held");

    assert_eq!(refusal.code, "conflict");
    let expected = format!("no message is held in pane {}", bar.pane);
    assert!(refusal.message.starts_with(&expected), "{}", refusal.message);
}

#[test]
fn send_now_leaves_a_pane_that_is_resuming_its_agent_alone() {
    let mut bar = Bar::new();
    bar.hold(None, "theirs");
    let (now, later) = (Instant::now(), Instant::now() + Duration::from_secs(600));
    let restart = Launch::restart(bar.pane, "agent".into(), "restarting".into(), now, later);
    bar.app.agents.launches.add(restart);

    let refusal = bar.press("send-now").expect_err("the window has no bar there");

    assert_eq!(refusal.code, "conflict");
    assert!(refusal.message.contains("restarting or resuming"), "{}", refusal.message);
    assert_eq!((bar.fields(), bar.answered.len()), ((1, None, false), 0));
    bar.press("wait-turn").expect("it only says the person saw it");
    assert_eq!(bar.fields(), (1, None, true));
}

#[test]
fn wait_turn_changes_the_fingerprint_a_refresh_compares() {
    let mut bar = Bar::new();
    bar.hold(None, "theirs");
    let before = bar.app.refreshed();

    bar.press("wait-turn").expect("acknowledged");

    assert_ne!(bar.app.refreshed(), before);
}

#[test]
fn another_first_sender_changes_the_fingerprint_with_as_many_messages() {
    let mut bar = Bar::new();
    bar.hold(Some(41), "first");
    bar.hold(Some(42), "second");
    let before = bar.app.refreshed();

    bar.press("send-now").expect("sent");
    bar.hold(Some(43), "third");

    assert_eq!(bar.fields(), (2, Some(42), false));
    assert_ne!(bar.app.refreshed(), before);
}

#[rstest]
#[case::no_pane_of_that_id(json!({"pane": 4242, "action": "send-now"}), "not-found")]
#[case::no_tab_of_that_id(json!({"tab": 4242, "action": "wait-turn"}), "not-found")]
#[case::no_pane_here(json!({"action": "send-now"}), "failed")]
#[case::no_button(json!({"pane": 1}), "malformed")]
#[case::another_button(json!({"pane": 1, "action": "discard"}), "malformed")]
#[case::another_field(json!({"pane": 1, "action": "send-now", "extra": true}), "unknown-field")]
fn a_request_that_names_nothing_it_can_act_on_is_refused(#[case] args: Value, #[case] code: &str) {
    let (mut app, _rx) = empty_app();

    assert_eq!(refused(&mut app, "held", &args).code, code, "{args}");
}
