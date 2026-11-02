use super::*;
use crate::agents::claude::fake::FakeClaude;
use crate::app::context_dialog::REFRESH;
use crate::control::ContextInfo;

const ANSWER: &str = crate::test_util::fixture!("claude/2.1.316/context-usage.jsonl");
const REPLY: &str = r#"{"type":"assistant","timestamp":"2026-10-08T19:56:37.241Z","message":{"id":"m1","model":"claude-opus-5-5","content":[{"type":"text","text":"Fixed."}],"usage":{"input_tokens":2,"cache_creation_input_tokens":15655,"cache_read_input_tokens":149954}}}"#;

struct Probed {
    agent: Agent,
    probe: PathBuf,
    _bin: TempDir,
    id: u64,
}

impl Probed {
    fn new() -> Self {
        let mut agent = Agent::new();
        let bin = TempDir::new();
        let (probe, wrapper) = (bin.path().join("probe"), bin.path().join("claude"));
        write_executable(&probe, FakeClaude::PROBE);
        std::fs::write(format!("{}.answer", probe.display()), ANSWER).expect("the probe's answer");
        let script = format!(
            "#!/bin/sh\ncase \"$1\" in -p) exec '{}' \"$@\" ;; esac\nexec '{}' \"$@\"\n",
            probe.display(),
            agent.fake.script().display()
        );
        write_executable(&wrapper, &script);
        agent.app.config.agent_commands.insert("claude".into(), wrapper.display().to_string());
        let id = agent.start(None);
        agent.transcript(id, &[r#"{"type":"user","message":{"role":"user","content":"fix"}}"#, REPLY]);
        agent.refreshing("the conversation is known", |a| pane(a, id).context.conversation().is_some());
        Self { agent, probe, _bin: bin, id }
    }

    fn runs(&self) -> Vec<String> {
        std::fs::read_to_string(format!("{}.args", self.probe.display()))
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    fn read(&mut self, answers: u64) {
        let id = self.id;
        self.agent.until("the probe answers", move |a| {
            a.agents.contexts.reads.get(&id).is_some_and(|r| r.probe.answers() >= answers)
        });
    }

    fn open(&mut self) {
        self.agent.app.open_context(self.id);
        self.read(1);
    }

    fn shown(&self) -> ui::Update {
        match overlay_shown(&self.agent.app, AREA) {
            ui::Overlay::Update(update) => update,
            _ => panic!("the context dialog is shown"),
        }
    }

    fn text(&self) -> String {
        let view = self.shown();
        let notes: String = view.notes.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n");
        format!("{}\n{notes}", view.message)
    }
}

fn info(response: Response) -> ContextInfo {
    match response {
        Response::Ok(value) => serde_json::from_value(value).expect("a context answer"),
        Response::Error(message) => panic!("context was refused: {message}"),
    }
}

fn context(pane: u64, cached: bool) -> Command {
    Command::Context(crate::control::ContextRead { pane: Some(pane), tab: None, cached })
}

fn assert_probe_flags(p: &Probed) {
    let runs = p.runs();
    assert_eq!(runs.len(), 1, "{runs:?}");
    let folder = pane(&p.agent.app, p.id).context.folder().expect("a folder").display().to_string();
    for wanted in
        ["--resume ", "--model claude-opus-5-5", "--settings {\"disableAllHooks\":true}", &format!("|{folder}|")]
    {
        assert!(runs[0].contains(wanted), "{wanted}: {runs:?}");
    }
}

#[test]
fn the_dialog_reads_the_conversation_with_the_panes_model_and_folder() {
    let mut p = Probed::new();
    p.agent.app.open_context(p.id);
    assert!(matches!(p.shown().note, Some(ui::Note::Busy("reading…"))));
    p.read(1);

    assert_probe_flags(&p);
    let text = p.text();
    assert!(text.contains("251.9k of 1M tokens · 25%"), "{text}");
    assert!(text.contains("Memory files"), "{text}");
    assert!(matches!(p.shown().note, Some(ui::Note::Info(_))));
    assert!(p.agent.app.agents.contexts.reads[&p.id].probe.last().is_some());
}

#[test]
fn a_probe_ends_by_closing_its_input_and_not_by_a_kill() {
    let mut p = Probed::new();
    p.open();
    wait_until("the probe reaches its end", || Path::new(&format!("{}.done", p.probe.display())).exists());
}

#[test]
fn refresh_reads_again_and_a_failure_keeps_the_old_numbers() {
    let mut p = Probed::new();
    p.open();
    std::fs::write(format!("{}.fail", p.probe.display()), "").expect("make the probe fail");
    let [refresh, _] = ui::update_buttons(AREA, REFRESH, "done");

    click(&mut p.agent.app, Position::new(refresh.x + 1, refresh.y));
    p.read(2);

    assert_eq!(p.runs().len(), 2);
    let text = p.text();
    assert!(text.contains("251.9k of 1M tokens"), "{text}");
    assert!(
        matches!(p.shown().note, Some(ui::Note::Error(ref e)) if e == "could not read the context: claude exited without answering"),
        "{:?}",
        p.shown().note.map(|_| ())
    );
}

#[test]
fn done_and_a_click_outside_close_the_dialog() {
    let mut p = Probed::new();
    p.open();
    let [_, done] = ui::update_buttons(AREA, REFRESH, "done");
    click(&mut p.agent.app, Position::new(done.x + 1, done.y));
    assert!(p.agent.app.overlay.is_none());
    p.open();
    click(&mut p.agent.app, Position::new(0, 0));
    assert!(p.agent.app.overlay.is_none());
}

#[test]
fn the_dialog_closes_with_its_pane() {
    let mut p = Probed::new();
    p.open();
    ask(
        &mut p.agent.app,
        None,
        Command::Close(wire::Close { item: Item::Pane(p.id), remove_worktree: false, force: false, unlock: false }),
    );
    p.agent.refreshing("the dialog goes", |a| a.overlay.is_none());
}

#[test]
fn a_request_answers_with_the_breakdown_and_shares_the_dialogs_probe() {
    let mut p = Probed::new();
    p.agent.app.open_context(p.id);
    ask(&mut p.agent.app, None, context(p.id, false));
    let got = info(p.agent.answered("the context is read"));

    assert_eq!(got.pane, p.id);
    assert_eq!(got.breakdown.as_ref().map(|b| b.total_tokens), Some(251_853));
    assert_eq!((got.agent.as_deref(), got.title.as_deref()), (Some("claude"), Some("Claude Code")));
    assert_eq!(p.runs().len(), 1, "one probe for the dialog and the request");
}

#[test]
fn cached_answers_at_once_and_starts_nothing() {
    let mut p = Probed::new();
    let got = info(now(&mut p.agent.app, None, context(p.id, true)));
    assert!(got.breakdown.is_none() && !got.loading);
    assert_eq!(p.runs().len(), 0);
    p.open();
    let got = info(now(&mut p.agent.app, None, context(p.id, true)));
    assert_eq!(got.breakdown.map(|b| b.max_tokens), Some(1_000_000));
    assert!(got.age.is_some() && got.read_at.is_some());
}

#[test]
fn a_failing_probe_is_text_beside_the_previous_numbers() {
    let mut p = Probed::new();
    p.open();
    std::fs::write(format!("{}.fail", p.probe.display()), "").expect("make the probe fail");
    ask(&mut p.agent.app, None, context(p.id, false));
    let got = info(p.agent.answered("the failure is answered"));
    assert_eq!(got.error.as_deref(), Some("claude exited without answering"));
    assert!(got.breakdown.is_some());
}

#[test]
fn a_shell_pane_and_an_unknown_pane_are_refused() {
    let (mut app, _rx, _dirs) = app_with(1);
    let shell = app.projects[0].workspaces[0].tabs[0].panes[0].id;
    let refused_shell = refused(&mut app, "context", &json!({"pane": shell}));
    assert_eq!(refused_shell.code, "conflict");
    assert_eq!(refused_shell.message, format!("pane {shell} runs no agent cornercase reads"));
    assert_eq!(refused(&mut app, "context", &json!({"pane": 999})).code, "not-found");
}

fn dialog_only(app: &mut App, area: Rect) -> String {
    let screen = format!("{}", render(app, area).backend());
    let lines: Vec<&str> = screen.lines().collect();
    let first = lines.iter().position(|l| l.contains('╭')).expect("the dialog's top");
    let last = lines.iter().position(|l| l.contains('╰')).expect("the dialog's bottom");
    lines[first..=last].join("\n")
}

#[test]
fn the_dialog_at_two_widths() {
    let mut p = Probed::new();
    p.open();
    insta::assert_snapshot!("the_context_dialog_wide", dialog_only(&mut p.agent.app, Rect::new(0, 0, 100, 40)));
    insta::assert_snapshot!("the_context_dialog_narrow", dialog_only(&mut p.agent.app, Rect::new(0, 0, 60, 40)));
}

#[test]
fn a_request_after_the_agent_restarted_is_still_answered() {
    let mut p = Probed::new();
    p.open();
    p.agent.app.agents.contexts.reads.get_mut(&p.id).expect("a read").pid = 1;
    ask(&mut p.agent.app, None, context(p.id, false));
    let got = info(p.agent.answered("the request is answered"));
    assert_eq!(got.breakdown.map(|b| b.total_tokens), Some(251_853));
    assert_eq!(p.runs().len(), 2);
}

#[test]
fn a_probe_that_hangs_answers_with_its_error_at_the_timeout() {
    let mut p = Probed::new();
    std::fs::write(format!("{}.hang", p.probe.display()), "").expect("make the probe hang");
    p.agent.app.agents.contexts.timeout = Duration::from_millis(400);
    ask(&mut p.agent.app, None, context(p.id, false));
    let got = info(p.agent.answered("the timeout is answered"));
    assert_eq!(got.error.as_deref(), Some("claude did not answer in time"));
    assert!(got.breakdown.is_none());
}
