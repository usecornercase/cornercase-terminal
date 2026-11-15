use super::super::flow_panel::team;
use super::*;
use crate::agents::Known;
use crate::agents::claude::fake::FakeClaude;
use crate::agents::launch::{self, CONFIRM_EVERY, Launch, MAX_ENTERS};

struct Briefed {
    app: App,
    rx: Receiver<AppEvent>,
    fake: FakeClaude,
    own: u64,
    _dir: TempDir,
}

fn briefed(signal: Option<&str>, every: Option<Duration>) -> Briefed {
    let (mut app, rx, dir) = team();
    let fake = FakeClaude::running(FakeClaude::CONTROL);
    if let Some(name) = signal {
        fake.signal(name);
    }
    app.agents.set_claude_dir(fake.dir());
    let run = app.runs.all()[0].clone();
    let own = run.pane_of(ORCHESTRATOR).expect("the orchestrator");
    let until = Some(Instant::now() + RESUME_GRACE);
    app.launch_briefed(&run, (own, until), fake.command_line(""), "go on with the plan".into());
    for launch in app.agents.launches.of_pane_mut(own) {
        launch.confirm = every.or(launch.confirm);
    }
    Briefed { app, rx, fake, own, _dir: dir }
}

impl Briefed {
    fn until(&mut self, what: &str, cond: impl Fn(&App, u64) -> bool) {
        let own = self.own;
        refreshing(&mut self.app, &self.rx, what, |app| cond(app, own));
    }

    fn lost_reaches(&self, enters: usize) {
        wait_until("the Enter reached the agent", || self.lost() >= enters);
    }

    fn launching(&self) -> bool {
        self.app.agents.launches.of_pane(self.own).next().is_some()
    }

    fn lost(&self) -> usize {
        std::fs::read_to_string(self.fake.dir().join("lost")).map_or(0, |text| text.lines().count())
    }

    fn pastes(&self) -> String {
        std::fs::read_to_string(self.fake.dir().join("pastes")).unwrap_or_default()
    }

    fn prompts(&self) -> usize {
        let Some(context::Record::Claude(path)) = pane(&self.app, self.own).context.record_for(Known::Claude) else {
            return 0;
        };
        std::fs::read_to_string(path).map_or(0, |text| text.matches("\"type\":\"user\"").count())
    }
}

fn pane(app: &App, id: u64) -> &Term {
    app.projects.pane_by(id).expect("the pane")
}

#[test]
fn a_resumed_orchestrator_whose_first_enter_is_lost_still_takes_its_brief() {
    let mut run = briefed(Some("lose"), Some(Duration::from_secs(1)));

    run.until("the brief is recorded", |app, own| pane(app, own).context.prompted().is_some());
    run.until("the agent works", |app, own| pane(app, own).agent.status() == Some(activity::Status::Working));
    run.until("the launch is over", |app, own| app.agents.launches.of_pane(own).next().is_none());

    assert_eq!((run.lost(), run.prompts()), (1, 1));
}

#[test]
fn a_resumed_orchestrator_waits_for_bracketed_paste_before_its_brief() {
    let mut run = briefed(Some("late-paste"), None);

    run.until("the brief is recorded", |app, own| pane(app, own).context.prompted().is_some());
    run.until("the launch is over", |app, own| app.agents.launches.of_pane(own).next().is_none());

    assert_eq!((run.lost(), run.prompts(), run.pastes().as_str()), (0, 1, "bracketed\n"));
}

#[test]
fn a_brief_waits_until_its_agent_draws_its_input_box() {
    let mut run = briefed(Some("late-box"), None);

    run.until("the brief is recorded", |app, own| pane(app, own).context.prompted().is_some());
    run.until("the launch is over", |app, own| app.agents.launches.of_pane(own).next().is_none());

    assert_eq!((run.lost(), run.prompts(), run.pastes().as_str()), (0, 1, "bracketed\n"));
}

#[test]
fn a_fresh_brief_whose_first_enter_is_lost_gets_enter_again() {
    let (mut app, rx, _dir) = team();
    let fake = FakeClaude::running(FakeClaude::CONTROL);
    fake.signal("lose");
    app.agents.set_claude_dir(fake.dir());
    let run = app.runs.all()[0].clone();
    let role = run.flow.orchestrator.clone();
    let own = run.pane_of(ORCHESTRATOR).expect("the orchestrator");
    let spec = launch::Spec { command: fake.command_line(""), ..app.flow_spec(&role, "build the form".into(), None) };
    let mut launch = Launch::new(own, spec, Instant::now());
    assert_eq!(launch.confirm, Some(CONFIRM_EVERY));
    launch.confirm = Some(Duration::from_secs(1));
    app.agents.launches.add(launch);

    refreshing(&mut app, &rx, "the brief is recorded", |app| pane(app, own).context.prompted().is_some());
    refreshing(&mut app, &rx, "the launch is over", |app| app.agents.launches.of_pane(own).next().is_none());

    let lost = std::fs::read_to_string(fake.dir().join("lost")).map_or(0, |text| text.lines().count());
    assert_eq!(lost, 1);
}

#[test]
fn a_resumed_orchestrator_that_takes_its_brief_at_once_gets_one_enter() {
    let mut run = briefed(None, None);

    run.until("the brief is entered", |app, own| pane(app, own).submitted.is_some());
    let first = pane(&run.app, run.own).submitted;
    run.until("the brief is recorded", |app, own| pane(app, own).context.prompted().is_some());
    run.until("the launch is over", |app, own| app.agents.launches.of_pane(own).next().is_none());

    assert_eq!((pane(&run.app, run.own).submitted, run.lost(), run.prompts()), (first, 0, 1));
}

#[test]
fn an_orchestrator_that_never_takes_its_brief_gets_enter_a_bounded_number_of_times() {
    let mut run = briefed(Some("deaf"), None);
    let max = usize::from(MAX_ENTERS);

    run.until("the first Enter is pressed", |app, own| {
        app.agents.launches.of_pane(own).any(|l| l.stage() == "confirm")
    });
    let base = Instant::now();
    run.lost_reaches(1);
    for enters in 2..=max {
        run.app.drive_launches(base + CONFIRM_EVERY * u32::try_from(enters - 1).expect("a small count"));
        run.lost_reaches(enters);
    }
    assert!(run.launching());
    run.app.drive_launches(base + CONFIRM_EVERY * u32::from(MAX_ENTERS));

    let term = pane(&run.app, run.own);
    assert_eq!((run.lost(), term.context.prompted(), term.agent.status()), (max, None, Some(activity::Status::Idle)));
    assert!(!run.launching());
    let toast = run.app.messages.toast.as_ref().map(|toast| toast.message.clone()).unwrap_or_default();
    assert!(toast.contains("did not take its prompt"), "{toast}");
}

#[test]
fn a_restored_orchestrator_confirms_its_brief_and_its_team_does_not() {
    let (app, _rx, _dir) = team();
    let mut saved = app.state();
    let workspace = saved.projects[0].workspaces.iter_mut().find(|w| w.run.is_some()).expect("the run is saved");
    let seats: Vec<(usize, usize)> =
        workspace.run.as_ref().expect("the run").members.iter().map(|s| (s.tab, s.pane)).collect();
    for (tab, pane) in seats {
        let agent = AgentState { kind: "claude".into(), conversation: format!("c-{tab}"), mode: None, account: None };
        workspace.tabs[tab].panes[pane].agent = Some(agent);
    }
    let (mut again, _rx) = new_app(no_config());

    assert!(again.restore(&saved, AREA));

    let run = &again.runs.all()[0];
    let confirms = |pane: u64| again.agents.launches.of_pane(pane).next().map(|launch| launch.confirm);
    let orchestrator = run.pane_of(ORCHESTRATOR).expect("the orchestrator");
    assert_eq!(confirms(orchestrator), Some(Some(CONFIRM_EVERY)));
    for member in run.members.iter().filter(|m| m.pane != orchestrator) {
        assert_eq!(confirms(member.pane), Some(None), "{} is only resumed", member.role);
    }
}
