use super::*;
use crate::app::flow_runs::Starting;
use crate::control::RunInfo;

const SHELL: &str = if cfg!(windows) { "cmd.exe" } else { "/bin/sh" };
const VIEWER: u64 = 31;

struct Apart {
    app: App,
    _rx: Receiver<AppEvent>,
    _folders: [TempDir; 3],
}

fn apart() -> Apart {
    let folders = [TempDir::new(), TempDir::new(), TempDir::new()];
    let [config, viewed, windowed] = &folders;
    let (mut app, rx) = new_app_with_shell(SHELL, config.path().join("config.json"));
    app.open_project(viewed.path().to_path_buf(), AREA).expect("the project a view shows");
    app.open_project(windowed.path().to_path_buf(), AREA).expect("the project the window shows");
    Apart { app, _rx: rx, _folders: folders }
}

impl Apart {
    fn views_the_first_project(&mut self) -> u64 {
        let project = self.app.projects[0].id;
        open_view(&mut self.app, VIEWER, &json!({"cols": 100, "rows": 30, "project": project}))
    }

    fn runs(&mut self, view: Option<u64>) -> RunInfo {
        let flow =
            crate::flows::builtin::flows(&crate::agents::Known::ALL, Some(crate::agents::Known::Claude)).remove(0);
        let starting = Starting {
            view,
            ..Starting::new(flow, "Fix the login form".into(), "main".into(), self.app.active_accounts())
        };
        let place = self.app.projects.workspace_at(0, 0).expect("the workspace");
        self.app.open_run(place, starting, AREA, false).expect("the run")
    }
}

#[test]
fn a_focused_run_in_a_view_shows_its_orchestrator_there_and_leaves_the_window() {
    let mut run = apart();
    let view = run.views_the_first_project();
    let before = (run.app.focus(), run.app.active);

    let info = run.runs(Some(view));

    assert_eq!(info.projection, Some(2));
    assert_eq!(run.app.view_ids(view).tab, Some(info.members[0].tab));
    assert_eq!((run.app.focus(), run.app.active), before);
    assert_eq!(run.app.screen.side, None);
}

#[test]
fn a_focused_run_without_a_view_shows_its_orchestrator_and_its_panel_in_the_window() {
    let mut run = apart();

    let info = run.runs(None);

    assert_eq!(info.projection, None);
    assert_eq!(run.app.shown_ids(run.app.active).tab, Some(info.members[0].tab));
    assert_eq!(run.app.screen.side, Some(Side::Flow));
}

#[test]
fn a_focused_run_whose_view_closed_opens_and_moves_nothing() {
    let mut run = apart();
    let view = run.views_the_first_project();
    run.app.forget(VIEWER);
    let before = (run.app.focus(), run.app.active);

    let info = run.runs(Some(view));

    assert_eq!(info.projection, None);
    assert_eq!(run.app.runs.all().len(), 1);
    assert_eq!((run.app.focus(), run.app.active), before);
    assert_eq!(run.app.screen.side, None);
}
