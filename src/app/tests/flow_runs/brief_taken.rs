use std::path::Path;

use super::super::flow_panel::team;
use super::*;
use crate::agents::claude::fake::FakeClaude;
use crate::agents::launch::{CONFIRM_EVERY, MAX_ENTERS};

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

    fn launching(&self) -> bool {
        self.app.agents.launches.of_pane(self.own).next().is_some()
    }

    fn lost(&self) -> usize {
        std::fs::read_to_string(self.fake.dir().join("lost")).map_or(0, |text| text.lines().count())
    }

    fn prompts(&self) -> usize {
        records(&self.fake.dir().join("projects"))
    }
}

fn pane(app: &App, id: u64) -> &Term {
    app.projects.pane_by(id).expect("the pane")
}

fn records(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                records(&path)
            } else {
                std::fs::read_to_string(&path).map_or(0, |text| text.matches("\"type\":\"user\"").count())
            }
        })
        .sum()
}

#[test]
fn a_resumed_orchestrator_whose_first_enter_is_lost_still_takes_its_brief() {
    let mut run = briefed(Some("lose"), Some(Duration::from_secs(1)));

    run.until("the brief is recorded", |app, own| pane(app, own).context.prompted().is_some());
    run.until("the launch is over", |app, own| app.agents.launches.of_pane(own).next().is_none());

    let status = pane(&run.app, run.own).agent.status();
    assert_eq!((run.lost(), run.prompts(), status), (1, 1, Some(activity::Status::Working)));
}

#[test]
fn a_resumed_orchestrator_that_takes_its_brief_at_once_gets_one_enter() {
    let mut run = briefed(None, None);

    run.until("the brief is entered", |app, own| pane(app, own).submitted.is_some());
    let first = pane(&run.app, run.own).submitted;
    run.until("the launch is over", |app, own| app.agents.launches.of_pane(own).next().is_none());

    assert_eq!((pane(&run.app, run.own).submitted, run.lost(), run.prompts()), (first, 0, 1));
}

#[test]
fn an_orchestrator_that_never_takes_its_brief_gets_enter_a_bounded_number_of_times() {
    let mut run = briefed(Some("deaf"), Some(Duration::from_millis(200)));

    run.until("the launch gives up", |app, own| app.agents.launches.of_pane(own).next().is_none());
    let lost = run.fake.dir().join("lost");
    wait_until("every Enter reached the agent", || {
        std::fs::read_to_string(&lost).is_ok_and(|text| text.lines().count() >= usize::from(MAX_ENTERS))
    });

    let term = pane(&run.app, run.own);
    assert_eq!(
        (run.lost(), term.context.prompted(), term.agent.status()),
        (usize::from(MAX_ENTERS), None, Some(activity::Status::Idle))
    );
    assert!(!run.launching());
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
