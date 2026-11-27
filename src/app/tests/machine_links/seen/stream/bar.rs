use super::keys::split_listing;
use super::*;
use crate::control::PaneInfo;

#[test]
fn the_details_follow_the_pane_focused_in_the_view() {
    let mut streaming = split_listing(&[1, 2]);
    let mut report = worked(report(&[("web-shop", Some(SHOP)), ("blog", None)]));
    let pane =
        |id, model: Option<&str>| PaneInfo { id, model: model.map(Into::into), active: id == 2, ..PaneInfo::default() };
    report.projects[0].workspaces[0].tabs[1].panes = vec![pane(1, Some("Opus 5.5")), pane(2, None)];
    serve(&mut streaming.seeing.theirs, 3, report);
    streaming.seeing.rig.until("the models are listed", |app| {
        let seen = app.machines.held()[0].seen.as_ref();
        seen.is_some_and(|seen| seen.projects[0].workspaces[0].tabs[1].panes[0].model.is_some())
    });
    let app = &mut streaming.seeing.rig.app;
    app.config.model = true;

    assert_eq!(app.tab_bar_view().details.model.as_deref(), Some("Opus 5.5"));
}
