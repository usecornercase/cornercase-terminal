use super::*;
use crate::agents::AgentState;
use crate::app::flow_runs::Starting;
use crate::control::RunInfo;
use crate::flows::{Back, Flow, ORCHESTRATOR, Role, Stage, store};
use crate::paths;
use crate::project::Phase;
use crate::test_util::git_repo;

const STARTING: Duration = Duration::from_secs(30);

const FAKE: &str = r#"#!/bin/sh
log="LOG"
printf 'start %s\n' "$*" >> "$log/order"
printf 'Do you trust the files in this folder?\n❯ 1. Yes, proceed\n  2. No, exit\n'
read answer
printf '\033[2J\033[Hagent ready> '
IFS= read -r line
printf 'got %s\n' "$line" >> "$log/order"
while [ -d "$log" ]; do sleep 0.1; done
"#;

struct Team {
    app: App,
    rx: Receiver<AppEvent>,
    repo: TempDir,
    log: TempDir,
    config: TempDir,
    _worktrees: TempDir,
    _bin: TempDir,
}

fn role(name: &str, model: Option<&str>, edits: bool, prompt: &str) -> Role {
    Role {
        name: name.into(),
        agent: "claude".into(),
        model: model.map(str::to_string),
        edits,
        prompt: prompt.into(),
        ..Role::default()
    }
}

fn pair() -> Flow {
    Flow {
        name: "Pair".into(),
        notes: "Keep it small.".into(),
        orchestrator: role(ORCHESTRATOR, Some("opus"), false, "Be brief."),
        roles: vec![role("coder", None, true, "Write it."), role("reviewer", Some("sonnet"), false, "Review it.")],
        stages: vec![
            Stage { roles: vec!["coder".into()], back: None },
            Stage { roles: vec!["reviewer".into()], back: Some(Back { to: 1, when: "findings".into(), at_most: 2 }) },
        ],
        ..Flow::default()
    }
}

impl Team {
    fn new() -> Self {
        let (repo, log, config, worktrees, bin) =
            (git_repo(&[("README", "hi")]), TempDir::new(), TempDir::new(), TempDir::new(), TempDir::new());
        let fake = bin.path().join("claude");
        write_executable(&fake, &FAKE.replace("LOG", &log.path().display().to_string()));
        let settings = Config {
            worktrees_dir: worktrees.path().display().to_string(),
            agent_commands: [("claude".to_string(), fake.display().to_string())].into(),
            ..Config::default()
        };
        let config_path = write_config(config.path(), &settings);
        let flows = paths::flows(&config_path);
        std::fs::create_dir_all(&flows).expect("the flows folder");
        store::save(&flows, &pair(), None).expect("save the flow");
        let (app, rx) = app_in(repo.path(), config_path);
        Self { app, rx, repo, log, config, _worktrees: worktrees, _bin: bin }
    }

    fn run(&mut self, name: &str, branch: Option<&str>) -> Response {
        let run = wire::FlowRun {
            name: name.into(),
            prompt: "Fix the login form".into(),
            worktree: branch.map(str::to_string),
            ..wire::FlowRun::default()
        };
        ask(&mut self.app, None, Command::FlowRun(run));
        answered(&mut self.app, &self.rx, "the flow's tabs are open")
    }

    fn started(&mut self) -> RunInfo {
        match self.run("Pair", None) {
            Response::Ok(value) => serde_json::from_value(value).expect("the run"),
            Response::Error(message) => panic!("the flow did not start: {message}"),
        }
    }

    fn order(&self) -> Vec<String> {
        std::fs::read_to_string(self.log.path().join("order")).unwrap_or_default().lines().map(String::from).collect()
    }

    fn until_got(&mut self, n: usize) -> Vec<String> {
        let log = self.log.path().join("order");
        until_logged(&mut self.app, &self.rx, "every agent got its prompt", || {
            std::fs::read_to_string(&log).unwrap_or_default().lines().filter(|l| l.starts_with("got ")).count() >= n
        });
        self.order()
    }
}

fn until_logged(app: &mut App, rx: &Receiver<AppEvent>, what: &str, mut logged: impl FnMut() -> bool) {
    wait_until_within(what, STARTING, || {
        drain(app, rx);
        app.refresh(Instant::now());
        logged()
    });
}

fn position(lines: &[String], start: &str) -> usize {
    lines.iter().position(|line| line.starts_with(start)).unwrap_or_else(|| panic!("no `{start}` in {lines:#?}"))
}

#[test]
fn a_flow_opens_a_tab_per_role_in_a_new_worktree_and_leaves_the_window_as_it_is() {
    let mut team = Team::new();
    let before = team.app.focus();

    let run = team.started();

    let (p, w) = team.app.projects.workspace_index(run.workspace).expect("the run's workspace");
    let workspace = &team.app.projects[p].workspaces[w];
    let names: Vec<Option<&str>> = workspace.tabs.iter().map(|t| t.name.as_deref()).collect();
    assert_eq!(names, [Some(ORCHESTRATOR), Some("coder"), Some("reviewer")]);
    assert!(workspace.worktree);
    assert_eq!(run.branch, "fix-the-login-form");
    assert_eq!(team.app.focus(), before);
    let panes: Vec<(String, u64)> = run.members.iter().map(|m| (m.role.clone(), m.pane)).collect();
    let tabs: Vec<u64> = workspace.tabs.iter().filter_map(|t| t.pane().map(|term| term.id)).collect();
    assert_eq!(panes.iter().map(|(_, pane)| *pane).collect::<Vec<_>>(), tabs);
    assert_eq!(team.app.runs.len(), 1);
    let recorded: Vec<(String, u64)> = team.app.runs[0].members.iter().map(|m| (m.role.clone(), m.pane)).collect();
    assert_eq!((team.app.runs[0].workspace, recorded), (run.workspace, panes));
}

#[test]
fn every_agent_starts_without_asking_its_trust_is_answered_and_the_orchestrator_is_briefed_last() {
    let mut team = Team::new();
    let run = team.started();
    let pane = |role: &str| run.members.iter().find(|m| m.role == role).map(|m| m.pane).expect("the role");

    let order = team.until_got(3);

    assert!(order.contains(&"start --dangerously-skip-permissions --model opus".to_string()), "{order:#?}");
    assert!(order.contains(&"start --dangerously-skip-permissions".to_string()), "{order:#?}");
    assert!(order.contains(&"start --dangerously-skip-permissions --model sonnet".to_string()), "{order:#?}");
    let briefed = position(&order, "start --dangerously-skip-permissions --model opus");
    assert!(position(&order, "got Write it.") < briefed, "{order:#?}");
    assert!(position(&order, "got Review it.") < briefed, "{order:#?}");
    let brief = &order[position(&order, "got You are the orchestrator")];
    for part in [
        "flow \"Pair\"",
        "on branch fix-the-login-form",
        "Fix the login form",
        &format!("coder (pane {}): claude, edits files", pane("coder")),
        &format!("reviewer (pane {}): claude · sonnet, must not edit files", pane("reviewer")),
        "If it reports findings, send them to coder and go back to stage 1; at most 2 times",
        "Keep it small.",
        "Be brief.",
        &format!("cornercase mark --pane {}", pane(ORCHESTRATOR)),
    ] {
        assert!(brief.contains(part), "`{part}` is missing from {brief}");
    }
    assert!(order.iter().any(|l| l.starts_with("got Write it.") && l.contains("you may edit files")), "{order:#?}");
    assert!(order.iter().any(|l| l.starts_with("got Review it.") && l.contains("do not edit files")), "{order:#?}");
}

#[test]
fn flows_are_listed_with_their_agents_and_their_problems() {
    let mut team = Team::new();
    let mut broken = pair();
    broken.name = "Broken".into();
    broken.roles[0].agent = "vim".into();
    let project_flows = store::project_dir(team.repo.path());
    std::fs::create_dir_all(&project_flows).expect("the project's flows");
    store::save(&project_flows, &broken, None).expect("save the flow");

    let listed: wire::Flows =
        serde_json::from_value(value(&mut team.app, None, Command::FlowList(wire::FlowList::default())))
            .expect("the flows");

    let pair = listed.flows.iter().find(|f| f.name == "Pair").expect("yours");
    assert_eq!(pair.origin, "user");
    let roles: Vec<(&str, Option<&str>)> = pair.roles.iter().map(|r| (r.role.as_str(), r.model.as_deref())).collect();
    assert_eq!(roles, [(ORCHESTRATOR, Some("opus")), ("coder", None), ("reviewer", Some("sonnet"))]);
    assert!(pair.problems.is_empty(), "{:?}", pair.problems);
    let broken = listed.flows.iter().find(|f| f.name == "Broken").expect("the project's");
    assert_eq!(broken.origin, "project");
    assert!(broken.problems.iter().any(|p| p.contains("`coder` runs `vim`")), "{:?}", broken.problems);
    assert!(listed.flows.iter().any(|f| f.origin == "built-in"));
}

#[test]
fn a_flow_with_problems_an_unknown_name_and_a_branch_already_running_one_are_refused() {
    let mut team = Team::new();
    let mut broken = pair();
    broken.name = "Broken".into();
    broken.stages.clear();
    store::save(&paths::flows(&team.config.path().join("config.json")), &broken, None).expect("save");

    let refused = error(team.run("Broken", None));
    assert!(refused.starts_with("flow Broken cannot run: ") && refused.contains("the flow has no stages"), "{refused}");
    let unknown = error(team.run("Nope", None));
    assert!(unknown.contains("there is no flow named `Nope`; the flows are") && unknown.contains("Pair"), "{unknown}");

    let mut codex = pair();
    codex.name = "Codex".into();
    codex.roles[1].agent = "codex".into();
    store::save(&paths::flows(&team.config.path().join("config.json")), &codex, None).expect("save");
    team.app.config.agent_commands.insert("codex".into(), "/nonexistent/codex".into());
    let missing = error(team.run("Codex", None));
    assert!(missing.starts_with("flow Codex needs /nonexistent/codex, which this server cannot find"), "{missing}");

    let run = team.started();
    let again = error(team.run("pair", Some(&run.branch)));
    assert!(again.contains(&format!("workspace {} already runs a flow", run.workspace)), "{again}");
}

#[test]
fn two_projects_start_runs_on_branches_of_the_same_name_at_once() {
    let mut team = Team::new();
    let other = git_repo(&[("README", "hi")]);
    team.app.open_project(other.path().to_path_buf(), AREA).expect("open it");
    let run = |project: u64| {
        let prompt = "Fix the login form".into();
        Command::FlowRun(wire::FlowRun {
            name: "Pair".into(),
            prompt,
            project: Some(project),
            ..wire::FlowRun::default()
        })
    };
    let (first, second) = (team.app.projects[0].id, team.app.projects[1].id);

    ask(&mut team.app, None, run(first));
    ask(&mut team.app, None, run(second));

    assert_eq!(answers(&mut team.app), [], "both wait for git");
    let mut got = Vec::new();
    wait_until("git makes both worktrees", || {
        drain(&mut team.app, &team.rx);
        got.extend(answers(&mut team.app));
        got.len() == 2
    });
    let runs: Vec<RunInfo> = got
        .into_iter()
        .map(|answer| {
            serde_json::from_value(match answer {
                Response::Ok(value) => value,
                Response::Error(message) => panic!("a run did not start: {message}"),
            })
            .expect("a run")
        })
        .collect();
    let mut projects: Vec<u64> = runs.iter().map(|run| run.project).collect();
    projects.sort_unstable();
    assert_eq!(projects, [first, second]);
    assert!(runs.iter().all(|run| run.branch == "fix-the-login-form"));
}

#[test]
fn a_project_outside_git_cannot_run_a_flow() {
    let mut team = Team::new();
    let plain = TempDir::new();
    team.app.open_project(plain.path().to_path_buf(), AREA).expect("open it");
    let project = team.app.projects[1].id;

    let run =
        wire::FlowRun { name: "Pair".into(), prompt: "x".into(), project: Some(project), ..wire::FlowRun::default() };
    let refused = error(now(&mut team.app, None, Command::FlowRun(run)));

    assert!(refused.contains("is not the root of a git repository"), "{refused}");
}

#[test]
fn a_worktree_that_comes_back_once_its_project_is_closing_opens_nothing() {
    let mut team = Team::new();
    let project = team.app.projects[0].id;
    let run = wire::FlowRun { name: "Pair".into(), prompt: "Fix the login form".into(), ..wire::FlowRun::default() };
    ask(&mut team.app, None, Command::FlowRun(run));
    assert_eq!(answers(&mut team.app), [], "git is still making the worktree");

    team.app.close_project(project);
    let mut got = Vec::new();
    wait_until("git makes the worktree", || {
        for event in team.rx.try_iter().filter(|event| matches!(event, AppEvent::Worktree(_))) {
            team.app.handle_event(event, AREA).expect("the answer is handled");
        }
        got = answers(&mut team.app);
        !got.is_empty()
    });

    let refused = error(got.remove(0));
    assert_eq!(refused, format!("project {project} is closing"));
    assert!(team.app.runs.is_empty());
    let (p, _) = team.app.projects.workspace_index(team.app.projects[0].workspaces[0].id).expect("still closing");
    assert!(team.app.projects[p].workspaces.iter().all(|w| w.tabs.len() <= 1), "a tab opened in a closing project");
}

#[rstest::rstest]
#[case::a_workspace_being_removed(Some(Phase::Removing("main".into())), false, "is being removed")]
#[case::a_closing_workspace(Some(Phase::Closing), false, "is closing")]
#[case::a_closing_project(None, true, "is closing")]
fn a_run_never_opens_in_a_workspace_or_project_that_is_going(
    #[case] phase: Option<Phase>,
    #[case] closing: bool,
    #[case] said: &str,
) {
    let mut team = Team::new();
    if let Some(phase) = phase {
        team.app.projects[0].workspaces[0].phase = phase;
    }
    team.app.projects[0].closing = closing;
    let starting = Starting { flow: pair(), task: "Fix it".into(), branch: "main".into(), focus: false };

    let refused = team.app.open_run(0, 0, starting, AREA).expect_err("refused");

    assert!(refused.contains(said), "{refused}");
    assert_eq!((team.app.projects[0].workspaces[0].tabs.len(), team.app.runs.len()), (1, 0));
}

#[test]
fn the_run_ends_with_its_workspace() {
    let mut team = Team::new();
    let run = team.started();

    let close = wire::Close { item: Item::Workspace(run.workspace), remove_worktree: false, force: false };
    done(now(&mut team.app, None, Command::Close(close)));

    refreshing(&mut team.app, &team.rx, "the run goes with its workspace", |a| a.runs.is_empty());
}

#[test]
fn a_run_is_saved_and_comes_back_with_its_roles_resumed_and_the_orchestrator_told_its_new_panes() {
    let mut team = Team::new();
    team.started();
    team.until_got(3);
    let mut saved = team.app.state();
    let workspace = saved.projects[0].workspaces.iter_mut().find(|w| w.run.is_some()).expect("the run is saved");
    let state = workspace.run.as_ref().expect("the run");
    let seats: Vec<(&str, usize, usize)> = state.members.iter().map(|s| (s.role.as_str(), s.tab, s.pane)).collect();
    assert_eq!(seats, [(ORCHESTRATOR, 0, 0), ("coder", 1, 0), ("reviewer", 2, 0)]);
    assert_eq!((state.flow.name.as_str(), state.task.as_str()), ("Pair", "Fix the login form"));
    for (i, tab) in workspace.tabs.iter_mut().enumerate() {
        tab.panes[0].agent = Some(AgentState { kind: "claude".into(), conversation: format!("c{i}"), mode: None });
    }
    std::fs::write(team.log.path().join("order"), "").expect("empty the log");
    let (mut again, rx) = new_app(team.config.path().join("config.json"));

    assert!(again.restore(&saved, AREA));

    assert_eq!(again.runs.len(), 1);
    let (p, w) = again.projects.workspace_index(again.runs[0].workspace).expect("the workspace");
    let tabs: Vec<(Option<String>, u64)> = again.projects[p].workspaces[w]
        .tabs
        .iter()
        .map(|t| (t.name.clone(), t.pane().map_or(0, |term| term.id)))
        .collect();
    let members: Vec<(Option<String>, u64)> =
        again.runs[0].members.iter().map(|m| (Some(m.role.clone()), m.pane)).collect();
    assert_eq!(members, tabs);
    let log = team.log.path().join("order");
    until_logged(&mut again, &rx, "the orchestrator is told", || {
        std::fs::read_to_string(&log).unwrap_or_default().lines().any(|l| l.starts_with("got cornercase restarted"))
    });
    let order = team.order();
    assert!(order.contains(&"start --dangerously-skip-permissions --resume c1".to_string()), "{order:#?}");
    assert!(order.contains(&"start --dangerously-skip-permissions --model sonnet --resume c2".to_string()));
    assert!(order.contains(&"start --dangerously-skip-permissions --model opus --resume c0".to_string()));
    let told = &order[position(&order, "got cornercase restarted")];
    let coder = again.runs[0].pane_of("coder").expect("the coder");
    assert!(told.contains(&format!("coder is pane {coder}")), "{told}");
}
