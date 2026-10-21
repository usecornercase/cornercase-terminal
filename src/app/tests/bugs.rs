use super::*;

#[test]
fn a_bug_says_where_to_look_and_stays_a_while() {
    let (mut app, _rx) = empty_app();
    app.report_bug();
    let shown = app.messages.toast.as_ref().map(|t| (t.message.as_str(), t.icon, t.lasts()));
    assert_eq!(shown, Some(("cornercase hit a bug, see server.log", ui::ToastIcon::Bug, BUG_FOR)));
}

#[test]
fn resetting_the_interaction_closes_dialogs_and_ends_drags() {
    let (mut app, _rx) = app();
    let row = sidebar_pos(&app, SidebarRow::Project(0));
    press(&mut app, row);
    app.overlay = Some(Overlay::Usage);
    app.screen.nav = Some(ui::Nav::Projects);
    assert!(matches!(app.screen.drag, Some(Drag::Row(_))) && app.screen.hover.is_some());

    app.reset_interaction();

    assert!(app.overlay.is_none());
    assert_eq!((app.screen.nav, app.screen.hover, app.screen.drag), (None, None, None));
}

#[test]
fn resetting_the_interaction_drops_the_todo_field_and_gives_the_filter_keys_back() {
    let (mut app, _rx) = app();
    app.panels.changes.focus_filter();
    app.panels.changes.push_filter('a');
    app.panels.todo.start_adding();

    app.reset_interaction();

    let filter = app.panels.changes.filter().map(|f| (f.query(), f.focused));
    assert_eq!((app.panels.todo.field().is_none(), filter), (true, Some(("a", false))));
}
