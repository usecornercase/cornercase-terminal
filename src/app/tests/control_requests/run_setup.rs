use std::time::Duration;

use super::*;
use crate::app::menus::MenuAction;
use crate::config::ProjectSettings;
use crate::control::{OpenView, RunSetup};
use crate::project::{Hierarchy, Workspace};
use crate::test_util::{git_repo, wait_until_within};

const SHELL: &str = if cfg!(windows) { "cmd.exe" } else { "/bin/sh" };
const WITHIN: Duration = Duration::from_secs(60);
const VIEWER: u64 = 31;

struct Retry {
    app: App,
    rx: Receiver<AppEvent>,
    repo: TempDir,
    _dirs: (TempDir, TempDir),
}

fn retry(setup: &str) -> Retry {
    let repo = git_repo(&[("README", "hi")]);
    let (worktrees, config) = (TempDir::new(), TempDir::new());
    let project = ProjectSettings { setup: setup.to_string(), ..ProjectSettings::default() };
    let settings = Config {
        worktrees_dir: worktrees.path().display().to_string(),
        projects: [(repo.path().display().to_string(), project)].into(),
        ..Config::default()
    };
    let (mut app, rx) = new_app_with_shell(SHELL, write_config(config.path(), &settings));
    app.open_project(repo.path().to_path_buf(), AREA).expect("open the project");
    Retry { app, rx, repo, _dirs: (worktrees, config) }
}

fn run(workspace: u64) -> Command {
    Command::RunSetup(RunSetup { workspace: Some(workspace), focus: false })
}

impl Retry {
    fn until(&mut self, what: &str, cond: impl Fn(&App) -> bool) {
        wait_until_within(what, WITHIN, || {
            drain(&mut self.app, &self.rx);
            self.app.refresh(Instant::now());
            cond(&self.app)
        });
    }

    fn answer(&mut self, what: &str) -> Response {
        let mut got = Vec::new();
        wait_until_within(what, WITHIN, || {
            drain(&mut self.app, &self.rx);
            self.app.refresh(Instant::now());
            got = answers(&mut self.app);
            !got.is_empty()
        });
        got.remove(0)
    }

    fn ask_new_worktree(&mut self, branch: &str) {
        let new = wire::NewWorkspace { name: branch.into(), worktree: true, ..wire::NewWorkspace::default() };
        ask(&mut self.app, None, Command::NewWorkspace(new));
    }

    fn new_worktree(&mut self, branch: &str) -> Response {
        self.ask_new_worktree(branch);
        self.answer("the new worktree's setup ends")
    }

    fn worktree(&self) -> &Workspace {
        self.app.projects[0].workspaces.iter().find(|w| w.worktree).expect("a worktree")
    }

    fn setup_state(&mut self, workspace: u64) -> Value {
        let report = value(&mut self.app, None, Command::Status(wire::Status {}));
        let workspaces = report["projects"][0]["workspaces"].as_array().expect("workspaces");
        workspaces.iter().find(|w| w["id"] == workspace).expect("the workspace")["setup"].clone()
    }

    fn shown_tab(&self) -> Option<u64> {
        self.app.tab().map(|tab| tab.id)
    }

    fn open_view_on(&mut self, tab: u64) -> u64 {
        let open = OpenView { cols: Some(100), rows: Some(30), tab: Some(tab), ..OpenView::default() };
        let opening = json!({"command": "view", "args": open}).to_string();
        self.app.request(VIEWER, &opening, Some(AREA), Instant::now());
        let (_, answer) = self.app.take_answers().remove(0);
        let opened: Value = serde_json::from_str(&answer).expect("an answer");
        opened["ok"]["view"]["id"].as_u64().expect("the view's id")
    }

    fn refuses(&mut self, workspace: u64, why: &str) {
        let panes = self.app.projects.terms().count();
        let refusal = refused(&mut self.app, "run-setup", &json!({"workspace": workspace}));
        assert_eq!(refusal.code, "conflict", "{refusal:?}");
        assert!(refusal.message.contains(why), "{refusal:?}");
        assert_eq!(self.app.projects.terms().count(), panes, "a refused setup opened a tab");
        let project = self.app.projects[0].id;
        assert_eq!(self.app.setup_action(project, workspace), None, "the menu offers what the request refuses");
        self.app.run_setup(workspace, AREA);
        assert_eq!(self.app.projects.terms().count(), panes, "the menu's click ran what the request refuses");
    }
}

#[test]
fn a_failed_setup_runs_again_in_a_new_tab_and_answers_once_it_is_done() {
    let mut retry = retry(
        "echo \"$CORNERCASE_BRANCH:$CORNERCASE_WORKTREE\" > \"$CORNERCASE_ROOT/ran\"\ntest -f \"$CORNERCASE_ROOT/ok\"",
    );
    let first = error(retry.new_worktree("feat/retry"));
    assert!(first.contains("exit status 1"), "{first}");
    let workspace = retry.worktree().id;
    let project = retry.app.projects[0].id;
    assert_eq!(retry.setup_state(workspace), "failed");
    assert_eq!(retry.app.setup_action(project, workspace), Some(MenuAction::RunSetup(workspace)));
    std::fs::write(retry.repo.path().join("ok"), "").expect("write the marker");

    ask(&mut retry.app, None, run(workspace));

    assert_eq!(retry.setup_state(workspace), "running");
    assert!(retry.worktree().tabs[0].pane().is_some_and(|term| term.setup.is_none()), "the failed mark stayed");
    assert_eq!(answers(&mut retry.app).len(), 0, "it answered before the setup ended");
    let made = done(retry.answer("the setup passes"));
    assert_eq!(retry.setup_state(workspace), Value::Null, "the mark came back");
    let target = retry.worktree();
    let tab = target.tabs.last().expect("a tab");
    assert_eq!(target.tabs.len(), 2);
    assert_eq!((made.ids.project, made.ids.workspace, made.ids.tab), (Some(project), Some(workspace), Some(tab.id)));
    assert_eq!(made.ids.pane, tab.pane().map(|term| term.id));
    let ran = std::fs::read_to_string(retry.repo.path().join("ran")).expect("what the setup wrote");
    assert_eq!(ran.trim(), format!("feat/retry:{}", target.path.display()));
}

#[test]
fn a_setup_that_fails_again_names_its_exit_status_and_its_pane() {
    let mut retry = retry("exit 3");
    error(retry.new_worktree("feat/three"));
    let workspace = retry.worktree().id;

    ask(&mut retry.app, None, run(workspace));
    let message = error(retry.answer("the setup fails again"));

    let pane = retry.worktree().tabs.last().and_then(|tab| tab.pane()).expect("its pane").id;
    assert!(message.contains("exit status 3") && message.contains(&format!("--pane {pane}")), "{message}");
    assert!(!message.contains("agent"), "no agent was asked for: {message}");
    assert_eq!(retry.setup_state(workspace), "failed");
}

#[test]
fn a_tab_opened_right_after_the_setup_failed_does_not_turn_the_answer_into_a_success() {
    let mut retry = retry("exit 3");
    error(retry.new_worktree("feat/raced"));
    let workspace = retry.worktree().id;
    ask(&mut retry.app, None, run(workspace));
    let (app, rx) = (&mut retry.app, &retry.rx);
    wait_until_within("the setup ends", WITHIN, || {
        drain(app, rx);
        app.watch_setups(Instant::now());
        !app.workspace_setting_up(workspace)
    });
    let new = wire::NewTab { workspace: Some(workspace), ..wire::NewTab::default() };
    done(now(app, None, Command::NewTab(new)));

    app.check_requests(Instant::now());

    let message = error(answers(app).remove(0));
    assert!(message.contains("exit status 3"), "{message}");
}

#[test]
fn the_setup_goes_on_when_the_client_that_asked_leaves() {
    let mut retry = retry("test -f \"$CORNERCASE_ROOT/ok\"\necho done > \"$CORNERCASE_ROOT/ran\"");
    error(retry.new_worktree("feat/left"));
    let workspace = retry.worktree().id;
    std::fs::write(retry.repo.path().join("ok"), "").expect("write the marker");
    ask(&mut retry.app, None, run(workspace));

    retry.app.forget(CLIENT);

    retry.until("the setup ends", |app| !app.workspace_setting_up(workspace));
    assert!(retry.repo.path().join("ran").exists(), "the setup stopped");
    assert_eq!(retry.setup_state(workspace), Value::Null);
}

#[test]
fn only_focus_shows_the_new_tab() {
    let mut retry = retry("true");
    done(retry.new_worktree("feat/shown"));
    let workspace = retry.worktree().id;
    let shown = retry.shown_tab();

    ask(&mut retry.app, None, run(workspace));
    let left = done(retry.answer("the setup ends"));
    assert_ne!(left.ids.tab, shown);
    assert_eq!(retry.shown_tab(), shown, "the window moved");

    ask(&mut retry.app, None, Command::RunSetup(RunSetup { workspace: Some(workspace), focus: true }));
    let moved = done(retry.answer("the setup ends"));
    assert_eq!(retry.shown_tab(), moved.ids.tab, "the window stayed");
}

#[test]
fn a_view_is_the_default_and_focus_moves_it_while_the_window_stays() {
    let mut retry = retry("true");
    done(retry.new_worktree("feat/view"));
    let target = retry.worktree();
    let (workspace, tab) = (target.id, target.tabs[0].id);
    let window = retry.shown_tab();
    let view = retry.open_view_on(tab);
    let command = Command::RunSetup(RunSetup { workspace: None, focus: true });
    let request = wire::Request { caller: None, server: None, contract: None, boot: None, view: Some(view), command };

    let asking = serde_json::to_string(&request).expect("json");
    retry.app.request(CLIENT, &asking, Some(AREA), Instant::now());
    let made = done(retry.answer("the setup ends"));

    assert_eq!(made.ids.workspace, Some(workspace));
    assert!(made.projection.is_some(), "{made:?}");
    assert_eq!(retry.shown_tab(), window, "the window moved");
    assert_eq!(retry.app.clients.views.get(view).expect("the view").shown.tab, made.ids.tab);
}

#[test]
fn from_a_pane_it_runs_in_that_workspace_and_the_pane_is_its_parent() {
    let mut retry = retry("true");
    done(retry.new_worktree("feat/caller"));
    let target = retry.worktree();
    let (workspace, caller) = (target.id, target.tabs[0].pane().expect("a pane").id);

    ask(&mut retry.app, Some(caller), Command::RunSetup(RunSetup::default()));
    let made = done(retry.answer("the setup ends"));

    assert_eq!(made.ids.workspace, Some(workspace));
    let pane = made.ids.pane.and_then(|pane| retry.app.projects.pane_by(pane)).expect("the new pane");
    assert_eq!(pane.parent, Some(caller));
}

mod what_the_menu_does_not_offer {
    use super::*;

    #[test]
    fn a_workspace_that_is_not_a_worktree_is_refused() {
        let mut retry = retry("true");
        let plain = retry.app.projects[0].workspaces[0].id;

        retry.refuses(plain, "is not a worktree");

        let shown = refused(&mut retry.app, "run-setup", &json!({}));
        assert_eq!(shown.code, "conflict", "{shown:?}");
        assert!(shown.message.contains(&format!("workspace {plain} is not a worktree")), "{shown:?}");
    }

    #[test]
    fn a_project_with_no_setup_is_refused() {
        let mut retry = retry("  \n");
        done(retry.new_worktree("feat/none"));
        let workspace = retry.worktree().id;
        let project = retry.app.projects[0].id;

        retry.refuses(workspace, &format!("project {project} has no setup"));
    }

    #[test]
    fn a_setup_already_running_is_refused() {
        let mut retry = retry("true");
        retry.ask_new_worktree("feat/running");
        let (app, rx) = (&mut retry.app, &retry.rx);
        wait_until_within("the setup starts", WITHIN, || {
            drain(app, rx);
            app.projects[0].workspaces.iter().any(Workspace::setting_up)
        });
        let target = retry.worktree();
        let (workspace, pane) = (target.id, target.tabs[0].pane().expect("a pane").id);

        retry.refuses(workspace, &format!("already running its setup in pane {pane}"));
    }

    #[test]
    fn a_workspace_being_removed_is_refused_and_the_menu_says_nothing() {
        let mut retry = retry("true");
        done(retry.new_worktree("feat/going"));
        let workspace = retry.worktree().id;
        let at = retry.app.projects.workspace_index(workspace).expect("the workspace");
        retry.app.projects.workspace_mut(at).start_removing();

        let refusal = refused(&mut retry.app, "run-setup", &json!({"workspace": workspace}));

        assert!(refusal.message.contains("is being removed"), "{refusal:?}");
        assert_eq!(retry.app.setup_action(retry.app.projects[0].id, workspace), None);
    }

    #[test]
    fn a_workspace_that_does_not_exist_is_not_found() {
        let (mut app, _rx) = empty_app();

        let refusal = refused(&mut app, "run-setup", &json!({"workspace": 99}));

        assert_eq!(refusal.code, "not-found", "{refusal:?}");
    }

    #[test]
    fn a_field_it_does_not_know_is_refused_by_its_path() {
        let (mut app, _rx) = empty_app();

        let refusal = refused(&mut app, "run-setup", &json!({"workspace": 99, "focus": true, "name": "x"}));

        assert_eq!(
            (refusal.code.as_str(), refusal.message.contains("args.name")),
            ("unknown-field", true),
            "{refusal:?}"
        );
    }

    #[test]
    fn no_workspace_here_says_how_to_name_one() {
        let (mut app, _rx) = empty_app();

        let refusal = refused(&mut app, "run-setup", &json!({}));

        assert!(refusal.message.contains("pass --workspace or --worktree"), "{refusal:?}");
    }

    #[test]
    fn a_server_with_no_size_says_how_to_start_its_session() {
        let (mut app, _rx) = empty_app();

        request(&mut app, None, Command::RunSetup(RunSetup::default()), None);

        let message = error(answers(&mut app).remove(0));
        assert!(message.contains("cornercase start-server"), "{message}");
    }
}
