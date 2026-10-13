use super::*;

fn external_checkout() -> (TempDir, App, Receiver<AppEvent>, TempDir, PathBuf) {
    let repo = git_repo(&[]);
    let (mut app, rx) = app_in(repo.path(), no_config());
    let tmp = TempDir::new();
    let path = tmp.path().join("hotfix");
    crate::test_util::git(repo.path(), &["worktree", "add", "--quiet", "-b", "hotfix", &path.display().to_string()]);
    app.sync_worktrees();
    (repo, app, rx, tmp, path)
}

fn open_form(app: &mut App) {
    let pos = new_workspace_pos(app);
    click(app, pos);
}

fn worktree_option(app: &App) -> Option<bool> {
    let Some(Overlay::NewWorkspace { worktree, .. }) = &app.overlay else { panic!("the form is not open") };
    *worktree
}

#[test]
fn outside_git_the_form_has_no_worktree_option() {
    let (mut app, _rx, _dirs) = app_with(1);
    open_form(&mut app);
    assert_eq!(worktree_option(&app), None);
}

#[test]
fn in_a_repo_the_form_offers_a_worktree() {
    let repo = git_repo(&[]);
    let (mut app, _rx) = app_in(repo.path(), no_config());
    open_form(&mut app);
    assert_eq!(worktree_option(&app), Some(true));
}

#[test]
fn the_toggle_switches_the_worktree_option() {
    let repo = git_repo(&[]);
    let (mut app, _rx) = app_in(repo.path(), no_config());
    open_form(&mut app);

    click(&mut app, ui::form_toggle(ui::form_area(AREA)).as_position());

    assert_eq!(worktree_option(&app), Some(false));
}

#[test]
fn an_empty_name_is_refused() {
    let (mut app, _rx, _dirs) = app_with(1);
    open_form(&mut app);

    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(form_error(&app), Some("the name is required"));
}

#[test]
fn a_plain_one_shares_the_project_folder() {
    let (mut app, _rx, dirs) = app_with(1);
    open_form(&mut app);
    type_text(&mut app, "bug-123");

    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    let project = &app.projects[0];
    assert_eq!((app.overlay.is_none(), project.active), (true, 1));
    assert_eq!(
        (project.workspaces[1].path.clone(), workspace_labels(&app)[1].as_str()),
        (canonical(&dirs[0]), "bug-123")
    );
    wait_until("its tab starts in the project", || tab_term(&app, 1, 0).cwd() == Some(canonical(&dirs[0])));
}

#[test]
fn with_a_worktree_it_opens_in_a_new_checkout() {
    let repo = git_repo(&[("README", "hi")]);
    let (worktrees, _config, config_path) = with_worktrees_config();
    let (mut app, rx) = app_in(repo.path(), config_path);
    open_form(&mut app);
    type_text(&mut app, "feat/login");

    click(&mut app, form_button(CREATE_SUBMIT, 0));

    pump_until(&mut app, &rx, "the workspace opens", |a| a.projects[0].workspaces.len() == 2);
    let repo_name = repo.path().file_name().expect("repo name");
    let expected = worktrees.path().join(repo_name).join("feat-login").canonicalize().expect("checkout");
    let workspace = &app.projects[0].workspaces[1];
    assert_eq!(
        (workspace.path.clone(), workspace.worktree, workspace.label()),
        (expected.clone(), true, "feat/login".into())
    );
    assert_eq!((app.overlay.is_none(), app.projects[0].active, workspace.tabs.len()), (true, 1, 1));
    wait_until("its tab starts in the checkout", || tab_term(&app, 1, 0).cwd() == Some(expected.clone()));
}

#[test]
fn git_errors_stay_in_the_form() {
    let repo = git_repo(&[("README", "hi")]);
    let (_worktrees, _config, config_path) = with_worktrees_config();
    let (mut app, rx) = app_in(repo.path(), config_path);
    open_form(&mut app);
    type_text(&mut app, "not valid");

    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    pump_until(&mut app, &rx, "git fails", |a| form_error(a).is_some());
    assert_eq!(
        (form_error(&app), app.projects[0].workspaces.len()),
        (Some("'not valid' is not a valid branch name"), 1)
    );
}

#[test]
fn checkouts_made_outside_show_up() {
    let (_repo, app, _rx, _tmp, _path) = external_checkout();

    let workspace = &app.projects[0].workspaces[1];
    assert_eq!((workspace.label(), workspace.worktree, workspace.tabs.len()), ("hotfix".into(), true, 0));
}

#[test]
fn checkouts_removed_outside_go_away() {
    let (_repo, mut app, _rx, _tmp, path) = external_checkout();

    std::fs::remove_dir_all(&path).expect("remove checkout");
    app.sync_worktrees();

    assert_eq!(app.projects[0].workspaces.len(), 1);
}

mod behind {
    use super::*;
    use crate::test_util::git;

    fn behind(app: &App) -> u32 {
        app.projects[0].workspaces[0].behind
    }

    fn commit(dir: &Path) {
        git(dir, &["commit", "--quiet", "--allow-empty", "-m", "more"]);
    }

    #[test]
    fn a_commit_on_the_remote_shows_until_it_is_pulled() {
        let remote = git_repo(&[]);
        let (_tmp, clone) = crate::test_util::clone_of(remote.path());
        let (mut app, rx) = app_in(&clone, no_config());
        commit(remote.path());
        let start = Instant::now();

        app.count_behind(start);
        pump_until(&mut app, &rx, "the commit to pull shows", |a| behind(a) == 1);
        git(&clone, &["pull", "--quiet", "--ff-only"]);
        app.count_behind(start + COUNT_BEHIND_EVERY);

        pump_until(&mut app, &rx, "the pulled commit is gone", |a| behind(a) == 0);
    }

    #[test]
    fn a_repo_without_upstream_shows_nothing() {
        let repo = git_repo(&[]);
        let (mut app, rx) = app_in(repo.path(), no_config());

        app.count_behind(Instant::now());

        pump_until(&mut app, &rx, "the count answers", |a| a.upstream.counting.is_empty());
        assert_eq!(behind(&app), 0);
    }

    #[test]
    fn nothing_is_fetched_when_turned_off() {
        let repo = git_repo(&[]);
        let tmp = TempDir::new();
        let config_path = write_config(tmp.path(), &Config { fetch_minutes: 0, ..Config::default() });
        let (mut app, _rx) = app_in(repo.path(), config_path);
        app.projects[0].workspaces[0].behind = 2;

        app.count_behind(Instant::now());

        assert_eq!((app.upstream.counting.len(), behind(&app)), (0, 0));
    }
}

#[test]
fn clicking_one_selects_it() {
    let (mut app, _rx, _dirs) = app_with(1);
    open_form(&mut app);
    submit_text(&mut app, "other");

    click_row(&mut app, WorkspaceRow::Workspace(0));

    assert_eq!(app.projects[0].active, 0);
}

#[test]
fn closing_a_plain_one_closes_its_tabs() {
    let (mut app, rx, _dirs) = app_with(1);
    open_form(&mut app);
    submit_text(&mut app, "other");

    click_close(&mut app, WorkspaceRow::Workspace(1));

    pump_until(&mut app, &rx, "the workspace closes", |a| a.projects[0].workspaces.len() == 1);
}
