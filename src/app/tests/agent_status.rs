use rstest::rstest;

use super::*;
use crate::agents::activity::{Activity, Status};
use crate::agents::claude::fake::FakeClaude;
use crate::agents::codex::fake::FakeCodex;
use crate::agents::gemini::fake::FakeGemini;
use crate::agents::grok::fake::FakeGrok;
use crate::agents::opencode::fake::FakeOpencode;
use crate::config;
use crate::test_util::write_executable;

mod flows;
mod osc;
mod resuming;

fn working_claude() -> (App, Receiver<AppEvent>, Vec<TempDir>, FakeClaude) {
    let (mut app, rx, dirs) = app_with(1);
    let claude = FakeClaude::new();
    claude.start(&mut app);
    watch_until(&mut app, &rx, "claude works", |a| status(a, 0, 0) == Some(Status::Working));
    (app, rx, dirs, claude)
}

const TURN_STARTED: &str = crate::test_util::fixture!("codex/0.160.0/turn-started.jsonl");
const TURN_COMPLETE: &str = crate::test_util::fixture!("codex/0.160.0/turn-complete.jsonl");
const OPENCODE_WORKING: &str = crate::test_util::fixture!("opencode/1.18.35/working.jsonl");
const OPENCODE_REPLY: &str = crate::test_util::fixture!("opencode/1.18.35/reply.jsonl");

trait StartClaude {
    fn start(&self, app: &mut App) {
        self.start_with(app, "");
    }

    fn start_with(&self, app: &mut App, args: &str);
}

impl StartClaude for FakeClaude {
    fn start_with(&self, app: &mut App, args: &str) {
        app.agents.claude_dir = Some(self.dir().to_path_buf());
        type_line(app, &self.command_line(args));
    }
}

fn tick(app: &mut App) {
    let now = app.agents.watched.map_or_else(Instant::now, |at| at + WATCH_AGENTS_EVERY);
    app.watch_agents(now);
}

fn watch_until(app: &mut App, rx: &Receiver<AppEvent>, what: &str, cond: impl Fn(&App) -> bool) {
    wait_until(what, || {
        drain(app, rx);
        tick(app);
        cond(app)
    });
}

fn status(app: &App, p: usize, t: usize) -> Option<Status> {
    app.projects[p].workspaces[0].tabs[t].status()
}

fn text(t: &Terminal<TestBackend>, r: Rect) -> String {
    (r.x..r.right()).map(|x| t.backend().buffer()[(x, r.y)].symbol().to_string()).collect()
}

fn second_row(app: &mut App) -> (u16, String) {
    let r = row_rect(app, WorkspaceRow::Tab(0, 0));
    let below = text(&render(app, AREA), Rect { y: r.y + 1, height: 1, ..r });
    (r.height, below.trim_end().to_string())
}

#[rstest]
#[case::the_moon("moon", "◐", false)]
#[case::the_spinner("spinner", "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏", true)]
fn a_working_agent_shows_the_chosen_icon_and_only_the_spinner_wakes_the_loop(
    #[case] icon: &str,
    #[case] glyphs: &str,
    #[case] spins: bool,
) {
    let (mut app, rx, _dirs, claude) = working_claude();
    app.config.working_icon = icon.into();
    let r = row_rect(&app, WorkspaceRow::Tab(0, 0));
    let row = text(&render(&mut app, AREA), r);
    let working = app.tick(Instant::now());
    claude.signal("finish");
    watch_until(&mut app, &rx, "claude finishes in sight", |a| status(a, 0, 0) == Some(Status::Idle));

    let spinning = working.is_some_and(|wait| !wait.is_zero() && wait <= activity::SPIN_EVERY);
    let shown = row.chars().any(|c| glyphs.contains(c));
    assert_eq!((shown, spinning, app.tick(Instant::now())), (true, spins, None));
}

#[test]
fn a_tab_follows_what_claude_says_it_is_doing() {
    let (mut app, rx, _dirs, claude) = working_claude();
    claude.signal("finish");
    watch_until(&mut app, &rx, "claude finishes in sight", |a| status(a, 0, 0) == Some(Status::Idle));
    claude.signal("quit");

    watch_until(&mut app, &rx, "the tab loses its icon", |a| status(a, 0, 0).is_none());
}

#[test]
fn finishing_out_of_sight_marks_the_tab_done_until_it_is_opened() {
    let (mut app, rx, _dirs, claude) = working_claude();
    app.add_tab(0, 0, AREA).expect("add a tab");
    claude.signal("finish");
    watch_until(&mut app, &rx, "claude finishes out of sight", |a| status(a, 0, 0) == Some(Status::Done));

    click_row(&mut app, WorkspaceRow::Tab(0, 0));
    let now = app.agents.watched.expect("watched");
    app.watch_agents(now);

    assert_eq!(status(&app, 0, 0), Some(Status::Idle));
}

#[test]
fn the_compact_menu_hides_the_tab_beneath_it() {
    let (mut app, rx, _dirs, claude) = working_claude();
    app.nav = Some(ui::Nav::Workspaces);
    claude.signal("finish");
    watch_until(&mut app, &rx, "claude finishes under the menu", |a| status(a, 0, 0) == Some(Status::Done));
    let small = Rect::new(0, 0, 80, 30);
    let bar = text(&render(&mut app, small), Rect::new(0, 1, 7, 1));

    app.nav = None;
    let now = app.agents.watched.expect("watched");
    app.watch_agents(now);

    assert_eq!((bar.as_str(), status(&app, 0, 0)), ("   ≡ ✓ ", Some(Status::Idle)));
}

#[rstest]
#[case::both(true, true, (2, "▌ │   Opus 5.5 · 17%"))]
#[case::the_model_alone(true, false, (2, "▌ │   Opus 5.5"))]
#[case::the_context_alone(false, true, (2, "▌ │   17%"))]
#[case::neither(false, false, (1, "  └ + Tab"))]
fn a_tab_running_claude_shows_its_model_and_context_under_its_name(
    #[case] model: bool,
    #[case] context: bool,
    #[case] expected: (u16, &str),
) {
    let (mut app, rx, _dirs) = app_with(1);
    (app.config.model, app.config.context) = (model, context);
    let claude = FakeClaude::running(FakeClaude::ANSWERING);
    claude.start(&mut app);
    watch_until(&mut app, &rx, "claude answers", |a| a.projects[0].workspaces[0].tabs[0].context().is_some());

    let (height, below) = second_row(&mut app);
    claude.signal("quit");

    assert_eq!((height, below.as_str()), expected);
}

fn memory(app: &App) -> Option<u64> {
    app.projects[0].workspaces[0].tabs[0].memory()
}

fn measured_claude() -> (App, Receiver<AppEvent>, Vec<TempDir>, FakeClaude) {
    let (mut app, rx, dirs) = app_with(1);
    app.config.memory = true;
    let claude = FakeClaude::running(FakeClaude::SILENT);
    claude.start(&mut app);
    watch_until(&mut app, &rx, "the memory is measured", |a| memory(a).is_some());
    (app, rx, dirs, claude)
}

#[test]
fn with_the_memory_on_an_agent_tab_shows_how_much_it_uses_under_its_name() {
    let (mut app, _rx, _dirs, claude) = measured_claude();

    let (height, below) = second_row(&mut app);
    claude.signal("quit");

    assert_eq!((height, below.ends_with(" MB")), (2, true), "{below}");
}

fn measured_shell() -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, dirs) = app_with(1);
    app.config.memory = true;
    watch_until(&mut app, &rx, "the shell is measured", |a| memory(a).is_some());
    (app, rx, dirs)
}

#[test]
fn turning_the_memory_off_gives_the_tab_its_row_back() {
    let (mut app, _rx, _dirs) = measured_shell();

    app.config.memory = false;
    let (height, below) = second_row(&mut app);

    assert_eq!((height, below.contains(" MB")), (1, false), "{below}");
}

#[test]
fn with_the_memory_on_a_shell_tab_shows_how_much_it_uses_under_its_name() {
    let (mut app, _rx, _dirs) = measured_shell();

    let (height, below) = second_row(&mut app);

    assert_eq!((height, below.ends_with(" MB")), (2, true), "{below}");
}

#[test]
fn a_shell_tab_says_whether_a_program_runs_in_it() {
    let (mut app, rx, _dirs) = app_with(1);
    let running = |a: &App| a.projects[0].workspaces[0].tabs[0].running();
    watch_until(&mut app, &rx, "the shell is at its prompt", |a| !running(a));

    type_line(&mut app, "sleep 30");
    watch_until(&mut app, &rx, "sleep runs", running);
    send_key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);

    watch_until(&mut app, &rx, "the shell is back", |a| !running(a));
}

#[test]
fn a_split_tab_adds_up_the_memory_of_every_pane() {
    let (mut app, rx, _dirs) = measured_shell();
    let pane = areas().pane;
    right_click(&mut app, Position::new(pane.x + 1, pane.y + 1));
    pick(&mut app, "split right");
    let panes = |a: &App| -> Vec<Option<u64>> {
        a.projects[0].workspaces[0].tabs[0].panes.iter().map(|t| t.memory.bytes()).collect()
    };
    watch_until(&mut app, &rx, "both panes are measured", |a| panes(a).iter().all(Option::is_some));

    let sum = panes(&app).into_iter().flatten().sum::<u64>();

    assert_eq!(memory(&app), Some(sum));
}

#[test]
fn the_tree_measures_the_tabs_of_every_open_project() {
    let (mut app, rx, _dirs) = app_with(2);
    app.config.memory = true;
    app.config.sidebar = config::Sidebar::Tree.id().into();
    render(&mut app, Rect::new(0, 0, 100, 30));

    let measured = |a: &App| a.projects.iter().all(|p| p.workspaces[0].tabs[0].memory().is_some());
    watch_until(&mut app, &rx, "the tabs of both projects are measured", measured);
}

#[test]
fn concurrent_codex_sessions_in_one_directory_keep_their_own_context_and_cleanup() {
    let (mut app, rx, _dirs) = app_with(1);
    let first = FakeCodex::new("019a1234-5678-7000-8000-000000000001", "gpt-5.4", false);
    let mut second = FakeCodex::new("019a1234-5678-7000-8000-000000000002", "gpt-5.4-mini", true);
    second.home.clone_from(&first.home);
    let path = first.rollout.parent().expect("sessions").join(second.rollout.file_name().expect("filename"));
    std::fs::copy(&second.rollout, &path).expect("share the configured home");
    second.rollout = path;
    let context = |app: &App, t: usize| app.projects[0].workspaces[0].tabs[t].context().cloned();
    type_line(&mut app, &first.command_line());
    watch_until(&mut app, &rx, "first Codex answers", |a| context(a, 0).is_some());
    app.add_tab(0, 0, AREA).expect("add a concurrent session");
    type_line(&mut app, &second.command_line());
    watch_until(&mut app, &rx, "wrapper Codex answers", |a| context(a, 1).is_some());

    assert_eq!(context(&app, 0).expect("first").model, "gpt-5.4");
    assert_eq!(context(&app, 1).expect("second").model, "gpt-5.4-mini");
    assert_eq!(status(&app, 0, 1), Some(Status::Idle));
    second.append(crate::test_util::fixture!("codex/0.160.0/compacted.jsonl"));
    watch_until(&mut app, &rx, "compaction clears the percentage", |a| {
        context(a, 1).is_some_and(|c| c.percent.is_none())
    });
    assert_eq!(context(&app, 0).expect("first").percent, Some(20));
    second.signal("quit", "");
    watch_until(&mut app, &rx, "the wrapper session exits", |a| context(a, 1).is_none());
    assert_eq!(context(&app, 0).expect("first").percent, Some(20));
    first.signal("quit", "");
    watch_until(&mut app, &rx, "the native session exits", |a| context(a, 0).is_none());
}

#[test]
fn codex_model_and_session_changes_discard_obsolete_percentages() {
    let (mut app, rx, _dirs) = app_with(1);
    let fake = FakeCodex::new("019a1234-5678-7000-8000-000000000001", "gpt-5.4", false);
    let context = |a: &App| a.projects[0].workspaces[0].tabs[0].context().cloned();
    type_line(&mut app, &fake.command_line());
    watch_until(&mut app, &rx, "Codex answers", |a| context(a).is_some());
    fake.append(crate::test_util::fixture!("codex/0.160.0/model-change.jsonl"));
    watch_until(&mut app, &rx, "Codex changes models", |a| {
        context(a).is_some_and(|c| c.model == "gpt-5.4-mini" && c.percent.is_none())
    });

    let next = fake.rollout.with_file_name("rollout-2026-10-05T12-00-00-019a1234-5678-7000-8000-000000000002.jsonl");
    let header = crate::test_util::fixture!("codex/0.160.0/context.jsonl")
        .lines()
        .next()
        .expect("header")
        .replace("000000000001", "000000000002");
    std::fs::write(&next, format!("{header}\n")).expect("new session");
    fake.signal("switch", next.to_str().expect("path"));
    watch_until(&mut app, &rx, "Codex switches to an empty session", |a| context(a).is_none());
    fake.signal("quit", "");
}

#[test]
fn another_project_and_the_menu_button_show_that_claude_finished() {
    let (mut app, rx, _dirs) = app_with(2);
    app.active = 0;
    let claude = FakeClaude::new();
    claude.start(&mut app);
    watch_until(&mut app, &rx, "claude works", |a| status(a, 0, 0) == Some(Status::Working));
    app.active = 1;
    claude.signal("finish");
    watch_until(&mut app, &rx, "claude finishes out of sight", |a| status(a, 0, 0) == Some(Status::Done));

    let row = ui::entry_row(list(), areas().pitch, &app.sidebar_rows(), 0, SidebarRow::Project(0));
    let sidebar = text(&render(&mut app, AREA), row);
    let small = Rect::new(0, 0, 80, 30);
    let bar = text(&render(&mut app, small), Rect::new(0, 1, 7, 1));

    assert_eq!((sidebar.trim_end().ends_with('✓'), bar.as_str()), (true, "   ≡ ✓ "));
}

enum Fake {
    Claude(FakeClaude, i32),
    Codex(FakeCodex, usize),
    Gemini(FakeGemini, usize),
    Opencode(FakeOpencode, usize),
    Grok(FakeGrok, usize),
}

fn start_opencode(app: &mut App, rx: &Receiver<AppEvent>) -> FakeOpencode {
    let opencode = FakeOpencode::new();
    type_line(app, &opencode.command_line());
    pump_until(app, rx, "opencode runs", |a| runs(a, agents::Known::Opencode.name()));
    tick(app);
    opencode
}

fn opencode_says(app: &mut App, rx: &Receiver<AppEvent>, opencode: &FakeOpencode, tab: usize, messages: &str) {
    let folder = app.projects[0].path.clone();
    opencode.write(FakeOpencode::SESSION, &folder, messages);
    let turn = messages == OPENCODE_WORKING;
    watch_until(app, rx, "opencode's turn shows", |a| {
        tab_term(a, 0, tab).context.activity("") == Some(if turn { Activity::Working } else { Activity::Idle })
    });
}

struct Watched {
    app: App,
    rx: Receiver<AppEvent>,
    _dirs: Vec<TempDir>,
    agents: Vec<Fake>,
}

impl Watched {
    fn start(agent: agents::Known, hidden: bool) -> Self {
        let (app, rx, dirs) = app_with(1);
        let mut watched = Self { app, rx, _dirs: dirs, agents: Vec::new() };
        watched.run(agent);
        watched.report("busy", 1);
        if hidden {
            watched.app.add_tab(0, 0, AREA).expect("add a tab");
        }
        watched
    }

    fn run(&mut self, agent: agents::Known) {
        let (app, rx) = (&mut self.app, &self.rx);
        let fake = match agent {
            agents::Known::Codex => {
                let codex = FakeCodex::new("019a1234-5678-7000-8000-000000000001", "gpt-5.4", false);
                type_line(app, &codex.command_line());
                watch_until(app, rx, "codex runs", |a| a.tab().is_some_and(|t| t.context().is_some()));
                Fake::Codex(codex, app.projects[0].workspaces[0].active)
            }
            agents::Known::Gemini => {
                let gemini = FakeGemini::new(&app.projects[0].path);
                type_line(app, &gemini.command_line());
                pump_until(app, rx, "Gemini runs", |a| runs(a, agents::Known::Gemini.name()));
                Fake::Gemini(gemini, app.projects[0].workspaces[0].active)
            }
            agents::Known::Opencode => Fake::Opencode(start_opencode(app, rx), app.projects[0].workspaces[0].active),
            agents::Known::Grok => {
                let grok = FakeGrok::new(&app.projects[0].path);
                type_line(app, &grok.command_line());
                pump_until(app, rx, "grok runs", |a| runs(a, agents::Known::Grok.name()));
                let pid = app.term().and_then(Term::foreground_pid).expect("the pid of grok");
                grok.bind(pid);
                Fake::Grok(grok, app.projects[0].workspaces[0].active)
            }
            agents::Known::Claude => {
                let claude = FakeClaude::running(FakeClaude::SILENT);
                claude.start(app);
                pump_until(app, rx, "claude runs", |a| runs(a, agents::Known::Claude.name()));
                let pid = app.term().and_then(Term::foreground_pid).expect("the pid of claude");
                Fake::Claude(claude, pid)
            }
        };
        self.agents.push(fake);
    }

    fn report(&mut self, status: &str, ticks: usize) {
        for fake in &self.agents {
            match fake {
                Fake::Claude(claude, pid) => claude.report(*pid, status),
                Fake::Codex(codex, tab) => {
                    let (title, event) = match status {
                        "busy" => ("⠴ fix the login | shop", TURN_STARTED),
                        "waiting" => ("[ ! ] Action Required | fix the login | shop", ""),
                        _ => ("fix the login | shop", TURN_COMPLETE),
                    };
                    codex.append(event);
                    codex.signal("title", title);
                    pump_until(&mut self.app, &self.rx, "codex sets its title", |a| {
                        tab_term(a, 0, *tab).emulator.title() == title
                    });
                }
                Fake::Gemini(gemini, tab) => {
                    let title = match status {
                        "busy" => "✦  Working… (project)",
                        "waiting" => "✋  Action Required (project)",
                        _ => "◇  Ready (project)",
                    };
                    gemini.signal(title);
                    pump_until(&mut self.app, &self.rx, "Gemini sets its title", |a| {
                        tab_term(a, 0, *tab).emulator.title() == title
                    });
                }
                Fake::Opencode(opencode, tab) => {
                    let messages = if status == "busy" { OPENCODE_WORKING } else { OPENCODE_REPLY };
                    opencode_says(&mut self.app, &self.rx, opencode, *tab, messages);
                }
                Fake::Grok(grok, tab) => {
                    grok.report(status);
                    let wanted = match status {
                        "busy" => Activity::Working,
                        "waiting" => Activity::Waiting,
                        _ => Activity::Idle,
                    };
                    watch_until(&mut self.app, &self.rx, "grok's turn shows", |a| {
                        tab_term(a, 0, *tab).context.activity("") == Some(wanted)
                    });
                }
            }
        }
        for _ in 0..ticks {
            tick(&mut self.app);
        }
    }

    fn place(&self) -> String {
        format!("{} › default", self.app.project_label(&self.app.projects[0]))
    }

    fn told(&mut self) -> (Option<String>, Vec<Notification>) {
        (toast(&self.app).map(str::to_owned), self.app.take_notifications())
    }
}

#[rstest]
fn a_restart_names_the_agent_at_work_and_the_other_program(
    #[values(
        agents::Known::Claude,
        agents::Known::Codex,
        agents::Known::Gemini,
        agents::Known::Opencode,
        agents::Known::Grok
    )]
    agent: agents::Known,
) {
    let mut w = Watched::start(agent, true);
    type_line(&mut w.app, "sleep 30");
    pump_until(&mut w.app, &w.rx, "sleep runs", |a| {
        a.term().and_then(|t| t.program(&a.config)).as_deref() == Some("sleep")
    });

    let text = restart::confirmation(Some(&w.app.running()));

    assert!(text.contains("1 agent (1 working)") && text.contains("1 other program (`sleep` in "), "{text}");
}

fn runs(app: &App, agent: &str) -> bool {
    app.term()
        .and_then(Term::foreground_pid)
        .is_some_and(|pid| agents::detect(&app.config, &process::args(pid)).as_deref() == Some(agent))
}

#[rstest]
fn a_tab_follows_turns_and_approvals(
    #[values(agents::Known::Codex, agents::Known::Gemini, agents::Known::Grok)] agent: agents::Known,
) {
    let mut w = Watched::start(agent, false);
    let mut seen = vec![status(&w.app, 0, 0)];

    for step in ["waiting", "busy", "idle"] {
        w.report(step, 1);
        seen.push(status(&w.app, 0, 0));
    }

    let expected = [Status::Working, Status::Waiting, Status::Working, Status::Idle];
    assert_eq!(seen, expected.map(Some));
}

#[rstest]
fn a_hidden_tab_that_needs_you_shows_a_toast_and_sends_one_notification(
    #[values(agents::Known::Claude, agents::Known::Codex, agents::Known::Gemini, agents::Known::Grok)]
    agent: agents::Known,
) {
    let mut w = Watched::start(agent, true);

    w.report("waiting", 6);

    let text = format!("{} needs you in {}", agent.name(), w.place());
    let notification = Notification { text: text.clone(), channel: None };
    assert_eq!(w.told(), (Some(text), vec![notification]));
}

#[test]
fn an_opencode_tab_works_until_its_reply_and_shows_its_model_and_context() {
    let mut w = Watched::start(agents::Known::Opencode, false);
    let mut seen = vec![status(&w.app, 0, 0)];

    w.report("idle", 1);
    seen.push(status(&w.app, 0, 0));
    w.report("busy", 1);
    seen.push(status(&w.app, 0, 0));

    assert_eq!(seen, [Status::Working, Status::Idle, Status::Working].map(Some));
    assert_eq!(second_row(&mut w.app), (2, "▌ │   DeepSeek V4… · 12%".to_string()));
}

#[test]
fn a_grok_tab_shows_its_model_and_context() {
    let mut w = Watched::start(agents::Known::Grok, false);
    let row = second_row(&mut w.app).1;
    assert!(row.contains("grok-4.7") && row.contains("8%"), "{row}");
}

#[test]
fn gemini_context_reaches_both_layouts_and_ambiguity_hides_only_conversation_data() {
    let mut w = Watched::start(agents::Known::Gemini, false);
    let Fake::Gemini(fake, _) = &w.agents[0] else { unreachable!("Gemini was started") };
    fake.append(crate::test_util::fixture!("gemini/0.63.0/reply.jsonl"));
    watch_until(&mut w.app, &w.rx, "Gemini's context shows", |a| a.tab().is_some_and(|t| t.context().is_some()));
    assert!(second_row(&mut w.app).1.contains("1%"));
    w.app.nav = Some(ui::Nav::Workspaces);
    let compact = render(&mut w.app, Rect::new(0, 0, 80, 30));
    let contents: String = compact.backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect();
    assert!(contents.contains("gemini-"));
    assert!(contents.contains("1%"));
    let Fake::Gemini(fake, _) = &w.agents[0] else { unreachable!("Gemini was started") };
    let other = fake.start(&w.app.projects[0].path);
    watch_until(&mut w.app, &w.rx, "the ambiguous context goes", |a| a.tab().is_some_and(|t| t.context().is_none()));
    assert_eq!(status(&w.app, 0, 0), Some(Status::Working));
    drop(other);
    watch_until(&mut w.app, &w.rx, "the unambiguous context returns", |a| {
        a.tab().is_some_and(|t| t.context().is_some())
    });
}

#[test]
fn an_opencode_tab_is_measured_like_any_agent() {
    let mut w = Watched::start(agents::Known::Opencode, false);
    w.app.config.memory = true;

    watch_until(&mut w.app, &w.rx, "the memory is measured", |a| memory(a).is_some());
}

#[test]
fn a_second_opencode_in_its_folder_hides_the_status_instead_of_finishing_it() {
    let mut w = Watched::start(agents::Known::Opencode, true);
    let Fake::Opencode(opencode, _) = &w.agents[0] else { unreachable!("an opencode was started") };
    let other = opencode.start(&w.app.projects[0].path.clone());

    watch_until(&mut w.app, &w.rx, "the status goes", |a| status(a, 0, 0).is_none());
    drop(other);

    assert_eq!(w.told().1, []);
}

#[test]
fn opencode_finishing_out_of_sight_marks_the_tab_done_until_it_is_opened() {
    let mut w = Watched::start(agents::Known::Opencode, true);

    w.report("idle", 1);
    assert_eq!(status(&w.app, 0, 0), Some(Status::Done));
    click_row(&mut w.app, WorkspaceRow::Tab(0, 0));
    tick(&mut w.app);

    assert_eq!(status(&w.app, 0, 0), Some(Status::Idle));
}

#[rstest]
fn a_hidden_tab_that_finishes_says_so(
    #[values(
        agents::Known::Claude,
        agents::Known::Codex,
        agents::Known::Gemini,
        agents::Known::Opencode,
        agents::Known::Grok
    )]
    agent: agents::Known,
) {
    let mut w = Watched::start(agent, true);

    w.report("idle", 6);

    let text = format!("{} finished in {}", agent.name(), w.place());
    assert_eq!(w.told().1, [Notification { text, channel: None }]);
}

#[rstest]
fn a_settled_change_notifies_once(
    #[values(agents::Known::Claude, agents::Known::Codex, agents::Known::Gemini, agents::Known::Grok)]
    agent: agents::Known,
) {
    let mut w = Watched::start(agent, true);
    w.report("waiting", 6);
    w.told();

    w.report("waiting", 6);

    assert_eq!(w.told().1, []);
}

#[rstest]
fn the_visible_tab_stays_quiet(
    #[values(agents::Known::Claude, agents::Known::Codex, agents::Known::Gemini, agents::Known::Grok)]
    agent: agents::Known,
) {
    let mut w = Watched::start(agent, false);

    w.report("waiting", 6);

    assert_eq!(w.told(), (None, Vec::new()));
}

#[rstest]
fn a_question_answered_at_once_stays_quiet(
    #[values(agents::Known::Claude, agents::Known::Codex, agents::Known::Gemini)] agent: agents::Known,
) {
    let mut w = Watched::start(agent, true);
    w.report("waiting", 1);

    w.report("busy", 6);

    assert_eq!(w.told(), (None, Vec::new()));
}

#[test]
fn claude_and_codex_in_one_workspace_each_say_who_needs_you() {
    let mut w = Watched::start(agents::Known::Claude, true);
    w.run(agents::Known::Codex);
    w.report("busy", 1);
    w.app.add_tab(0, 0, AREA).expect("hide the codex tab");

    w.report("waiting", 6);

    let place = w.place();
    let sent: Vec<String> = w.told().1.into_iter().map(|n| n.text).collect();
    assert_eq!(sent, [format!("claude needs you in {place}"), format!("codex needs you in {place}")]);
}

#[test]
fn turned_off_only_the_toast_shows() {
    let mut w = Watched::start(agents::Known::Claude, true);
    w.app.config.desktop_notifications = notify::OFF.into();

    w.report("waiting", 6);

    let (shown, sent) = w.told();
    assert_eq!((shown.is_some(), sent), (true, Vec::new()));
}

#[test]
fn control_characters_never_reach_the_message() {
    let mut w = Watched::start(agents::Known::Codex, true);
    w.app.projects[0].name = Some("shop\x1b]0;evil\x07".into());

    w.report("waiting", 6);

    assert_eq!(toast(&w.app), Some("codex needs you in shop]0;evil › default"));
}

mod agents_section {
    use super::*;

    const TALL: Rect = Rect { x: 0, y: 0, width: 100, height: 30 };
    const SMALL: Rect = Rect { x: 0, y: 0, width: 80, height: 30 };

    fn statuses(app: &App) -> Vec<Option<Status>> {
        app.agents_view().entries.iter().map(|e| e.status).collect()
    }

    fn agent_pos(app: &App, area: Rect, i: usize) -> Position {
        let a = app.layout(area).shown(app.nav);
        let r = ui::agent_row(a.agents_list, a.pitch, app.agent_places().len(), app.scroll.agents, i);
        Position::new(r.x + 4, r.y + r.height / 2)
    }

    fn focus(app: &App) -> (usize, Option<u64>) {
        (app.active, app.term().map(|t| t.id))
    }

    fn two_agents() -> (App, Receiver<AppEvent>, Vec<TempDir>, FakeClaude, FakeCodex) {
        let (mut app, rx, dirs) = app_with(2);
        app.config.agents_section = true;
        app.active = 0;
        let claude = FakeClaude::running(FakeClaude::SILENT);
        claude.start(&mut app);
        pump_until(&mut app, &rx, "claude runs", |a| runs(a, agents::Known::Claude.name()));
        app.active = 1;
        let codex = FakeCodex::new("019a1234-5678-7000-8000-000000000001", "gpt-5.4", false);
        type_line(&mut app, &codex.command_line());
        watch_until(&mut app, &rx, "both agents are listed", |a| a.agent_places().len() == 2);
        (app, rx, dirs, claude, codex)
    }

    #[test]
    fn lists_every_agent_and_a_click_shows_its_pane() {
        let (mut app, rx, _dirs, claude, codex) = two_agents();
        let pane = app.layout(TALL).pane;
        let codex_pane = app.term().map(|t| t.id);
        right_click(&mut app, Position::new(pane.x + 1, pane.y + 1));
        pick(&mut app, "split right");
        app.active = 0;
        render(&mut app, TALL);
        let agents: Vec<String> = app.agents_view().entries.into_iter().map(|e| e.agent).collect();

        let at = agent_pos(&app, TALL, 1);
        click_in(&mut app, at, TALL);
        let shown = focus(&app);
        claude.signal("quit");
        codex.signal("quit", "");
        drop(rx);

        assert_eq!((agents, shown), (vec!["claude".to_string(), "codex".to_string()], (1, codex_pane)));
    }

    #[test]
    fn the_rows_follow_an_agent_as_it_starts_changes_and_quits() {
        let (mut app, rx, _dirs) = app_with(1);
        app.config.agents_section = true;
        let claude = FakeClaude::new();
        let before = statuses(&app);

        claude.start(&mut app);
        watch_until(&mut app, &rx, "claude works", |a| statuses(a) == [Some(Status::Working)]);
        claude.signal("finish");
        watch_until(&mut app, &rx, "claude goes idle", |a| statuses(a) == [Some(Status::Idle)]);
        claude.signal("quit");
        watch_until(&mut app, &rx, "the row goes", |a| statuses(a).is_empty());

        assert_eq!(before, []);
    }

    #[test]
    fn the_switch_shows_and_hides_the_section() {
        let (mut app, _rx, _dirs) = app_with(1);
        let off = text(&render(&mut app, TALL), app.layout(TALL).agents_list).trim().to_string();

        app.config.agents_section = true;
        let list = app.layout(TALL).agents_list;
        let on = text(&render(&mut app, TALL), list).trim().to_string();

        assert_eq!((off.as_str(), on.as_str()), ("", "no agents running"));
    }

    #[test]
    fn dragging_its_line_resizes_it_and_the_session_keeps_it() {
        let (mut app, _rx, _dirs) = app_with(1);
        app.config.agents_section = true;
        let line = app.layout(TALL).agents_border;
        let before = app.layout(TALL).agents.height;

        mouse_in(&mut app, MouseEventKind::Down(MouseButton::Left), Position::new(3, line.y), TALL);
        mouse_in(&mut app, MouseEventKind::Drag(MouseButton::Left), Position::new(3, line.y - 3), TALL);
        mouse_in(&mut app, MouseEventKind::Up(MouseButton::Left), Position::new(3, line.y - 3), TALL);

        assert_eq!(
            (app.layout(TALL).agents.height, app.state().widths.and_then(|w| w.agents)),
            (before + 3, Some(before + 3))
        );
    }

    #[test]
    fn the_wheel_scrolls_it() {
        let (mut app, rx, _dirs, claude, codex) = two_agents();
        let area = Rect { height: 20, ..TALL };
        let at = app.layout(area).agents_list.as_position();

        mouse_in(&mut app, MouseEventKind::ScrollDown, at, area);
        let scroll = app.scroll.agents;
        claude.signal("quit");
        codex.signal("quit", "");
        drop(rx);

        assert_eq!(scroll, 1);
    }

    #[test]
    fn compact_mode_opens_it_from_the_projects_menu_and_a_click_shows_the_pane() {
        let (mut app, rx, _dirs, claude, codex) = two_agents();
        let claude_pane = app.projects[0].workspaces[0].tabs[0].panes[0].id;
        app.nav = Some(ui::Nav::Projects);
        render(&mut app, SMALL);

        let button = app.layout(SMALL).shown(app.nav).agents_button.as_position();
        click_in(&mut app, button, SMALL);
        let nav = app.nav;
        let at = agent_pos(&app, SMALL, 0);
        click_in(&mut app, at, SMALL);
        claude.signal("quit");
        codex.signal("quit", "");
        drop(rx);

        assert_eq!((nav, app.nav, focus(&app)), (Some(ui::Nav::Agents), None, (0, Some(claude_pane))));
    }
}
