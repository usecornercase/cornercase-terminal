use std::any::Any;

use super::*;
use crate::agents::activity::Status;
use crate::agents::codex::fake::FakeCodex;
use crate::agents::gemini::fake::FakeGemini;
use crate::test_util::signal;

struct Pair {
    app: App,
    rx: Receiver<AppEvent>,
    ids: [u64; 2],
    signals: [PathBuf; 2],
    idle: &'static str,
    _keep: Vec<Box<dyn Any>>,
}

type Made = ([String; 2], [PathBuf; 2], Vec<Box<dyn Any>>);

impl Pair {
    fn open(idle: &'static str, make: impl FnOnce(&Path) -> Made) -> Self {
        let (mut app, rx, dirs) = app_with(1);
        let (lines, signals, mut keep) = make(&app.projects[0].path.clone());
        type_line(&mut app, &lines[0]);
        let first = first(&app);
        app.add_tab(0, 0, AREA).expect("a second tab");
        type_line(&mut app, &lines[1]);
        let ids = [first, first_of_tab(&app, 1)];
        keep.extend(dirs.into_iter().map(|dir| Box::new(dir) as Box<dyn Any>));
        let mut pair = Self { app, rx, ids, signals, idle, _keep: keep };
        pair.refresh("both agents are idle", |a, ids| ids.iter().all(|&id| status(a, id) == Some(Status::Idle)));
        pair
    }

    fn geminis() -> Self {
        Self::open("◇  Ready (project)", |folder| {
            let (fake, signals) = (FakeGemini::new(folder), TempDir::new());
            let home = fake.dir.path().to_path_buf();
            let args = [&fake.script, &fake.session, signals.path()].map(|p| p.to_string_lossy().into_owned());
            let shared = format!(
                "env GEMINI_CLI_HOME={} {}",
                crate::agents::quote(&home.to_string_lossy()),
                crate::agents::join_args(&args)
            );
            let paths = [home, signals.path().to_path_buf()];
            ([fake.command_line(), shared], paths, vec![Box::new(fake), Box::new(signals)])
        })
    }

    fn codexes() -> Self {
        Self::open("fix the login | project", |folder| {
            let (one, other) = (TempDir::new(), TempDir::new());
            let fake = FakeCodex::new("019a1234-5678-7000-8000-000000000001", "gpt-5.4", false);
            let daemon = fake.serve(folder);
            let lines = [fake.served_command_line(one.path()), fake.served_command_line(other.path())];
            let paths = [one.path().to_path_buf(), other.path().to_path_buf()];
            (lines, paths, vec![Box::new(daemon), Box::new(fake), Box::new(one), Box::new(other)])
        })
    }

    fn refresh(&mut self, what: &str, cond: impl Fn(&App, [u64; 2]) -> bool) {
        let ids = self.ids;
        refreshing(&mut self.app, &self.rx, what, |a| cond(a, ids));
    }

    fn turn_over(&mut self, pane: usize) {
        signal(&self.signals[pane], "title", self.idle);
    }
}

fn first_of_tab(app: &App, tab: usize) -> u64 {
    app.projects[0].workspaces[0].tabs[tab].pane().map(|t| t.id).expect("the tab's pane")
}

fn status(app: &App, id: u64) -> Option<Status> {
    pane(app, id).agent.status()
}

#[rstest::rstest]
#[case::codex_served_by_its_daemon(Pair::codexes as fn() -> Pair)]
#[case::gemini_with_one_home(Pair::geminis as fn() -> Pair)]
fn a_send_with_enter_returns_once_enter_is_pressed(#[case] pair: fn() -> Pair) {
    let mut pair = pair();
    pair.app.confirm_within = Duration::from_millis(300);
    ask(&mut pair.app, None, send_text(pair.ids[0], "fix the login", true, false));
    assert_eq!(done(answered(&mut pair.app, &pair.rx, "the send returns")).ids.pane, Some(pair.ids[0]));
}

#[rstest::rstest]
#[case::codex_served_by_its_daemon(Pair::codexes as fn() -> Pair)]
#[case::gemini_with_one_home(Pair::geminis as fn() -> Pair)]
fn status_and_turn_over_come_from_each_ones_title(#[case] pair: fn() -> Pair) {
    let mut pair = pair();
    let [one, other] = pair.ids;
    ask(&mut pair.app, None, send_text(one, "fix the login", true, true));
    pair.refresh("the first agent works", |a, _| status(a, one) == Some(Status::Working));
    assert_eq!(status(&pair.app, other), Some(Status::Idle));
    pair.turn_over(0);
    let ended = done(answered(&mut pair.app, &pair.rx, "its turn ends"));
    assert_eq!(ended.ended.as_deref(), Some("done"));
}

#[rstest::rstest]
#[case::codex_served_by_its_daemon(Pair::codexes as fn() -> Pair, "two Codex in one folder hide each other")]
#[case::gemini_with_one_home(Pair::geminis as fn() -> Pair, "two Gemini share that home and folder")]
fn the_last_message_says_the_record_cannot_be_found(#[case] pair: fn() -> Pair, #[case] why: &'static str) {
    let mut pair = pair();
    let command = Command::LastMessage(wire::LastMessage { pane: Some(pair.ids[0]), ..wire::LastMessage::default() });
    ask(&mut pair.app, None, command);
    let message = error(answered(&mut pair.app, &pair.rx, "the last message is refused"));
    assert!(message.contains(why), "{message}");
}

#[test]
fn a_lone_agent_without_a_record_still_waits_for_its_prompt_to_be_recorded() {
    let (mut app, rx, _dirs) = app_with(1);
    let (fake, signals) = (FakeCodex::new("019a1234-5678-7000-8000-000000000001", "gpt-5.4", false), TempDir::new());
    let _daemon = fake.serve(&app.projects[0].path.clone());
    type_line(&mut app, &fake.served_command_line(signals.path()));
    let id = first(&app);
    refreshing(&mut app, &rx, "Codex is idle", |a| status(a, id) == Some(Status::Idle));
    app.confirm_within = Duration::from_millis(300);
    let send = wire::SendText { timeout: Some(1.0), ..send_text_of(id) };
    ask(&mut app, None, Command::Send(send));
    let message = error(answered(&mut app, &rx, "the send gives up"));
    assert!(message.starts_with("timed out"), "{message}");
}

fn send_text_of(pane: u64) -> wire::SendText {
    wire::SendText { pane: Some(pane), text: Some("fix the login".into()), enter: true, ..wire::SendText::default() }
}
