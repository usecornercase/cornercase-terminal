use super::*;

fn reviewer() -> Named {
    Named { id: "reviewer".into(), name: "Code Reviewer".into(), emoji: "👁️".into() }
}

fn with_library() -> Dialog {
    let mut d = editing();
    d.agents.personas = vec![reviewer()];
    let agents = d.agents.clone();
    edit_mut(&mut d).show_tab(2, &agents);
    d
}

fn persona_row(d: &Dialog) -> RowText {
    let i = edit(d).rows().iter().position(|row| *row == Row::Persona).expect("a persona row");
    edit(d).row_texts(&d.agents).remove(i)
}

#[test]
fn every_role_and_the_orchestrator_have_a_persona_row_that_starts_at_none() {
    let mut d = with_library();
    assert_eq!(persona_row(&d).value, "none");
    let agents = d.agents.clone();
    edit_mut(&mut d).show_tab(1, &agents);
    assert!(edit(&d).rows().contains(&Row::Persona));
}

#[test]
fn the_pick_lists_none_and_the_library_by_emoji_and_name_and_sets_the_id() {
    let mut d = with_library();
    click_row(&mut d, Row::Persona);

    let labels: Vec<String> = edit(&d).pick_choices().iter().map(|choice| choice.label.clone()).collect();
    assert_eq!(labels, ["none", "👁️ Code Reviewer"]);
    d.hit(Hit::Pick(1), ROOM);

    let role = edit(&d).role().expect("a role");
    assert_eq!(role.persona.as_deref(), Some("reviewer"));
    assert_eq!((persona_row(&d).value.as_str(), persona_row(&d).red), ("👁️ Code Reviewer", false));
    click_row(&mut d, Row::Persona);
    d.hit(Hit::Pick(0), ROOM);
    assert_eq!(edit(&d).role().expect("a role").persona, None);
}

#[test]
fn a_persona_you_lack_is_red_and_a_problem_to_fix() {
    let mut d = with_library();
    edit_mut(&mut d).flow.roles[0].persona = Some("gone".into());

    let row = persona_row(&d);

    assert_eq!((row.value.as_str(), row.note.as_str(), row.red), ("gone", "not among your personas", true));
    assert!(d.warnings().iter().any(|w| w.contains("`gone`, which is not among your personas")), "{:?}", d.warnings());
    assert_eq!(d.save(), Action::None);
}
