use rstest::rstest;

use super::*;
use crate::app::menus::Target;
use crate::app::overlay::Overlay;
use crate::project::group::{GROUP_COLOURS, GROUP_ICONS, GROUP_STYLES};

fn new_group(app: &mut App, name: &str) -> u64 {
    done(Response::Ok(ok(app, "group-new", &json!({"name": name})))).group.expect("the group's id")
}

fn group_of(app: &App, project: u64) -> Option<u64> {
    app.projects.iter().find(|p| p.id == project).and_then(|p| p.group)
}

#[test]
fn a_new_group_takes_the_next_preset_and_answers_its_id() {
    let (mut app, _rx) = app();

    let first = new_group(&mut app, "work");
    let second = new_group(&mut app, "  side  ");

    let styles: Vec<(char, u8)> = app.groups.iter().map(|g| (g.entry.icon, g.entry.colour)).collect();
    assert_eq!(styles, [GROUP_STYLES[0], GROUP_STYLES[1]]);
    assert_eq!(
        app.groups.iter().map(|g| (g.id, g.entry.name.as_str())).collect::<Vec<_>>(),
        [(first, "work"), (second, "side")]
    );
}

#[test]
fn a_group_needs_a_name() {
    let (mut app, _rx) = app();

    let refused = refused(&mut app, "group-new", &json!({"name": " \t"}));

    assert_eq!((refused.code.as_str(), app.groups.len()), ("failed", 0));
    assert_eq!(refused.message, "a group needs a name");
}

#[test]
fn a_style_comes_from_the_sets_and_the_answer_lists_them() {
    let (mut app, _rx) = app();
    let group = new_group(&mut app, "work");

    let read = ok(&mut app, "group-style", &json!({"group": group}));
    assert_eq!(read["icon"], GROUP_STYLES[0].0.to_string());
    let set = ok(&mut app, "group-style", &json!({"group": group, "icon": "★", "colour": 99}));

    assert_eq!((set["icon"].as_str(), set["colour"].as_u64()), (Some("★"), Some(99)));
    assert_eq!(set["icons"].as_array().map(Vec::len), Some(GROUP_ICONS.len()));
    assert_eq!(set["colours"].as_array().map(Vec::len), Some(GROUP_COLOURS.len()));
    assert_eq!((app.groups[0].entry.icon, app.groups[0].entry.colour), ('★', 99));
    ok(&mut app, "group-style", &json!({"group": group, "colour": 2}));
    assert_eq!((app.groups[0].entry.icon, app.groups[0].entry.colour), ('★', 2));
}

#[rstest]
#[case::an_icon_outside_the_set(json!({"icon": "x"}), "malformed")]
#[case::two_icons(json!({"icon": "★★"}), "malformed")]
#[case::a_colour_outside_the_set(json!({"colour": 7}), "malformed")]
#[case::an_unknown_group(json!({"group": 999}), "not-found")]
#[case::no_group(json!({"__none": true}), "malformed")]
fn a_style_outside_the_sets_is_refused(#[case] mut args: Value, #[case] code: &str) {
    let (mut app, _rx) = app();
    let group = new_group(&mut app, "work");
    let before = (app.groups[0].entry.icon, app.groups[0].entry.colour);
    if args.get("__none").is_some() {
        args = json!({});
    } else if args.get("group").is_none() {
        args["group"] = json!(group);
    }

    let refused = refused(&mut app, "group-style", &args);

    assert_eq!(refused.code, code, "{}", refused.message);
    assert_eq!((app.groups[0].entry.icon, app.groups[0].entry.colour), before);
}

#[test]
fn deleting_a_group_keeps_its_projects_out_of_it() {
    let (mut app, _rx, _dirs) = app_with(2);
    let group = new_group(&mut app, "work");
    let ids: Vec<u64> = app.projects.iter().map(|p| p.id).collect();
    ok(&mut app, "move", &json!({"item": {"project": ids[0]}, "into": group}));

    ok(&mut app, "group-delete", &json!({"group": group}));

    assert!(app.groups.is_empty());
    assert_eq!(app.projects.len(), 2);
    assert!(ids.iter().all(|&id| group_of(&app, id).is_none()));
}

#[test]
fn deleting_a_group_with_its_projects_closes_them_and_the_group_goes_once_they_are_gone() {
    let (mut app, rx, _dirs) = app_with(3);
    let group = new_group(&mut app, "work");
    let ids: Vec<u64> = app.projects.iter().map(|p| p.id).collect();
    ok(&mut app, "move", &json!({"item": {"project": ids[0]}, "into": group}));
    ok(&mut app, "move", &json!({"item": {"project": ids[1]}, "into": group}));

    ok(&mut app, "group-delete", &json!({"group": group, "close_projects": true}));

    pump_until(&mut app, &rx, "the projects close and the group goes", |a| {
        a.groups.is_empty() && a.projects.len() == 1
    });
    assert_eq!(app.projects[0].id, ids[2]);
}

#[test]
fn deleting_a_group_closes_the_windows_dialog_for_it() {
    let (mut app, _rx) = app();
    let group = new_group(&mut app, "work");
    let other = new_group(&mut app, "side");
    app.overlay = Some(Overlay::GroupStyle { group: other });
    ok(&mut app, "group-delete", &json!({"group": group}));
    assert!(app.overlay.is_some(), "another group's dialog stays");

    app.overlay = Some(Overlay::GroupStyle { group: other });
    ok(&mut app, "group-delete", &json!({"group": other}));
    assert!(app.overlay.is_none());

    let third = new_group(&mut app, "third");
    app.overlay = Some(Overlay::Rename { target: Target::Group(third), input: "x".into() });
    ok(&mut app, "group-delete", &json!({"group": third}));
    assert!(app.overlay.is_none());
}

#[rstest]
#[case::unknown(json!({"group": 999}), "not-found")]
#[case::no_group(json!({}), "malformed")]
fn a_group_to_delete_must_exist(#[case] args: Value, #[case] code: &str) {
    let (mut app, _rx) = app();
    new_group(&mut app, "work");

    let refused = refused(&mut app, "group-delete", &args);

    assert_eq!((refused.code.as_str(), app.groups.len()), (code, 1));
}

#[test]
fn closing_a_group_says_how_to_delete_it() {
    let (mut app, _rx) = app();
    let group = new_group(&mut app, "work");

    let refused = refused(&mut app, "close", &json!({"item": {"group": group}}));

    assert_eq!(refused.code, "failed");
    assert!(refused.message.contains(&format!("cornercase group delete {group}")), "{}", refused.message);
}
