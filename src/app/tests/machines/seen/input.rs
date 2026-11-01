use super::*;
use crate::app::machines::RemoteItem;

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
