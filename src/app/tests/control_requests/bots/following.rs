use std::time::Duration;

use super::*;
use crate::agents::activity::Status;
use crate::app::bots::Started;
use crate::app::control::Starter;
use crate::bots::chat::{Line, read};
use crate::bots::folder::CHAT_FILE;

const USER: &str = r#"{"type":"user","message":{"role":"user","content":"fix"}}"#;
const REPLY: &str = r#"{"type":"assistant","timestamp":"2026-10-08T19:56:37.241Z","message":{"id":"m1","model":"claude-opus-5-5","content":[{"type":"text","text":"Fixed the flaky test."}],"usage":{"input_tokens":2,"cache_creation_input_tokens":15655,"cache_read_input_tokens":149954}}}"#;

struct Following {
    agent: Agent,
    program: PathBuf,
    _dirs: (TempDir, TempDir),
}

fn following() -> Following {
    let mut agent = Agent::new();
    let (state, bin) = (TempDir::new(), TempDir::new());
    let program = fake_program(bin.path(), "gemini");
    std::fs::write(program.with_extension("out"), fixture!("gemini/0.67.0/bot-new.jsonl")).expect("canned output");
    for kind in Known::ALL.into_iter().filter(|kind| *kind != Known::Claude) {
        let command =
            if kind == Known::Gemini { program.display().to_string() } else { format!("/nonexistent/{}", kind.name()) };
        agent.app.config.agent_commands.insert(kind.name().into(), command);
    }
    agent.app.set_bots_dir(&state.path().join("bots"));
    agent.app.panels.bots.batch = Duration::ZERO;
    let mut made = Following { agent, program, _dirs: (state, bin) };
    for name in ["PRs", "Digest"] {
        made.bot(&json!({"action": "new", "name": name, "agent": "gemini", "access": "cornercase"}));
    }
    made
}

impl Following {
    fn bot(&mut self, args: &Value) -> Value {
        asked(&mut self.agent.app, &self.agent.rx, "bot", &wire(args))
            .unwrap_or_else(|e| panic!("{args}: {}", e.message))
    }

    fn lines(&self) -> Vec<Line> {
        read(&self.agent.app.panels.bots.live[0].folder.join(CHAT_FILE))
    }

    fn kinds(&self, kind: Kind) -> Vec<Line> {
        self.lines().into_iter().filter(|line| line.kind == kind).collect()
    }

    fn argvs(&self) -> Vec<String> {
        std::fs::read_to_string(self.program.with_extension("argv"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn request(&mut self, bot: Option<&str>, run: Option<u64>, command: Command) {
        let (bot, bot_run) = (bot.map(str::to_string), run);
        let request =
            wire::Request { caller: None, server: None, contract: None, boot: None, view: None, bot, bot_run, command };
        let text = serde_json::to_string(&request).expect("json");
        self.agent.app.request(CLIENT, &text, Some(AREA), Instant::now());
    }

    fn start_with(&mut self, bot: Option<&str>, prompt: &str, wait: bool) -> u64 {
        let start = wire::Start {
            agent: Some("claude".into()),
            name: Some("fixer".into()),
            prompt: Some(prompt.into()),
            wait,
            until: Until::Text("❯".into()),
            ..wire::Start::default()
        };
        self.request(bot, None, Command::Start(start));
        let started = done(answered(&mut self.agent.app, &self.agent.rx, "the agent is ready"));
        started.ids.pane.expect("the agent's pane")
    }

    fn start_as_prs(&mut self) -> u64 {
        self.start_with(Some("prs"), "go", false)
    }

    fn until(&mut self, what: &str, cond: impl Fn(&App) -> bool) {
        let start = Instant::now();
        loop {
            drain(&mut self.agent.app, &self.agent.rx);
            self.agent.app.refresh(Instant::now());
            if cond(&self.agent.app) {
                return;
            }
            if start.elapsed() > Duration::from_secs(8) {
                let term = self.agent.app.projects.terms().find(|t| t.bot.is_some());
                eprintln!(
                    "DEBUG {what}: state={:?} status={:?} follows={:?} reports={} lines={:?} screen={:?}",
                    term.map(|t| t.agent.state()),
                    term.map(|t| t.agent.status()),
                    self.follows(),
                    self.agent.app.panels.bots.live[0].reports.len(),
                    self.lines().iter().map(|l| (l.kind, l.text.clone())).collect::<Vec<_>>(),
                    term.map(|t| t.emulator.screen_text().ok()),
                );
                panic!("timed out: {what}");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn follows(&self) -> Vec<(u64, bool)> {
        self.agent.app.panels.bots.live[0].following.iter().map(|followed| (followed.pane, followed.armed)).collect()
    }

    fn works(&mut self, pane: u64) {
        self.until("the agent works", move |a| {
            a.projects.pane_by(pane).is_some_and(|term| term.agent.status() == Some(Status::Working))
        });
    }

    fn finishes_and_is_answered(&mut self, pane: u64) {
        self.works(pane);
        self.says(pane);
        self.agent.finish();
        self.until("the bot answers the report", |a| chat_of(a, 0).iter().any(|line| line.kind == Kind::Bot));
    }

    fn shows_a_started_row(&self, pane: u64) {
        let card = self.agent.app.bot_card(0);
        let row = card.rows.iter().find(|row| row.label == "started").expect("a started row");
        assert_eq!(row.pane, Some(pane));
        assert!(row.status.is_some() && row.value.starts_with("claude · "), "{row:?}");
    }

    fn quiet(&mut self, span: Duration) {
        let from = Instant::now();
        self.until("the quiet passes", move |_| from.elapsed() >= span);
    }

    fn idle(&mut self, pane: u64) {
        self.until("the agent rests", move |a| {
            a.projects
                .pane_by(pane)
                .is_some_and(|term| matches!(term.agent.status(), Some(Status::Idle | Status::Done)))
        });
    }

    fn says(&mut self, pane: u64) {
        self.agent.transcript(pane, &[USER, REPLY]);
    }
}

fn chat_of(app: &App, at: usize) -> Vec<Line> {
    read(&app.panels.bots.live[at].folder.join(CHAT_FILE))
}

#[test]
fn a_start_from_a_bot_tags_the_pane_and_the_chat_says_started() {
    let mut f = following();
    let pane = f.start_as_prs();
    assert_eq!(f.agent.app.projects.pane_by(pane).and_then(|term| term.bot.clone()).as_deref(), Some("prs"));
    let report = f.agent.app.report(None);
    let mut tabs = report.projects.iter().flat_map(|p| &p.workspaces).flat_map(|w| &w.tabs);
    let tab = tabs.find(|tab| tab.panes.iter().any(|p| p.id == pane)).expect("its tab");
    assert_eq!(tab.bot.as_deref(), Some("prs"));
    assert_eq!(tab.panes.iter().find(|p| p.id == pane).and_then(|p| p.bot.as_deref()), Some("prs"));
    let started = f.kinds(Kind::Started);
    assert_eq!(started.len(), 1);
    assert!(started[0].text.starts_with("▶ started claude in "), "{}", started[0].text);
    let agent = started[0].agent.clone().expect("the agent");
    assert_eq!(
        (agent.pane, agent.kind.as_str(), agent.boot.as_str()),
        (pane, "claude", crate::control::server_token())
    );
    f.shows_a_started_row(pane);
    let listed = f.bot(&json!({"action": "list"}));
    assert_eq!(listed["bots"][0]["following"], json!([pane]));
    assert_eq!(listed["bots"][0]["follow"], json!(true));
}

#[test]
fn a_start_named_by_an_unknown_bot_tags_nothing() {
    let mut f = following();
    let pane = f.start_with(Some("nobody"), "go", false);
    assert_eq!(f.agent.app.projects.pane_by(pane).and_then(|term| term.bot.clone()), None);
    assert!(f.kinds(Kind::Started).is_empty() && f.follows().is_empty());
}

#[test]
fn a_cornercase_bot_connection_tags_with_its_own_bot_whatever_it_names() {
    let mut f = following();
    f.agent.app.clients.bots.marked(CLIENT, "digest");
    let own = f.agent.app.panels.bots.live[1].key;
    let starter = f.agent.app.starter(CLIENT, Some("prs"), None).expect("a starter");
    assert_eq!(starter.bot, own);
    let someone = f.agent.app.starter(CLIENT + 1, Some("prs"), None).expect("taken at its word");
    assert_eq!(someone.bot, f.agent.app.panels.bots.live[0].key);
    assert_eq!(f.agent.app.starter(CLIENT + 1, Some("nobody"), None), None);
    assert_eq!(f.agent.app.starter(CLIENT + 1, None, None), None);
}

#[test]
fn when_the_agent_finishes_the_chat_gets_finished_and_the_bot_resumes_its_chat_session() {
    let mut f = following();
    f.agent.app.panels.bots.live[0].bot.conversation = Some("chat-s".into());
    let pane = f.start_as_prs();
    f.finishes_and_is_answered(pane);
    let finished = f.kinds(Kind::Finished);
    assert_eq!(finished.len(), 1);
    assert!(finished[0].text.starts_with("✓ claude finished in "), "{}", finished[0].text);
    assert_eq!(finished[0].body.as_deref(), Some("Fixed the flaky test."));
    let argv = f.argvs().pop().expect("the bot's turn");
    assert!(argv.contains("chat-s") && argv.contains("--resume"), "{argv}");
    assert!(argv.contains(&format!("(pane {pane}) finished. Its last message:")), "{argv}");
    assert!(argv.contains("## cornercase says"), "{argv}");
    assert_eq!(f.follows(), [(pane, false)]);
}

#[test]
fn an_agent_a_routine_run_started_is_reported_into_the_runs_session() {
    let mut f = following();
    f.agent.app.panels.bots.live[0].bot.conversation = Some("chat-s".into());
    let id = {
        let added = json!({"action": "add", "entry": {"bot": "prs", "at": "in 2h", "prompt": "x"}});
        asked(&mut f.agent.app, &f.agent.rx, "queue", &added).expect("added")["id"].as_u64().expect("id")
    };
    f.agent.app.panels.queued.answered(id, Some("run-s".into()), crate::queue::outcome::Outcome::default());
    let start = wire::Start { agent: Some("claude".into()), prompt: Some("go".into()), ..wire::Start::default() };
    f.request(Some("prs"), Some(id), Command::Start(start));
    let pane = done(answered(&mut f.agent.app, &f.agent.rx, "the agent is ready")).ids.pane.expect("pane");
    f.finishes_and_is_answered(pane);
    let argv = f.argvs().pop().expect("the bot's turn");
    assert!(argv.contains("run-s") && !argv.contains("chat-s"), "{argv}");
    let answer = f.kinds(Kind::Bot).pop().expect("the answer");
    assert_eq!(answer.run, Some(id));
    assert_eq!(f.agent.app.panels.bots.live[0].bot.conversation.as_deref(), Some("chat-s"));
    assert!(!f.agent.app.panels.bots.live[0].unseen.is_empty());
    assert_eq!(f.kinds(Kind::Finished)[0].run, Some(id));
}

#[test]
fn waiting_writes_a_line_and_starts_no_turn() {
    let mut f = following();
    let pane = f.start_with(Some("prs"), "ask", false);
    f.works(pane);
    f.agent.finish();
    f.until("the waiting line", |a| chat_of(a, 0).iter().any(|line| line.kind == Kind::Waiting));
    let waiting = f.kinds(Kind::Waiting);
    assert!(waiting[0].text.starts_with("! claude needs you in "), "{}", waiting[0].text);
    assert!(waiting[0].body.is_some());
    assert_eq!(f.argvs(), Vec::<String>::new());
    assert_eq!(f.agent.app.panels.bots.live[0].unread, 1);
    assert_eq!(f.follows(), [(pane, true)]);
}

#[test]
fn a_failed_turn_is_reported_as_failed() {
    let mut f = following();
    let pane = f.start_with(Some("prs"), "api-error", false);
    f.works(pane);
    f.says(pane);
    f.agent.finish();
    f.until("the failed line", |a| chat_of(a, 0).iter().any(|line| line.kind == Kind::Finished));
    let finished = f.kinds(Kind::Finished);
    assert!(finished[0].text.starts_with("✗ claude failed in "), "{}", finished[0].text);
}

#[test]
fn two_agents_finishing_within_the_window_make_one_turn() {
    let mut f = following();
    f.agent.app.panels.bots.batch = Duration::from_secs(30);
    let (first, second) = (f.start_as_prs(), f.start_as_prs());
    for pane in [first, second] {
        f.works(pane);
        f.says(pane);
    }
    f.agent.finish();
    f.until("one report waits", |a| !a.panels.bots.live[0].reports.is_empty());
    f.agent.finish();
    f.until("both reports wait", |a| a.panels.bots.live[0].reports.len() == 2);
    assert_eq!(f.argvs(), Vec::<String>::new());
    f.agent.app.drive_bot_reports(Instant::now() + Duration::from_secs(31));
    f.until("the bot answers", |a| chat_of(a, 0).iter().any(|line| line.kind == Kind::Bot));
    assert_eq!(f.argvs().len(), 1);
    assert_eq!(f.kinds(Kind::Finished).len(), 2);
    let kinds: Vec<Kind> =
        f.lines().iter().map(|line| line.kind).filter(|k| matches!(k, Kind::Finished | Kind::Bot)).collect();
    assert_eq!(kinds, [Kind::Finished, Kind::Finished, Kind::Bot]);
}

#[test]
fn only_the_first_finish_after_a_prompt_counts_and_a_bot_send_arms_it_again() {
    let mut f = following();
    let pane = f.start_as_prs();
    assert_eq!(f.follows(), [(pane, true)]);
    f.finishes_and_is_answered(pane);
    assert_eq!(f.follows(), [(pane, false)]);
    let send = |text: &str| {
        Command::Send(wire::SendText {
            pane: Some(pane),
            text: Some(text.into()),
            enter: true,
            ..wire::SendText::default()
        })
    };
    f.request(None, None, send("typed by the user"));
    done(answered(&mut f.agent.app, &f.agent.rx, "the user's send"));
    assert_eq!(f.follows(), [(pane, false)]);
    f.works(pane);
    f.agent.finish();
    f.idle(pane);
    f.quiet(Duration::from_millis(2500));
    assert_eq!((f.kinds(Kind::Finished).len(), f.argvs().len()), (1, 1));
    f.request(Some("prs"), None, send("typed by the bot"));
    done(answered(&mut f.agent.app, &f.agent.rx, "the bot's send"));
    assert_eq!(f.follows(), [(pane, true)]);
    f.works(pane);
    f.agent.finish();
    f.until("the second report", |a| chat_of(a, 0).iter().filter(|line| line.kind == Kind::Finished).count() == 2);
}

#[test]
fn a_waited_start_tags_but_does_not_arm() {
    let mut f = following();
    let pane = f.start_with(Some("prs"), "go", true);
    assert_eq!(f.follows(), [(pane, false)]);
    assert_eq!(f.agent.app.projects.pane_by(pane).and_then(|term| term.bot.clone()).as_deref(), Some("prs"));
    assert_eq!(f.kinds(Kind::Started).len(), 1);
}

#[test]
fn the_twenty_first_drops_the_oldest_with_a_notice() {
    let mut f = following();
    let key = f.agent.app.panels.bots.live[0].key;
    let mut panes = Vec::new();
    for _ in 0..21 {
        let made = done(now(&mut f.agent.app, None, new_tab(None)));
        panes.push(made.ids.pane.expect("a pane"));
    }
    for pane in &panes {
        let started = Started { agent: Some("claude".into()), arm: true };
        f.agent.app.tag_started(*pane, Some(Starter { bot: key, run: None }), started);
    }
    let followed: Vec<u64> = f.follows().into_iter().map(|(pane, _)| pane).collect();
    assert_eq!(followed, panes[1..]);
    let notices = f.kinds(Kind::Notice);
    assert!(notices.last().is_some_and(|line| line.text.starts_with("no longer following claude in ")
        && line.text.ends_with("a bot follows at most 20 agents")));
    assert!(
        panes.iter().all(|pane| f
            .agent
            .app
            .projects
            .pane_by(*pane)
            .is_some_and(|term| term.bot.as_deref() == Some("prs")))
    );
}

#[test]
fn follow_off_stops_it_all() {
    let mut f = following();
    let before = f.start_as_prs();
    f.bot(&json!({"action": "edit", "bot": "prs", "follow": false}));
    assert_eq!(f.follows(), Vec::<(u64, bool)>::new());
    f.works(before);
    f.agent.finish();
    f.idle(before);
    let pane = f.start_as_prs();
    assert_eq!(f.agent.app.projects.pane_by(pane).and_then(|term| term.bot.clone()).as_deref(), Some("prs"));
    assert_eq!(f.follows(), Vec::<(u64, bool)>::new());
    assert_eq!(f.kinds(Kind::Started).len(), 1);
    f.works(pane);
    f.says(pane);
    f.agent.finish();
    f.idle(pane);
    assert!(f.kinds(Kind::Finished).is_empty() && f.argvs().is_empty());
    let listed = f.bot(&json!({"action": "list"}));
    assert_eq!(
        (listed["bots"][0]["follow"].clone(), listed["bots"][0]["following"].clone()),
        (json!(false), json!([]))
    );
    f.bot(&json!({"action": "edit", "bot": "prs", "follow": true}));
    assert_eq!(f.follows(), [(before, false), (pane, false)]);
}

#[test]
fn stop_drops_pending_reports_and_remove_clears_the_tags() {
    let mut f = following();
    f.agent.app.panels.bots.batch = Duration::from_secs(3600);
    let pane = f.start_as_prs();
    f.works(pane);
    f.says(pane);
    f.agent.finish();
    f.until("the report waits", |a| a.panels.bots.live[0].reports.len() == 1);
    f.bot(&json!({"action": "stop", "bot": "prs"}));
    assert!(f.agent.app.panels.bots.live[0].reports.is_empty());
    f.bot(&json!({"action": "remove", "bot": "prs"}));
    assert_eq!(f.agent.app.projects.pane_by(pane).and_then(|term| term.bot.clone()), None);
}

mod restarting {
    use super::*;
    use crate::state::{PaneState, TabState};

    #[test]
    fn a_restart_keeps_the_bot_on_panes_and_follows_them_disarmed() {
        let mut f = following();
        let pane = f.start_as_prs();
        f.works(pane);
        f.says(pane);
        f.until("the conversation is known", move |a| a.projects.pane_by(pane).is_some_and(|t| t.resume.is_some()));
        let saved = f.agent.app.state();
        let tabs: Vec<&TabState> = saved.projects.iter().flat_map(|p| &p.workspaces).flat_map(|w| &w.tabs).collect();
        let panes: Vec<&PaneState> = tabs.iter().flat_map(|tab| &tab.panes).collect();
        assert!(panes.iter().any(|saved| saved.bot.as_deref() == Some("prs")), "{pane}");
        let (mut back, _rx) = empty_app();
        back.config.agent_commands = f.agent.app.config.agent_commands.clone();
        back.set_bots_dir(&f.agent.app.panels.bots.dir.clone().expect("a folder"));
        assert!(back.restore(&saved, AREA));
        let tagged = back.projects.terms().filter(|term| term.bot.as_deref() == Some("prs")).count();
        assert_eq!(tagged, 1);
        let restored: Vec<(u64, bool)> = back.panels.bots.live[back.panels.bots.by_id("prs").expect("prs")]
            .following
            .iter()
            .map(|followed| (followed.pane, followed.armed))
            .collect();
        let tagged_pane = back.projects.terms().find(|term| term.bot.is_some()).map(|term| term.id);
        assert_eq!(restored, [(tagged_pane.expect("a tagged pane"), false)]);
        let mut other = saved.clone();
        for pane in other
            .projects
            .iter_mut()
            .flat_map(|p| &mut p.workspaces)
            .flat_map(|w| &mut w.tabs)
            .flat_map(|t| &mut t.panes)
        {
            pane.bot = pane.bot.as_ref().map(|_| "gone".to_string());
        }
        let (mut gone, _rx) = empty_app();
        gone.set_bots_dir(&f.agent.app.panels.bots.dir.clone().expect("a folder"));
        gone.restore(&other, AREA);
        assert_eq!(gone.projects.terms().filter(|term| term.bot.is_some()).count(), 0);
    }
}

#[test]
fn a_report_waits_for_its_run_to_answer_and_costs_no_wake_meanwhile() {
    use crate::agents::probe::Running;
    use crate::app::bots::{Report, Run};

    let mut f = following();
    let agent = crate::bots::chat::AgentRef {
        kind: "claude".into(),
        pane: 1,
        workspace: 1,
        project: 1,
        boot: crate::control::server_token().to_string(),
    };
    let late = Instant::now().checked_sub(Duration::from_secs(60)).expect("an earlier instant");
    let report = Report {
        run: Some(5),
        agent,
        header: "✓ claude finished in x".into(),
        body: "done".into(),
        text: "the report".into(),
        at: late,
    };
    let live = &mut f.agent.app.panels.bots.live[0];
    live.reports.push(report);
    let running = Run {
        turn: 99,
        entry: 5,
        client: 0,
        label: "Review".into(),
        clock: String::new(),
        running: Running::default(),
        stopping: false,
    };
    live.runs.push(running);
    let now = Instant::now();
    assert_eq!(f.agent.app.bot_reports_wake(now), None);
    f.agent.app.drive_bot_reports(now);
    assert!(f.agent.app.panels.bots.live[0].turn.is_none());
    assert_eq!(f.agent.app.panels.bots.live[0].reports.len(), 1);
    f.agent.app.panels.bots.live[0].runs.clear();
    assert_eq!(f.agent.app.bot_reports_wake(now), Some(Duration::ZERO));
}

#[rstest::rstest]
#[case::at_the_desk(false)]
#[case::away(true)]
fn a_followed_agent_that_needs_you_is_sent_once_with_its_buttons(#[case] away: bool) {
    let mut f = following();
    f.agent.app.panels.bots.live[0].bot.chat_apps = crate::bots::ChatApps::Always;
    let mut phone = Phone::paired(&mut f.agent.app);
    if away {
        f.agent.app.away_request(false).expect("away");
    }
    let pane = f.start_with(Some("prs"), "ask", false);
    f.works(pane);
    f.agent.finish();
    let mut seen = Vec::new();
    let mut read = |f: &mut Following| {
        drain(&mut f.agent.app, &f.agent.rx);
        f.agent.app.refresh(Instant::now());
        seen.extend(phone.read(&mut f.agent.app).into_iter().map(|(_, order)| order));
        seen.iter().filter(|order| html(order).contains("claude needs you")).cloned().collect::<Vec<_>>()
    };
    wait_until("the chat app is told", || !read(&mut f).is_empty());
    let from = Instant::now();
    let mut needs = Vec::new();
    while from.elapsed() < Duration::from_secs(3) {
        needs = read(&mut f);
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(needs.len(), 1, "{needs:?}");
    assert!(keyboard(&needs[0]).iter().flatten().any(|key| key.text == "esc"), "{needs:?}");
    assert_eq!(f.kinds(Kind::Waiting).len(), 1);
}
