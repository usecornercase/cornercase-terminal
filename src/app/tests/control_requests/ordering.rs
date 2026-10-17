use rstest::rstest;

use super::*;
use crate::project::Phase;

fn new_group(app: &mut App, name: &str) -> u64 {
    done(Response::Ok(ok(app, "group-new", &json!({"name": name})))).group.expect("the group's id")
}

fn projects(app: &App) -> Vec<u64> {
    app.projects.iter().map(|p| p.id).collect()
}

fn groups(app: &App) -> Vec<u64> {
    app.groups.iter().map(|g| g.id).collect()
}

fn group_of(app: &App, project: u64) -> Option<u64> {
    app.projects.iter().find(|p| p.id == project).and_then(|p| p.group)
}

fn workspaces(app: &App) -> Vec<u64> {
    app.projects[0].workspaces.iter().map(|w| w.id).collect()
}

fn tabs(app: &App) -> Vec<u64> {
    app.projects[0].workspaces[0].tabs.iter().map(|t| t.id).collect()
}

fn workspace(app: &mut App, name: &str) -> u64 {
    done(Response::Ok(ok(app, "new-workspace", &json!({"name": name})))).ids.workspace.expect("the workspace")
}

fn tab(app: &mut App) -> u64 {
    done(Response::Ok(ok(app, "new-tab", &json!({})))).ids.tab.expect("the tab")
}

#[test]
fn a_group_moves_before_another_and_to_the_end() {
    let (mut app, _rx) = app();
    let ids: Vec<u64> = ["a", "b", "c"].iter().map(|name| new_group(&mut app, name)).collect();

    ok(&mut app, "move", &json!({"item": {"group": ids[2]}, "before": ids[0]}));
    assert_eq!(groups(&app), [ids[2], ids[0], ids[1]]);
    ok(&mut app, "move", &json!({"item": {"group": ids[2]}}));
    assert_eq!(groups(&app), [ids[0], ids[1], ids[2]]);
}

#[test]
fn a_project_moved_before_one_in_a_group_joins_it_and_the_window_keeps_its_project() {
    let (mut app, _rx, _dirs) = app_with(3);
    let group = new_group(&mut app, "work");
    let ids = projects(&app);
    ok(&mut app, "move", &json!({"item": {"project": ids[0]}, "into": group}));
    let shown = app.projects[app.active].id;

    ok(&mut app, "move", &json!({"item": {"project": ids[2]}, "before": ids[0]}));

    assert_eq!(group_of(&app, ids[2]), Some(group));
    assert!(projects(&app).iter().position(|&id| id == ids[2]) < projects(&app).iter().position(|&id| id == ids[0]));
    assert_eq!(app.projects[app.active].id, shown);
}

#[test]
fn into_puts_a_project_last_in_the_group_and_ungroup_last_among_the_loose_ones() {
    let (mut app, _rx, _dirs) = app_with(4);
    let group = new_group(&mut app, "work");
    let ids = projects(&app);
    ok(&mut app, "move", &json!({"item": {"project": ids[0]}, "into": group}));
    ok(&mut app, "move", &json!({"item": {"project": ids[1]}, "into": group}));
    let in_group: Vec<u64> = app.projects.iter().filter(|p| p.group == Some(group)).map(|p| p.id).collect();
    assert_eq!(in_group, [ids[0], ids[1]]);

    ok(&mut app, "move", &json!({"item": {"project": ids[0]}, "into": group}));
    let in_group: Vec<u64> = app.projects.iter().filter(|p| p.group == Some(group)).map(|p| p.id).collect();
    assert_eq!(in_group, [ids[1], ids[0]]);

    ok(&mut app, "move", &json!({"item": {"project": ids[1]}, "ungroup": true}));
    let loose: Vec<u64> = app.projects.iter().filter(|p| p.group.is_none()).map(|p| p.id).collect();
    assert_eq!(loose, [ids[2], ids[3], ids[1]]);
}

#[test]
fn a_project_moved_without_a_place_goes_to_the_end_of_its_own_list() {
    let (mut app, _rx, _dirs) = app_with(3);
    let ids = projects(&app);

    ok(&mut app, "move", &json!({"item": {"project": ids[0]}}));

    assert_eq!(projects(&app), [ids[1], ids[2], ids[0]]);
}

#[test]
fn moving_a_project_into_a_group_pending_deletion_cancels_it() {
    let (mut app, _rx, _dirs) = app_with(2);
    let group = new_group(&mut app, "work");
    let ids = projects(&app);
    app.deleting_groups.push(group);

    ok(&mut app, "move", &json!({"item": {"project": ids[0]}, "into": group}));

    assert_eq!(app.deleting_groups.len(), 0);
}

#[test]
fn workspaces_and_tabs_move_among_their_siblings_and_the_active_one_stays_active() {
    let (mut app, _rx, _dirs) = app_with(1);
    let first = app.projects[0].workspaces[0].id;
    let (b, c) = (workspace(&mut app, "b"), workspace(&mut app, "c"));
    let active = app.projects[0].workspaces[app.projects[0].active].id;

    ok(&mut app, "move", &json!({"item": {"workspace": c}, "before": first}));
    assert_eq!(workspaces(&app), [c, first, b]);
    ok(&mut app, "move", &json!({"item": {"workspace": c}}));
    assert_eq!(workspaces(&app), [first, b, c]);
    assert_eq!(app.projects[0].workspaces[app.projects[0].active].id, active);

    let t0 = app.projects[0].workspaces[0].tabs[0].id;
    let (t1, t2) = (tab(&mut app), tab(&mut app));
    let current = app.projects[0].workspaces[0].tabs[app.projects[0].workspaces[0].active].id;
    ok(&mut app, "move", &json!({"item": {"tab": t2}, "before": t0}));
    assert_eq!(tabs(&app), [t2, t0, t1]);
    ok(&mut app, "move", &json!({"item": {"tab": t2}}));
    assert_eq!(tabs(&app), [t0, t1, t2]);
    assert_eq!(app.projects[0].workspaces[0].tabs[app.projects[0].workspaces[0].active].id, current);
}

#[test]
fn a_row_never_moves_before_one_of_another_parent() {
    let (mut app, _rx, _dirs) = app_with(2);
    let (other_workspace, other_tab) = (app.projects[1].workspaces[0].id, app.projects[1].workspaces[0].tabs[0].id);
    let (workspace, tab) = (app.projects[0].workspaces[0].id, app.projects[0].workspaces[0].tabs[0].id);

    let refused_workspace =
        refused(&mut app, "move", &json!({"item": {"workspace": workspace}, "before": other_workspace}));
    let refused_tab = refused(&mut app, "move", &json!({"item": {"tab": tab}, "before": other_tab}));

    assert_eq!((refused_workspace.code.as_str(), refused_tab.code.as_str()), ("conflict", "conflict"));
}

type Build = fn(u64, u64, u64, u64) -> Value;

#[rstest]
#[case::a_pane(|_, _, _, _| json!({"item": {"pane": 1}}), "malformed")]
#[case::before_itself(|group, _, _, _| json!({"item": {"group": group}, "before": group}), "malformed")]
#[case::a_tab_into_a_group(|group, _, _, tab| json!({"item": {"tab": tab}, "into": group}), "malformed")]
#[case::two_places(|_, p0, p1, _| json!({"item": {"project": p0}, "before": p1, "ungroup": true}), "malformed")]
#[case::before_and_into(|group, p0, p1, _| json!({"item": {"project": p0}, "before": p1, "into": group}), "malformed")]
#[case::an_unknown_group(|_, _, _, _| json!({"item": {"group": 999}}), "not-found")]
#[case::an_unknown_project_before(|_, p0, _, _| json!({"item": {"project": p0}, "before": 999}), "not-found")]
#[case::an_unknown_group_to_join(|_, p0, _, _| json!({"item": {"project": p0}, "into": 999}), "not-found")]
#[case::an_unknown_tab(|_, _, _, _| json!({"item": {"tab": 999}}), "not-found")]
#[case::an_unknown_workspace(|_, _, _, _| json!({"item": {"workspace": 999}}), "not-found")]
fn a_move_that_makes_no_sense_is_refused_and_changes_nothing(#[case] build: Build, #[case] code: &str) {
    let (mut app, _rx, _dirs) = app_with(2);
    let group = new_group(&mut app, "work");
    let (p0, p1, t0) = (app.projects[0].id, app.projects[1].id, app.projects[0].workspaces[0].tabs[0].id);
    let before = (groups(&app), projects(&app), tabs(&app));

    let refused = refused(&mut app, "move", &build(group, p0, p1, t0));

    assert_eq!(refused.code, code, "{}", refused.message);
    assert_eq!((groups(&app), projects(&app), tabs(&app)), before);
}

#[test]
fn collapse_and_expand_set_the_fold_whatever_it_was() {
    let (mut app, _rx, _dirs) = app_with(1);
    let group = new_group(&mut app, "work");
    let project = app.projects[0].id;
    let workspace = app.projects[0].workspaces[0].id;
    let folds =
        |app: &App| (app.groups[0].entry.collapsed, app.projects[0].collapsed, app.projects[0].workspaces[0].collapsed);

    for item in [json!({"group": group}), json!({"project": project}), json!({"workspace": workspace})] {
        ok(&mut app, "collapse", &json!({"item": item}));
        ok(&mut app, "collapse", &json!({"item": item}));
    }
    assert_eq!(folds(&app), (true, true, true));
    for item in [json!({"group": group}), json!({"project": project}), json!({"workspace": workspace})] {
        ok(&mut app, "expand", &json!({"item": item}));
        ok(&mut app, "expand", &json!({"item": item}));
    }
    assert_eq!(folds(&app), (false, false, false));
}

#[rstest]
#[case::a_tab(json!({"tab": 1}))]
#[case::a_pane(json!({"pane": 1}))]
fn a_tab_or_a_pane_has_no_fold(#[case] item: Value) {
    let (mut app, _rx) = app();

    let refused = refused(&mut app, "collapse", &json!({"item": item}));

    assert_eq!(refused.code, "malformed");
}

#[test]
fn an_unknown_row_has_no_fold() {
    let (mut app, _rx) = app();

    let refused = refused(&mut app, "expand", &json!({"item": {"project": 999}}));

    assert_eq!(refused.code, "not-found");
}

#[test]
fn a_workspace_being_removed_is_neither_moved_nor_folded() {
    let (mut app, _rx, _dirs) = app_with(1);
    let first = app.projects[0].workspaces[0].id;
    let second = workspace(&mut app, "b");
    app.projects[0].workspaces[1].phase = Phase::Removing("b".into());

    let moved = refused(&mut app, "move", &json!({"item": {"workspace": second}, "before": first}));
    let folded = refused(&mut app, "collapse", &json!({"item": {"workspace": second}}));

    assert!(moved.message.contains("is being removed") && folded.message.contains("is being removed"));
    assert!(!app.projects[0].workspaces[1].collapsed);
}
