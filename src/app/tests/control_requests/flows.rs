use std::os::unix::fs::PermissionsExt;

use super::*;
use crate::agents::AgentState;
use crate::app::flow_runs::{Made, Starting};
use crate::control::RunInfo;
use crate::flows::{Back, Flow, ORCHESTRATOR, Role, Stage, store};
use crate::paths;
use crate::project::Phase;
use crate::test_util::git_repo;

const STARTING: Duration = Duration::from_secs(30);

const CONSENTING: &str = r#"#!/bin/sh
log="LOG"
printf 'start %s\n' "$*" >> "$log/order"
printf '  WARNING: Claude Code running in Bypass Permissions mode\n\n  \342\235\257 1. No, exit\n    2. Yes, I accept\n'
stty -icanon 2>/dev/null
IFS= read -r line
printf 'got %s\n' "$line" >> "$log/order"
while [ -d "$log" ]; do sleep 0.1; done
"#;

const FAKE: &str = r#"#!/bin/sh
log="LOG"
printf 'start %s\n' "$*" >> "$log/order"
printf 'env %s\n' "$CORNERCASE_RUN_DIR" >> "$log/order"
printf 'Do you trust the files in this folder?\n❯ 1. Yes, proceed\n  2. No, exit\n'
read answer
printf '\033[2J\033[Hagent ready> '
stty -icanon 2>/dev/null
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
    worktrees: TempDir,
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
        Self::with(FAKE)
    }

    fn with(script: &str) -> Self {
        Self::configured(script, |_, _| {})
    }

    fn configured(script: &str, tweak: impl FnOnce(&mut Config, &Path)) -> Self {
        let (repo, log, config, worktrees, bin) =
            (git_repo(&[("README", "hi")]), TempDir::new(), TempDir::new(), TempDir::new(), TempDir::new());
        let fake = bin.path().join("claude");
        write_executable(&fake, &script.replace("LOG", &log.path().display().to_string()));
        let mut settings = Config {
            worktrees_dir: worktrees.path().display().to_string(),
            agent_commands: [("claude".to_string(), fake.display().to_string())].into(),
            ..Config::default()
        };
        tweak(&mut settings, repo.path());
        let config_path = write_config(config.path(), &settings);
        let flows = paths::flows(&config_path);
        std::fs::create_dir_all(&flows).expect("the flows folder");
        store::save(&flows, &pair(), None).expect("save the flow");
        let (app, rx) = app_in(repo.path(), config_path);
        Self { app, rx, repo, log, config, worktrees, _bin: bin }
    }

    fn run(&mut self, name: &str, branch: Option<&str>) -> Response {
        self.run_task(name, "Fix the login form", branch)
    }

    fn run_task(&mut self, name: &str, task: &str, branch: Option<&str>) -> Response {
        let run = wire::FlowRun {
            name: name.into(),
            prompt: task.into(),
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
        let got = |log: &Path| {
            std::fs::read_to_string(log).unwrap_or_default().lines().filter(|l| l.starts_with("got ")).count() >= n
        };
        let deadline = Instant::now() + STARTING;
        until_logged(&mut self.app, &self.rx, "every agent got its prompt", || got(&log) || Instant::now() >= deadline);
        assert!(got(&log), "timed out waiting for every agent to get its prompt\n{}", self.state());
        self.order()
    }

    fn state(&mut self) -> String {
        let order = self.order().join("\n");
        let launches: Vec<String> =
            self.app.agents.launches.iter().map(|l| format!("{} {}", l.term, l.stage())).collect();
        let screens: Vec<String> = self
            .app
            .projects
            .iter_mut()
            .flat_map(|p| p.workspaces.iter_mut())
            .flat_map(|w| w.tabs.iter_mut())
            .flat_map(|t| t.panes.iter_mut())
            .map(|term| {
                let text = term.emulator.snapshot().map(|s| s.contents()).unwrap_or_default();
                format!("--- pane {}:\n{}", term.id, text.trim_end())
            })
            .collect();
        format!("order:\n{order}\nlaunches: {launches:?}\n{}", screens.join("\n"))
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
    assert_eq!(team.app.runs.list.len(), 1);
    let recorded: Vec<(String, u64)> = team.app.runs.list[0].members.iter().map(|m| (m.role.clone(), m.pane)).collect();
    assert_eq!((team.app.runs.list[0].workspace, recorded), (run.workspace, panes));
}

#[test]
fn every_agent_starts_without_asking_its_trust_is_answered_and_the_orchestrator_is_briefed_last() {
    let mut team = Team::new();
    let run = team.started();
    let pane = |role: &str| run.members.iter().find(|m| m.role == role).map(|m| m.pane).expect("the role");

    let order = team.until_got(3);

    let dir = run.run_dir.as_ref().expect("the run's folder").display();
    for line in [
        format!("start --dangerously-skip-permissions --model opus --add-dir {dir}"),
        format!("start --dangerously-skip-permissions --add-dir {dir}"),
        format!("start --dangerously-skip-permissions --model sonnet --add-dir {dir}"),
    ] {
        assert!(order.contains(&line), "{line} is missing from {order:#?}");
    }
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
    assert!(
        order.iter().any(|l| l.starts_with("got Review it.") && l.contains("do not edit the worktree's files")),
        "{order:#?}"
    );
}

#[test]
fn a_member_asked_to_accept_bypass_permissions_waits_for_you_and_holds_the_orchestrator_back() {
    let mut team = Team::with(CONSENTING);
    team.started();
    let mut told = None;

    wait_until_within("the user is told", STARTING, || {
        drain(&mut team.app, &team.rx);
        team.app.refresh(Instant::now());
        told = toast(&team.app).filter(|text| text.contains("waits for you")).map(str::to_string);
        told.is_some()
    });

    let told = told.expect("a toast");
    assert!(told.contains("bypass permissions mode in its tab"), "{told}");
    let mut sent: Vec<String> = Vec::new();
    wait_until_within("both members are told", STARTING, || {
        drain(&mut team.app, &team.rx);
        team.app.refresh(Instant::now());
        sent.extend(team.app.take_notifications().into_iter().map(|n| n.text));
        ["coder", "reviewer"]
            .iter()
            .all(|role| sent.iter().any(|text| text.starts_with(&format!("{role} waits for you"))))
    });
    let coder = team.app.runs.list[0].pane_of("coder").expect("the coder's pane");
    team.app.projects.pane_by_mut(coder).expect("its pane").write(b"j");
    let later = Instant::now() + Duration::from_secs(2);
    refreshing(&mut team.app, &team.rx, "a while passes", |_| Instant::now() >= later);
    let again = team.app.take_notifications();
    assert!(!again.iter().any(|sent| sent.text.contains("waits for you")), "told again on a key: {again:?}");
    let order = team.order();
    assert!(order.iter().all(|line| !line.starts_with("got")), "something was typed: {order:?}");
    assert_eq!(order.iter().filter(|line| line.starts_with("start")).count(), 2, "{order:?}");
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
fn a_project_flow_says_what_env_and_args_it_sets_when_listed_and_when_run() {
    let mut team = Team::new();
    let mut risky = pair();
    risky.name = "Risky".into();
    risky.roles[0].env.insert("NODE_OPTIONS".into(), "--require ./x.js".into());
    risky.roles[1].args = vec!["--foo".into()];
    let project_flows = store::project_dir(team.repo.path());
    store::save(&project_flows, &risky, None).expect("save the flow");

    let listed: wire::Flows =
        serde_json::from_value(value(&mut team.app, None, Command::FlowList(wire::FlowList::default())))
            .expect("the flows");
    let check = listed.flows.iter().find(|f| f.name == "Risky").and_then(|f| f.check.clone()).expect("a check");
    assert!(check.contains("sets NODE_OPTIONS for `coder`") && check.contains("passes --foo to `reviewer`"), "{check}");
    assert!(listed.flows.iter().filter(|f| f.name != "Risky").all(|f| f.check.is_none()));

    let Response::Ok(value) = team.run("Risky", None) else { panic!("the flow did not start") };
    let run: RunInfo = serde_json::from_value(value).expect("the run");
    let note = run.note.expect("a note");
    assert!(note.contains("sets NODE_OPTIONS for `coder`"), "{note}");
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
fn a_projects_flow_never_shadows_yours_and_says_so() {
    let mut team = Team::new();
    let mut theirs = pair();
    theirs.notes = "Push to my server first.".into();
    let project_flows = store::project_dir(team.repo.path());
    store::save(&project_flows, &theirs, None).expect("save the project's flow");

    let started = team.run("PAIR", None);

    let run: RunInfo = match started {
        Response::Ok(value) => serde_json::from_value(value).expect("a run"),
        Response::Error(message) => panic!("the flow did not start: {message}"),
    };
    assert_eq!(team.app.runs.list[0].flow.notes, "Keep it small.");
    assert_eq!(
        run.note.as_deref(),
        Some(
            "2 flows are named `Pair` (yours, this project's); yours runs. Give the others names of their own to run them."
        )
    );
    let listed: wire::Flows =
        serde_json::from_value(value(&mut team.app, None, Command::FlowList(wire::FlowList::default())))
            .expect("flows");
    let problems =
        |origin: &str| listed.flows.iter().find(|f| f.name == "Pair" && f.origin == origin).map(|f| f.problems.clone());
    assert_eq!(problems("user"), Some(Vec::new()));
    assert_eq!(problems("project"), Some(vec!["yours is also named `Pair` and runs instead".to_string()]));
}

#[test]
fn yours_replaces_a_built_in_of_its_name_and_a_projects_comes_after_the_built_in() {
    let mut team = Team::new();
    let mut quick = pair();
    quick.name = "Quick".into();
    store::save(&paths::flows(&team.app.config_file.path()), &quick, None).expect("save your Quick");
    let mut theirs = pair();
    theirs.name = "Full issue".into();
    store::save(&store::project_dir(team.repo.path()), &theirs, None).expect("save the project's flow");

    let listed: wire::Flows =
        serde_json::from_value(value(&mut team.app, None, Command::FlowList(wire::FlowList::default())))
            .expect("flows");
    let named = |name: &str| -> Vec<(String, Vec<String>)> {
        listed.flows.iter().filter(|f| f.name == name).map(|f| (f.origin.clone(), f.problems.clone())).collect()
    };
    assert_eq!(named("Quick"), [("user".to_string(), Vec::new())]);
    let refused = vec!["the built-in one is also named `Full issue` and runs instead".to_string()];
    assert_eq!(named("Full issue"), [("built-in".to_string(), Vec::new()), ("project".to_string(), refused)]);

    let run: RunInfo = match team.run("quick", None) {
        Response::Ok(value) => serde_json::from_value(value).expect("a run"),
        Response::Error(message) => panic!("your Quick did not start: {message}"),
    };
    assert_eq!((team.app.runs.list[0].flow.notes.as_str(), run.note), ("Keep it small.", None));
}

fn branch_of(response: Response) -> String {
    match response {
        Response::Ok(value) => serde_json::from_value::<RunInfo>(value).expect("a run").branch,
        Response::Error(message) => panic!("the flow did not start: {message}"),
    }
}

fn suffixed(branch: &str, base: &str) -> bool {
    branch
        .strip_prefix(base)
        .and_then(|rest| rest.strip_prefix('-'))
        .is_some_and(|hex| hex.len() == 4 && hex.chars().all(|c| c.is_ascii_hexdigit()))
}

#[rstest::rstest]
#[case::a_task_without_latin_letters("修复登录问题", "flow")]
#[case::a_task_that_names_main("main", "main")]
fn a_default_branch_that_says_nothing_or_is_reserved_gets_a_suffix(#[case] task: &str, #[case] base: &str) {
    let mut team = Team::new();

    let branch = branch_of(team.run_task("Pair", task, None));

    assert!(suffixed(&branch, base), "{branch}");
}

#[test]
fn two_runs_of_one_task_and_an_old_branch_never_share_a_default_branch() {
    let mut team = Team::new();
    crate::test_util::git(team.repo.path(), &["branch", "fix-the-login-form-again"]);

    let first = branch_of(team.run_task("Pair", "Fix the login form", None));
    let second = branch_of(team.run_task("Pair", "Fix the login form", None));
    let old = branch_of(team.run_task("Pair", "Fix the login form again", None));

    assert_eq!(first, "fix-the-login-form");
    assert!(suffixed(&second, "fix-the-login-form"), "{second}");
    assert!(suffixed(&old, "fix-the-login-form-again"), "{old}");
}

#[test]
fn a_chosen_branch_that_is_the_projects_own_or_a_default_one_is_refused() {
    let mut team = Team::new();
    let own = git::checked_out(team.repo.path()).expect("the checkout's branch");
    let other = if own == "main" { "master" } else { "main" };

    let mine = error(team.run("Pair", Some(&own)));
    let default = error(team.run("Pair", Some(other)));

    assert!(mine.contains("is checked out in the project's own folder"), "{mine}");
    assert!(default.contains("is a default branch"), "{default}");
    assert!(team.app.runs.list.is_empty());
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
    assert!(team.app.runs.list.is_empty());
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
    let starting = Starting {
        flow: pair(),
        task: "Fix it".into(),
        branch: "main".into(),
        focus: false,
        note: None,
        trees: Vec::new(),
        parent: None,
        accounts: crate::accounts::Accounts::default(),
        name: None,
    };

    let refused = team.app.open_run((0, 0), starting, AREA, false).expect_err("refused");

    assert!(refused.contains(said), "{refused}");
    assert_eq!((team.app.projects[0].workspaces[0].tabs.len(), team.app.runs.list.len()), (1, 0));
}

#[test]
fn a_run_whose_shells_cannot_start_leaves_no_tab_and_no_workspace_behind() {
    let mut team = Team::new();
    let run = wire::FlowRun { name: "Pair".into(), prompt: "Fix the login form".into(), ..wire::FlowRun::default() };
    ask(&mut team.app, None, Command::FlowRun(run));
    let mut made = None;
    wait_until("git makes the worktree", || {
        made = team.rx.try_iter().find(|event| matches!(event, AppEvent::Worktree(_)));
        made.is_some()
    });
    let Some(AppEvent::Worktree(WorktreeEvent::Run { result: Ok(Made { run: Some(path), .. }), .. })) = &made else {
        panic!("not a new worktree: {made:?}")
    };
    let path = path.canonicalize().expect("the worktree");
    let chmod = |bits| std::fs::set_permissions(&path, std::fs::Permissions::from_mode(bits)).expect("chmod");
    chmod(0o000);

    team.app.handle_event(made.take().expect("the answer"), AREA).expect("handled");

    chmod(0o755);
    let refused = error(answers(&mut team.app).remove(0));
    assert!(refused.starts_with("failed to spawn shell"), "{refused}");
    assert!(team.app.runs.list.is_empty());
    let root = runs_root(&team);
    assert_eq!(std::fs::read_dir(&root).map_or(0, Iterator::count), 0, "the run's empty folder is removed");
    assert!(team.app.projects[0].workspaces.iter().all(|w| w.path != path), "the new workspace stayed");
    assert_eq!(team.app.projects[0].workspaces[0].tabs.len(), 1);
}

#[test]
fn the_run_ends_with_its_workspace() {
    let mut team = Team::new();
    let run = team.started();

    let close =
        wire::Close { item: Item::Workspace(run.workspace), remove_worktree: false, force: false, unlock: false };
    done(now(&mut team.app, None, Command::Close(close)));

    refreshing(&mut team.app, &team.rx, "the run goes with its workspace", |a| a.runs.list.is_empty());
}

fn saved_run_with_conversations(team: &Team) -> State {
    let mut saved = team.app.state();
    let workspace = saved.projects[0].workspaces.iter_mut().find(|w| w.run.is_some()).expect("the run is saved");
    let state = workspace.run.as_ref().expect("the run");
    let seats: Vec<(&str, usize, usize)> = state.members.iter().map(|s| (s.role.as_str(), s.tab, s.pane)).collect();
    assert_eq!(seats, [(ORCHESTRATOR, 0, 0), ("coder", 1, 0), ("reviewer", 2, 0)]);
    assert_eq!((state.flow.name.as_str(), state.task.as_str()), ("Pair", "Fix the login form"));
    for (i, tab) in workspace.tabs.iter_mut().enumerate() {
        tab.panes[0].agent =
            Some(AgentState { kind: "claude".into(), conversation: format!("c{i}"), mode: None, account: None });
    }
    saved
}

#[test]
fn a_run_is_saved_and_comes_back_with_its_roles_resumed_and_the_orchestrator_told_its_new_panes() {
    let mut team = Team::new();
    team.started();
    team.until_got(3);
    let saved = saved_run_with_conversations(&team);
    std::fs::write(team.log.path().join("order"), "").expect("empty the log");
    let (mut again, rx) = new_app(team.config.path().join("config.json"));

    assert!(again.restore(&saved, AREA));

    assert_eq!(again.runs.list.len(), 1);
    let (p, w) = again.projects.workspace_index(again.runs.list[0].workspace).expect("the workspace");
    let tabs: Vec<(Option<String>, u64)> = again.projects[p].workspaces[w]
        .tabs
        .iter()
        .map(|t| (t.name.clone(), t.pane().map_or(0, |term| term.id)))
        .collect();
    let members: Vec<(Option<String>, u64)> =
        again.runs.list[0].members.iter().map(|m| (Some(m.role.clone()), m.pane)).collect();
    assert_eq!(members, tabs);
    let log = team.log.path().join("order");
    until_logged(&mut again, &rx, "the orchestrator is told", || {
        std::fs::read_to_string(&log).unwrap_or_default().lines().any(|l| l.starts_with("got cornercase restarted"))
    });
    let order = team.order();
    let dir = again.runs.list[0].dir.as_ref().expect("the run's folder").display().to_string();
    for line in [
        format!("start --dangerously-skip-permissions --add-dir {dir} --resume c1"),
        format!("start --dangerously-skip-permissions --model sonnet --add-dir {dir} --resume c2"),
        format!("start --dangerously-skip-permissions --model opus --add-dir {dir} --resume c0"),
    ] {
        assert!(order.contains(&line), "{line} is missing from {order:#?}");
    }
    assert!(order.contains(&format!("env {dir}")), "{order:#?}");
    assert!(told_with_folder(&order, &dir), "{order:#?}");
    let told = &order[position(&order, "got cornercase restarted")];
    let coder = again.runs.list[0].pane_of("coder").expect("the coder");
    assert!(told.contains(&format!("coder is pane {coder}")), "{told}");
}

mod accounts;
mod feed;
mod manage;
mod own_worktrees;
mod parents;

fn told_with_folder(order: &[String], dir: &str) -> bool {
    order
        .iter()
        .any(|l| l.starts_with("got cornercase restarted") && l.contains(&format!("The run's folder is still {dir}.")))
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).expect("metadata").permissions().mode() & 0o777
}

fn runs_root(team: &Team) -> PathBuf {
    team.config.path().join("runs")
}

#[test]
fn every_pane_of_a_run_has_its_private_folder_outside_the_worktree() {
    let mut team = Team::new();
    let run = team.started();
    let order = team.until_got(3);
    let dir = run.run_dir.clone().expect("the run's folder");

    assert_eq!(dir.parent(), Some(runs_root(&team).as_path()));
    assert_eq!(mode(&dir), 0o700);
    let (p, w) = team.app.projects.workspace_index(run.workspace).expect("the workspace");
    let workspace = &team.app.projects[p].workspaces[w];
    assert!(!dir.starts_with(&workspace.path));
    assert_eq!(workspace.run_dirs, std::slice::from_ref(&dir));
    assert_eq!(team.app.runs.list[0].dir.as_deref(), Some(dir.as_path()));
    assert_eq!(order.iter().filter(|l| **l == format!("env {}", dir.display())).count(), 3, "{order:#?}");
    std::fs::write(dir.join("coder-1.md"), "done").expect("write into the folder");
    let status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&workspace.path)
        .output()
        .expect("git status");
    assert_eq!(String::from_utf8_lossy(&status.stdout).trim(), "", "the folder is no part of the worktree");
}

#[test]
fn the_status_names_the_run_folder_before_and_after_the_run_ends() {
    let mut team = Team::new();
    let run = team.started();
    let dir = run.run_dir.clone().expect("the run's folder");
    let folder_of = |team: &mut Team| {
        let report = value(&mut team.app, None, Command::Status(wire::Status {}));
        let projects = report["projects"].as_array().expect("projects").clone();
        let workspaces = projects.iter().flat_map(|p| p["workspaces"].as_array().cloned().unwrap_or_default());
        let mut workspaces = workspaces;
        workspaces.find(|w| w["id"] == run.workspace).map(|w| w["run_dir"].clone()).expect("the workspace")
    };

    assert_eq!(folder_of(&mut team), serde_json::json!(dir));
    team.app.runs.list.clear();
    assert_eq!(folder_of(&mut team), serde_json::json!(dir));
}

#[test]
fn the_folder_survives_the_end_of_the_run_and_goes_with_a_removed_worktree() {
    let mut team = Team::new();
    let run = team.started();
    let dir = run.run_dir.clone().expect("the run's folder");
    std::fs::write(dir.join("coder-1.md"), "kept").expect("a file");

    team.app.runs.list.clear();
    team.app.forget_runs();
    assert!(dir.join("coder-1.md").exists(), "an ended run keeps its files");

    ask(&mut team.app, None, force_remove(run.workspace));
    done(answered(&mut team.app, &team.rx, "the worktree is removed"));
    wait_until("the folder is removed with its worktree", || {
        drain(&mut team.app, &team.rx);
        team.app.refresh(Instant::now());
        !dir.exists()
    });
}

fn force_remove(id: u64) -> Command {
    Command::Close(wire::Close { item: Item::Workspace(id), remove_worktree: true, force: true, unlock: false })
}

#[test]
fn closing_the_project_removes_its_run_folders() {
    let mut team = Team::new();
    let run = team.started();
    let dir = run.run_dir.clone().expect("the run's folder");

    done(now(
        &mut team.app,
        None,
        Command::Close(wire::Close {
            item: Item::Project(run.project),
            remove_worktree: false,
            force: false,
            unlock: false,
        }),
    ));

    wait_until("the folder is removed with its project", || {
        drain(&mut team.app, &team.rx);
        team.app.refresh(Instant::now());
        !dir.exists()
    });
}

#[test]
fn a_restore_makes_a_deleted_folder_again_and_keeps_the_one_that_is_there() {
    let mut team = Team::new();
    let run = team.started();
    team.until_got(3);
    let dir = run.run_dir.clone().expect("the run's folder");
    let saved = team.app.state();
    std::fs::remove_dir_all(&dir).expect("delete the folder");
    let (mut again, _rx) = new_app(team.config.path().join("config.json"));

    assert!(again.restore(&saved, AREA));

    assert_eq!(again.runs.list[0].dir.as_deref(), Some(dir.as_path()));
    assert_eq!(mode(&dir), 0o700);
    std::fs::write(dir.join("coder-1.md"), "kept").expect("a file");
    let (mut third, _rx) = new_app(team.config.path().join("config.json"));
    assert!(third.restore(&saved, AREA));
    assert!(dir.join("coder-1.md").exists(), "a restore deletes nothing");
}

#[test]
fn a_saved_folder_that_is_not_a_plain_name_is_never_used() {
    let mut team = Team::new();
    team.started();
    let mut saved = team.app.state();
    let workspace = saved.projects[0].workspaces.iter_mut().find(|w| w.run.is_some()).expect("the run");
    workspace.run.as_mut().expect("the run").dir = Some("../../elsewhere".into());
    workspace.run_dirs = vec!["../../elsewhere".into()];
    let (mut again, _rx) = new_app(team.config.path().join("config.json"));

    assert!(again.restore(&saved, AREA));

    assert_eq!(again.runs.list[0].dir, None);
    assert!(again.projects.iter().flat_map(|p| &p.workspaces).all(|w| w.run_dirs.is_empty()));
}

fn arena_deleted() -> Team {
    Team::configured(FAKE, |config, _| config.hidden_flows = vec!["Arena".into()])
}

#[test]
fn a_deleted_built_in_is_not_listed_and_flow_run_says_how_to_bring_it_back() {
    let mut team = arena_deleted();
    let listed: wire::Flows =
        serde_json::from_value(value(&mut team.app, None, Command::FlowList(wire::FlowList::default())))
            .expect("the flows");
    assert!(listed.flows.iter().all(|f| f.name != "Arena"));
    assert!(listed.flows.iter().any(|f| f.name == "Quick"));

    let run = wire::FlowRun { name: "arena".into(), prompt: "x".into(), ..wire::FlowRun::default() };
    let refused = error(now(&mut team.app, None, Command::FlowRun(run)));
    assert!(refused.contains("was deleted") && refused.contains("Settings › Flows"), "{refused}");
    let export = wire::FlowExport { name: "Arena".into(), ..wire::FlowExport::default() };
    let refused = error(now(&mut team.app, None, Command::FlowExport(export)));
    assert!(refused.contains("was deleted") && refused.contains("Settings › Flows"), "{refused}");
}

#[test]
fn a_projects_flow_named_like_a_deleted_built_in_does_not_run_and_is_listed_with_a_problem() {
    let mut team = arena_deleted();
    let mut project = pair();
    project.name = "Arena".into();
    store::save(&store::project_dir(team.repo.path()), &project, None).expect("save the flow");

    let listed: wire::Flows =
        serde_json::from_value(value(&mut team.app, None, Command::FlowList(wire::FlowList::default())))
            .expect("the flows");
    let arena = listed.flows.iter().find(|f| f.name == "Arena").expect("the project's flow");
    assert!(
        arena.problems.iter().any(|p| p.contains("deleted built-in `Arena` keeps its name")),
        "{:?}",
        arena.problems
    );

    let run = wire::FlowRun { name: "Arena".into(), prompt: "x".into(), ..wire::FlowRun::default() };
    assert!(error(now(&mut team.app, None, Command::FlowRun(run))).contains("was deleted"));
}

const ISSUE: &str = r#"{"number":7,"title":"Fix the login","state":"OPEN","labels":[],"author":{"login":"ana"},"updatedAt":"","url":"https://github.com/acme/shop/issues/7"}"#;

#[test]
fn a_flow_runs_on_an_issue_with_the_branch_its_start_would_use() {
    let gh_dir = TempDir::new();
    let gh = crate::test_util::fake_gh(gh_dir.path(), &format!("cat <<'EOF'\n{ISSUE}\nEOF"));
    let mut team = Team::configured(FAKE, |config, _| config.gh = gh.display().to_string());
    team.app.issues.env_tokens.clear();
    let run = wire::FlowRun { name: "Pair".into(), issue: Some("#7".into()), ..wire::FlowRun::default() };
    ask(&mut team.app, None, Command::FlowRun(run));

    let Response::Ok(value) = answered(&mut team.app, &team.rx, "the flow's tabs are open") else {
        panic!("the flow did not start")
    };
    let started: RunInfo = serde_json::from_value(value).expect("the run");

    assert_eq!(started.branch, "issue-7-fix-the-login");
}

#[test]
fn a_flow_on_an_issue_of_a_remote_tracker_asks_which_project() {
    let mut team = Team::new();
    team.app.issues.env_tokens.clear();
    let other = git_repo(&[("README", "hi")]);
    team.app.open_project(other.path().to_path_buf(), AREA).expect("a second repository");
    crate::issues::secrets::write(&paths::secrets(&team.config.path().join("config.json")), "shortcut_token", "t0k")
        .expect("save token");
    let story = r#"{"id":482,"name":"Returns page crashes","app_url":"https://app.shortcut.com/acme/story/482"}"#;
    let _server = fake_shortcut(&mut team.app, vec![("GET /api/v3/stories/482", 200, story)]);

    let run = wire::FlowRun { name: "Pair".into(), issue: Some("sc-482".into()), ..wire::FlowRun::default() };
    ask(&mut team.app, None, Command::FlowRun(run));
    let refused = error(answered(&mut team.app, &team.rx, "the flow is refused"));

    assert!(refused.contains("say which project the team works in with --project ID"), "{refused}");
    assert!(team.app.runs.list.is_empty());
}
