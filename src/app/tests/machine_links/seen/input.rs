use super::*;
use crate::app::machine_links::RemoteItem;
use crate::ui::files::{self as panel, Screen};

fn in_the_pane(app: &App) -> Position {
    let pane = app.layout(AREA).shown(app.screen.nav).pane;
    Position::new(pane.x + 2, pane.y + 2)
}

#[test]
fn keys_a_paste_and_clicks_never_reach_the_local_pane_behind_a_machine_s_workspace() {
    let Seeing { mut rig, .. } = seeing_two();
    rig.app.term_mut().expect("a pane").feed(b"\x1b[?1000h");
    assert!(rig.app.select_remote("vps", RemoteItem::Workspace(10)));

    let at = in_the_pane(&rig.app);

    type_text(&mut rig.app, "zz");
    rig.app.handle_paste("pasted-text", AREA);
    click(&mut rig.app, at);
    right_click(&mut rig.app, at);

    assert!(rig.app.overlay.is_none());
    rig.app.leave_remote();
    type_line(&mut rig.app, "\recho mark\"\"er");
    shows(&mut rig.app, &rig.rx, "marker");
    let shown = screen(&mut rig.app);
    assert!(!shown.contains("zz") && !shown.contains("pasted-text") && !shown.contains("^["), "{shown}");
}

fn listed(rig: &mut Rig) {
    let (app, rx) = (&mut rig.app, &rig.rx);
    wait_until("the local tree is listed", || {
        app.refresh(Instant::now());
        drain(app, rx);
        app.files_view().is_some_and(|view| matches!(view.screen, Screen::Tree(tree) if !tree.rows.is_empty()))
    });
}

#[test]
fn the_local_panels_give_no_target_behind_a_machine_s_workspace() {
    let Seeing { mut rig, _dir, .. } = seeing_two_in(git_repo(&[("README.md", "# shop\n")]));
    assert!(rig.app.changes_target().is_some(), "the paired project is a git checkout");
    let files = rig.app.layout(AREA).files_button.as_position();
    click(&mut rig.app, files);
    listed(&mut rig);
    let body = panel::parts(panel_area(&rig.app)).body;
    let row = Position::new(body.x + 4, body.y);
    right_click(&mut rig.app, row);
    assert!(matches!(rig.app.overlay, Some(Overlay::Menu { .. })), "the local row has its menu");
    rig.app.overlay = None;

    assert!(rig.app.select_remote("vps", RemoteItem::Workspace(10)));

    assert!(rig.app.changes_target().is_none());
    let view = rig.app.files_view().expect("the machine's tree");
    assert!(view.remote.is_some() && matches!(view.screen, Screen::Tree(tree) if tree.rows.is_empty()));
    right_click(&mut rig.app, row);
    assert!(rig.app.overlay.is_none());
}
