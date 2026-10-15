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
        Command::Close(wire::Close { item: Item::Workspace(id), remove_worktree: true, force: false }),
    );
    let removed = done(answered(&mut team.app, &team.rx, "git removes the worktree"));

    assert!(removed.warning.is_some_and(|w| w.contains("exit status 2")));
    assert_eq!(std::fs::read_to_string(&team.marker).expect("marker").trim(), path.display().to_string());
    assert!(!path.exists());
}
