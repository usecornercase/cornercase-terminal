use super::*;
use crate::agents::Known;
use crate::app::flow_runs::Starting;
use crate::control::RunInfo;
use crate::flows::builtin;

impl Apart {
    fn runs(&mut self, view: Option<u64>) -> RunInfo {
        let flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
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
