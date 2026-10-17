use serde_json::Value;

use super::*;

fn status(app: &Harness) -> Value {
    serde_json::from_str(&app.session.says(&["status", "--json"])).expect("a status report")
}

#[test]
fn the_sidebar_is_arranged_from_the_command_line() {
    let app = Harness::start();
    let other = temp_dir("arrange-project");
    let project = app.session.says(&["open", &other.display().to_string()]);

    let group = app.session.says(&["group", "new", "work"]);
    app.session.says(&["move", "--project", &project, "--into", &group]);
    app.session.says(&["collapse", "--group", &group]);
    let styled = app.session.says(&["group", "style", &group, "--icon", "★", "--colour", "99", "--json"]);

    let styled: Value = serde_json::from_str(&styled).expect("the style");
    assert_eq!((styled["icon"].as_str(), styled["colour"].as_u64()), (Some("★"), Some(99)));
    assert_eq!(app.session.says(&["group", "style", &group]), "★ 99");
    let report = status(&app);
    let group_id: u64 = group.parse().expect("an id");
    let project_id: u64 = project.parse().expect("an id");
    assert_eq!(report["groups"][0]["id"], group_id);
    assert_eq!(report["groups"][0]["collapsed"], true);
    let moved = report["projects"].as_array().expect("projects").iter().find(|p| p["id"] == project_id);
    assert_eq!(moved.map(|p| p["group"].clone()), Some(group_id.into()));

    app.session.says(&["expand", "--group", &group]);
    app.session.says(&["move", "--project", &project, "--ungroup"]);
    app.session.says(&["group", "delete", &group]);
    let report = status(&app);
    assert_eq!(report["groups"].as_array().map(Vec::len), Some(0));
    let _ = std::fs::remove_dir_all(&other);
}

#[test]
fn a_pane_is_moved_within_its_tab_from_the_command_line() {
    let app = Harness::start();
    let first = app.session.says(&["new-tab"]);
    let second = app.session.says(&["split", "--pane", &first]);

    app.session.says(&["move-pane", "--pane", &second, "--target", &first, "--place", "left"]);

    let out = app.session.says(&["move-pane", "--pane", &first, "--target", &second, "--place", "swap", "--json"]);
    let moved: Value = serde_json::from_str(&out).expect("the layout");
    assert!(moved["tab"].is_u64() && moved["layout"].is_object(), "{moved}");
    let refused = app.session.cli(&["move-pane", "--pane", &first, "--target", &first, "--place", "swap"]);
    assert_eq!(refused.status.code(), Some(1), "{refused:?}");
}
