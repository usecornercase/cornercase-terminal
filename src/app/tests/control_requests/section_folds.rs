use rstest::rstest;

use super::*;
use crate::state::Section;

fn folded(app: &App) -> Vec<Section> {
    app.screen.sections.listed()
}

#[rstest]
#[case::projects("projects", Section::Projects)]
#[case::workspaces("workspaces", Section::Workspaces)]
#[case::agents("agents", Section::Agents)]
fn collapse_and_expand_set_a_section_whatever_it_was(#[case] name: &str, #[case] section: Section) {
    let (mut app, _rx) = empty_app();

    for _ in 0..2 {
        ok(&mut app, "collapse", &json!({"section": name}));
    }
    assert_eq!(folded(&app), [section]);
    for _ in 0..2 {
        ok(&mut app, "expand", &json!({"section": name}));
    }

    assert_eq!(folded(&app), []);
}

#[rstest]
#[case::an_item_and_a_section(json!({"item": {"group": 1}, "section": "agents"}), "an item or a section, not both")]
#[case::neither(json!({}), "needs an item or a section")]
#[case::an_unknown_section(json!({"section": "tabs"}), "cannot read this `collapse` request")]
fn a_fold_that_does_not_name_one_thing_is_malformed(#[case] args: Value, #[case] says: &str) {
    let (mut app, _rx) = empty_app();

    let refused = refused(&mut app, "collapse", &args);

    assert_eq!(refused.code, "malformed", "{}", refused.message);
    assert!(refused.message.contains(says), "{}", refused.message);
    assert_eq!(folded(&app), []);
}

type Build = fn(u64) -> Value;

#[rstest]
#[case::a_section(|_| json!({"item": null, "section": "agents"}))]
#[case::an_item(|group| json!({"item": {"group": group}, "section": null}))]
fn a_null_for_the_half_left_out_passes_in_the_structured_mode(#[case] args: Build) {
    let (mut app, _rx) = empty_app();
    let group = done(Response::Ok(ok(&mut app, "group-new", &json!({"name": "work"})))).group.expect("the group");

    ok(&mut app, "collapse", &args(group));
}

#[test]
fn the_status_lists_the_collapsed_sections_in_the_sidebars_order() {
    let (mut app, _rx) = empty_app();
    assert!(ok(&mut app, "status", &json!({})).get("collapsed_sections").is_none());

    for section in ["agents", "projects"] {
        ok(&mut app, "collapse", &json!({"section": section}));
    }

    assert_eq!(app.report(None).collapsed_sections, [Section::Projects, Section::Agents]);
    assert_eq!(ok(&mut app, "status", &json!({}))["collapsed_sections"], json!(["projects", "agents"]));
}

#[test]
fn a_collapse_is_saved_in_the_session() {
    let (mut app, _rx) = empty_app();

    ok(&mut app, "collapse", &json!({"section": "workspaces"}));

    assert_eq!(app.state().collapsed_sections, [Section::Workspaces]);
}
