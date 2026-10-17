use super::*;
use crate::config::ProjectSettings;

struct Team {
    app: App,
    rx: Receiver<AppEvent>,
    repo: TempDir,
    _dirs: (TempDir, TempDir),
    marker: PathBuf,
}

fn team(setup: &str, teardown: &str) -> Team {
    let repo = git_repo(&[("README", "hi"), (".worktreeinclude", ".env\n"), (".gitignore", ".env\n")]);
    std::fs::write(repo.path().join(".env"), "SECRET=1").expect("write the env");
    let (worktrees, config) = (TempDir::new(), TempDir::new());
    let fake = config.path().join("claude");
    let marker = config.path().join("marker");
    write_executable(
        &fake,
        &format!("#!/bin/sh\ncat '{}' > '{}.agent'\nprintf 'ready> '\nread line\n", marker.display(), marker.display()),
    );
    let settings = Config {
        worktrees_dir: worktrees.path().display().to_string(),
        agent_commands: [("claude".to_string(), fake.display().to_string())].into(),
        projects: [(
            repo.path().display().to_string(),
            ProjectSettings {
                setup: setup.replace("MARKER", &marker.display().to_string()),
                teardown: teardown.replace("MARKER", &marker.display().to_string()),
            },
        )]
        .into(),
        ..Config::default()
    };
    let (app, rx) = app_in(repo.path(), write_config(config.path(), &settings));
    Team { app, rx, repo, _dirs: (worktrees, config), marker }
}

fn start_in_worktree(app: &mut App) {
    let start = wire::Start {
        agent: Some("claude".into()),
        worktree: Some("feat/x".into()),
        prompt: Some("go".into()),
        ..wire::Start::default()
    };
    ask(app, None, Command::Start(start));
}

#[test]
fn the_setup_runs_in_the_new_worktree_before_its_agent() {
    let mut team = team("echo \"$CORNERCASE_BRANCH\" > MARKER\ntest -f .env", "");
    start_in_worktree(&mut team.app);

    let made = done(answered(&mut team.app, &team.rx, "the agent starts after the setup"));

    let agent = PathBuf::from(format!("{}.agent", team.marker.display()));
    assert!(agent.exists(), "the agent read the marker the setup wrote");
    assert_eq!(std::fs::read_to_string(&agent).expect("agent"), "feat/x\n");
    assert!(made.ids.pane.is_some() && team.repo.path().exists());
}

#[test]
fn a_failed_setup_does_not_start_the_agent_and_says_why() {
    let mut team = team("exit 3", "");
    start_in_worktree(&mut team.app);

    let message = error(answered(&mut team.app, &team.rx, "the request fails"));

    assert!(message.contains("exit status 3") && message.contains("did not start"), "{message}");
    assert!(!PathBuf::from(format!("{}.agent", team.marker.display())).exists());
    let workspace = team.app.projects[0].workspaces.iter().find(|w| w.worktree).expect("worktree");
    assert!(!workspace.setting_up());
    assert_eq!(workspace.tabs[0].status(), Some(activity::Status::Failed));
}

#[test]
fn a_workspace_without_setup_starts_its_agent_at_once() {
    let mut team = team("", "");
    start_in_worktree(&mut team.app);

    done(answered(&mut team.app, &team.rx, "the agent starts"));

    assert!(team.app.projects[0].workspaces.iter().all(|w| !w.setting_up()));
}

#[test]
fn the_teardown_runs_before_the_folder_goes_and_a_failure_still_removes_it() {
    let mut team = team("", "pwd > MARKER\nexit 2");
    ask(
        &mut team.app,
        None,
        Command::NewWorkspace(wire::NewWorkspace {
            name: "feat/y".into(),
            worktree: true,
            ..wire::NewWorkspace::default()
        }),
    );
    let id = done(answered(&mut team.app, &team.rx, "git makes the worktree")).ids.workspace.expect("workspace");
    let (p, w) = team.app.projects.workspace_index(id).expect("it");
    let path = team.app.projects[p].workspaces[w].path.clone();

    ask(
        &mut team.app,
        None,
        Command::Close(wire::Close { item: Item::Workspace(id), remove_worktree: true, force: false, unlock: false }),
    );
    let removed = done(answered(&mut team.app, &team.rx, "git removes the worktree"));

    assert!(removed.warning.is_some_and(|w| w.contains("exit status 2")));
    assert_eq!(std::fs::read_to_string(&team.marker).expect("marker").trim(), path.display().to_string());
    assert!(!path.exists());
}

fn new_worktree(app: &mut App, name: &str) {
    let new = wire::NewWorkspace { name: name.into(), worktree: true, ..wire::NewWorkspace::default() };
    ask(app, None, Command::NewWorkspace(new));
}

#[test]
fn new_workspace_answers_once_the_setup_is_done_and_status_shows_it_meanwhile() {
    let mut team = team("sleep 1\necho done > MARKER", "");
    new_worktree(&mut team.app, "feat/slow");
    pump_until(&mut team.app, &team.rx, "the setup starts", |a| {
        a.projects[0].workspaces.iter().any(Workspace::setting_up)
    });
    let report = value(&mut team.app, None, Command::Status(wire::Status {}));
    let setup = report.pointer("/projects/0/workspaces").and_then(|w| w.as_array()).expect("workspaces");
    assert!(setup.iter().any(|w| w["setup"] == "running"), "{report}");
    assert_eq!(answers(&mut team.app).len(), 0);

    done(answered(&mut team.app, &team.rx, "the answer comes after the setup"));

    assert!(team.marker.exists());
}

#[test]
fn a_failed_setup_is_forgotten_once_it_is_run_again_successfully() {
    let mut team = team("test -f \"$CORNERCASE_ROOT/ok\"", "");
    new_worktree(&mut team.app, "feat/retry");
    let message = error(answered(&mut team.app, &team.rx, "the request fails"));
    assert!(message.contains("exit status 1"), "{message}");
    let workspace = team.app.projects[0].workspaces.iter().find(|w| w.worktree).expect("worktree").id;
    std::fs::write(team.repo.path().join("ok"), "").expect("write");

    team.app.run_setup(workspace, AREA);
    refreshing(&mut team.app, &team.rx, "the setup passes", |a| {
        a.projects[0].workspaces.iter().all(|w| !w.setting_up()) && a.setup_failure(1).is_none()
    });

    let target = team.app.projects[0].workspaces.iter().find(|w| w.id == workspace).expect("workspace");
    assert!(
        target
            .tabs
            .iter()
            .flat_map(|t| &t.panes)
            .all(|t| !matches!(t.setup, Some(git::worktree::setup::Pane::Failed { .. })))
    );
}

#[test]
fn closing_the_setup_tab_drops_the_agent_and_its_status_file() {
    let mut team = team("sleep 30", "");
    start_in_worktree(&mut team.app);
    pump_until(&mut team.app, &team.rx, "the setup starts", |a| {
        a.projects[0].workspaces.iter().any(Workspace::setting_up)
    });
    refreshing(&mut team.app, &team.rx, "it is watched", |a| !a.setups.is_empty());
    let (pane, (_, status)) = team.app.setups.iter().next().map(|(p, v)| (*p, v.clone())).expect("watched");
    std::fs::write(&status, "").expect("status");
    let tab = team.app.projects[0]
        .workspaces
        .iter()
        .flat_map(|w| &w.tabs)
        .find(|t| t.index_of(pane).is_some())
        .expect("tab")
        .id;
    let (project, workspace) =
        (team.app.projects[0].id, team.app.projects[0].workspaces.iter().find(|w| w.worktree).expect("w").id);
    ask(
        &mut team.app,
        None,
        Command::Close(wire::Close { item: Item::Tab(tab), remove_worktree: false, force: false, unlock: false }),
    );
    let _ = (project, workspace);

    let mut got = Vec::new();
    wait_until("the tab closes and the start fails", || {
        drain(&mut team.app, &team.rx);
        team.app.refresh(Instant::now());
        got.extend(answers(&mut team.app));
        got.len() >= 2
    });
    let message =
        got.into_iter().find_map(|r| if let Response::Error(m) = r { Some(m) } else { None }).expect("the start fails");

    assert!(message.contains("closed before it was ready") || message.contains("did not"), "{message}");
    let toast = team.app.toast.as_ref().map(|t| t.message.clone()).unwrap_or_default();
    assert!(toast.contains("setup was closed in") && toast.contains("its agent did not start"), "{toast}");
    assert!(!status.exists());
}

#[test]
fn a_setup_closed_before_the_first_watch_still_drops_the_agent() {
    let mut team = team("sleep 30", "");
    start_in_worktree(&mut team.app);
    pump_until(&mut team.app, &team.rx, "the setup starts", |a| {
        a.projects[0].workspaces.iter().any(Workspace::setting_up)
    });
    assert_eq!(team.app.setups.len(), 1);
    let pane = *team.app.setups.keys().next().expect("a setup");
    let tab = team.app.projects[0]
        .workspaces
        .iter()
        .flat_map(|w| &w.tabs)
        .find(|t| t.index_of(pane).is_some())
        .expect("tab")
        .id;
    ask(
        &mut team.app,
        None,
        Command::Close(wire::Close { item: Item::Tab(tab), remove_worktree: false, force: false, unlock: false }),
    );

    let mut got = Vec::new();
    wait_until("the start fails", || {
        drain(&mut team.app, &team.rx);
        team.app.refresh(Instant::now());
        got.extend(answers(&mut team.app));
        got.len() >= 2
    });

    assert!(got.iter().any(|r| matches!(r, Response::Error(_))));
}

#[test]
fn a_later_unrelated_failure_is_not_blamed_on_a_failed_setup() {
    let mut team = team("exit 3", "");
    new_worktree(&mut team.app, "feat/blame");
    error(answered(&mut team.app, &team.rx, "the request fails"));
    let workspace = team.app.projects[0].workspaces.iter().find(|w| w.worktree).expect("worktree").id;
    let before =
        team.app.projects[0].workspaces.iter().flat_map(Workspace::terms).find(|t| t.setup.is_some()).expect("pane").id;
    assert!(team.app.setup_failure(before).is_some());

    let new = wire::NewTab { workspace: Some(workspace), ..wire::NewTab::default() };
    let pane = done(now_done(&mut team.app, Command::NewTab(new))).ids.pane.expect("pane");

    assert!(team.app.setup_failure(pane).is_none() && team.app.setup_failure(before).is_none());
    let target = team.app.projects[0].workspaces.iter().find(|w| w.id == workspace).expect("workspace");
    assert!(target.setup().is_some_and(|(_, s)| !s.running()));
}

fn now_done(app: &mut App, command: Command) -> Response {
    now(app, None, command)
}
